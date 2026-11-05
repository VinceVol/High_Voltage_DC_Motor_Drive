use bytemuck::{Pod, Zeroable};
use embassy_rp::{
    flash::{Async, Flash},
    gpio::Input,
    peripherals::{FLASH, PIN_7},
    Peri,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, watch::Receiver};
use embassy_time::{Duration, Timer};

use crate::{
    AUTOTUNE_OVERRIDE, CONFIG_START, FLASH_MUT_SIZE, MAX_SPINDLE_SPEED, PWM_WATCH, RPM_REQ_SIG,
    RPM_WATCH,
};

//as described in lib.rs, basically this is here to generate a lookup table and optimize the pid parameters
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PidConfig {
    pub kp: f32,
    pub ki: f32,
    pub kd: f32,
    pub lookup_table_x: [u16; 24],
    pub lookup_table_y: [u16; 24],
    pub pwm_floor: i32,
    flash_check: [u8; 8],
}

impl Default for PidConfig {
    fn default() -> Self {
        Self {
            kp: 0.35,
            ki: 0.2,
            kd: 0.0,
            lookup_table_x: [0; 24],
            lookup_table_y: [0; 24],
            pwm_floor: 200,
            flash_check: FLASH_CHECK,
        }
    }
}

const SECTOR_SIZE: u32 = 2 * 1024 * 1024; //Size allocated for this struct
const STORAGE_OFFSET: u32 = 0; //offset from the CONFIG_START
const FLASH_CHECK: [u8; 8] = *b"PidConfi";

impl PidConfig {
    pub async fn run_thy_tune<'a>(
        flash_mut: &Mutex<
            CriticalSectionRawMutex,
            Flash<'static, FLASH, Async, { FLASH_MUT_SIZE }>,
        >,
        rpm_watch: &mut Receiver<'a, CriticalSectionRawMutex, u16, 100>,
        pwm_watch: &mut Receiver<'a, CriticalSectionRawMutex, u16, 5>,
    ) -> Self {
        //Tell the motor req signals to ignore mesa inputs
        let tune_sender = AUTOTUNE_OVERRIDE.sender();
        tune_sender.send(true);

        //Start with the current parameters TODO manage to unwrap TODO
        let mut the_tune = PidConfig::load(flash_mut).await.unwrap();

        //Figure out the stall pwm -- pwm_floor
        the_tune.find_floor(rpm_watch, pwm_watch).await;

        //Save the tune TODO manage the unwrap TODO
        the_tune.save(flash_mut).await.unwrap();
        tune_sender.send(false);
        todo!();
    }

    //Cycle through the bottom quarter of available RPMS to find when the motor stalls
    async fn find_floor<'a>(
        &mut self,
        rpm_watch: &mut Receiver<'a, CriticalSectionRawMutex, u16, 100>,
        pwm_watch: &mut Receiver<'a, CriticalSectionRawMutex, u16, 5>,
    ) {
        let mut last_working_pwm = 999; //Max pwm
        let mut new_rpm;
        for rpm in (0..100)
            .rev()
            .map(|i| (MAX_SPINDLE_SPEED as f64 / 4.0 * i as f64 / 99.0) as u16)
        {
            RPM_REQ_SIG.signal(rpm);
            Timer::after(Duration::from_millis(300)).await; //short pause to allow controller to react

            loop {
                new_rpm = rpm_watch.changed().await;

                //if the read RPM is within 10 of the stpnt then set the pwm accordingly
                if new_rpm + 10 > rpm && new_rpm - 10 < rpm {
                    last_working_pwm = pwm_watch.changed().await;
                } else if new_rpm <= 20 {
                    self.pwm_floor = last_working_pwm as i32;
                    return;
                }
            }
        }
    }

    //Load up the config perameters from flash
    pub async fn load(
        flash_mut: &Mutex<
            CriticalSectionRawMutex,
            Flash<'static, FLASH, Async, { FLASH_MUT_SIZE }>,
        >,
    ) -> Result<Self, embassy_rp::flash::Error> {
        let mut flash = flash_mut.lock().await;

        // Allocate a temporary byte buffer matching the size of our struct
        let mut buffer = [0u8; core::mem::size_of::<PidConfig>()];

        //CONFIG_START is unsafe
        unsafe {
            // Read directly from flash into our byte buffer
            flash
                .read(CONFIG_START + STORAGE_OFFSET, &mut buffer)
                .await?;
        }

        //Check if the sector is completely uninitialized (erased flash is all 0xFF)
        if buffer.iter().all(|&b| b == 0xFF) {
            return Ok(PidConfig::default()); // or your initial default values
        }

        //Safe to cast via bytemuck now that we know it's not empty
        let pid_config = *bytemuck::from_bytes::<PidConfig>(&buffer);
        if pid_config.flash_check != FLASH_CHECK {
            return Ok(PidConfig::default());
        }

        //if it wasn't all zeros and the flash check is valid then open the config
        return Ok(pid_config);
    }
    async fn save(
        &self,
        flash_mut: &Mutex<
            CriticalSectionRawMutex,
            Flash<'static, FLASH, Async, { FLASH_MUT_SIZE }>,
        >,
    ) -> Result<(), embassy_rp::flash::Error> {
        let mut flash = flash_mut.lock().await;

        // Convert struct reference to a &[u8] safely using bytemuck
        let bytes = bytemuck::bytes_of(self);

        // Unsafe as CONFIG START is in a unsafe block
        unsafe {
            // 1. Flash memory must be erased before writing.
            // Erase a full 2KB sector containing our STORAGE_OFFSET.
            flash.blocking_erase(
                CONFIG_START + STORAGE_OFFSET,
                CONFIG_START + STORAGE_OFFSET + SECTOR_SIZE,
            )?;

            // 2. Write the bytes to the flash offset
            flash.blocking_write(CONFIG_START + STORAGE_OFFSET, bytes)?;
        }

        Ok(())
    }
}

//task reacting to whether the button for enabling autotune is depressed or not
//use debounce to avoid noise on trigger
const DEBOUNCE_THRESHOLD_US: u64 = 2000;
#[embassy_executor::task]
pub async fn atune_btn_run(
    autotune_button_p: Peri<'static, PIN_7>,
    flash_mut: &'static Mutex<
        CriticalSectionRawMutex,
        Flash<'static, FLASH, Async, { FLASH_MUT_SIZE }>,
    >,
) {
    let mut atune_btn = Input::new(autotune_button_p, embassy_rp::gpio::Pull::Down);
    let mut rpm_watch = RPM_WATCH.receiver().unwrap(); //Direct feed from the hall sensor
    let mut pwm_watch = PWM_WATCH.receiver().unwrap(); //PWM feed

    //watch for a button press and then fire off the autotune fn
    loop {
        atune_btn.wait_for_rising_edge().await;
        Timer::after_micros(DEBOUNCE_THRESHOLD_US).await;
        PidConfig::run_thy_tune(flash_mut, &mut rpm_watch, &mut pwm_watch).await;
    }
}
