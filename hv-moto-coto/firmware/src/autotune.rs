use core::ptr;

use bytemuck::{Pod, Zeroable};
use defmt::info;
use embassy_rp::{
    flash::{Async, Flash},
    gpio::Input,
    peripherals::{FLASH, PIN_7},
    Peri,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    mutex::Mutex,
    watch::{Receiver, Sender},
};
use embassy_time::{Duration, Instant, Timer};

use crate::{
    AUTOTUNE_OVERRIDE, CONFIG_START, FLASH_MUT_SIZE, FLOOR_TBL_CNT, MOT_LOOP_EN, PI_TBL_CNT,
    PWM_WATCH, RPM_FILT_WATCH, RPM_RAW_WATCH,
};

//as described in lib.rs, basically this is here to generate a lookup table and optimize the pid parameters
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PidConfig {
    pub floor_table_rpm: [u16; FLOOR_TBL_CNT],
    pub floor_table_pwm: [u16; FLOOR_TBL_CNT],
    pub pi_table_rpm: [u16; PI_TBL_CNT], //rpm setpoint
    pub pi_table_p: [f32; PI_TBL_CNT],   //p components of PID
    pub pi_table_i: [f32; PI_TBL_CNT],   //i components of PID
    flash_check: [u8; 8],
}

impl Default for PidConfig {
    fn default() -> Self {
        Self {
            floor_table_rpm: [0; FLOOR_TBL_CNT],
            floor_table_pwm: [0; FLOOR_TBL_CNT],
            pi_table_rpm: [0; PI_TBL_CNT],
            pi_table_p: [0.35; PI_TBL_CNT],
            pi_table_i: [0.2; PI_TBL_CNT],
            flash_check: FLASH_CHECK,
        }
    }
}

const SECTOR_SIZE: u32 = 4 * 1024; //Size allocated for this struct
const STORAGE_OFFSET: u32 = 0; //offset from the CONFIG_START
const FLASH_CHECK: [u8; 8] = *b"PidConfi";

impl PidConfig {
    pub async fn run_thy_tune<'a>(
        flash_mut: &Mutex<
            CriticalSectionRawMutex,
            Flash<'static, FLASH, Async, { FLASH_MUT_SIZE }>,
        >,
        rpm_filt_watch: &mut Receiver<'a, CriticalSectionRawMutex, u16, 100>,
        rpm_raw_watch: &mut Receiver<'a, CriticalSectionRawMutex, u16, 100>,
    ) -> Self {
        //Tell the motor req signals to ignore mesa inputs
        let tune_sender = AUTOTUNE_OVERRIDE.sender();
        tune_sender.send(true);

        //Start with the current parameters TODO manage to unwrap TODO
        let mut the_tune = PidConfig::load(flash_mut).await.unwrap();

        //Generate the lookup tables by slowly incrementing pwm
        the_tune.gen_floor_tbl(rpm_filt_watch).await;

        //Generate PID lookup tbl
        the_tune.gen_pid_tbl(rpm_raw_watch).await;

        //Save the tune TODO manage the unwrap TODO
        the_tune.save(flash_mut).await.unwrap();
        tune_sender.send(false);
        the_tune
    }

    async fn gen_floor_tbl<'a>(
        &mut self,
        rpm_watch: &mut Receiver<'a, CriticalSectionRawMutex, u16, 100>,
    ) {
        //disable pid controlled motor loop
        MOT_LOOP_EN.signal(false);
        Timer::after_secs(1).await; // give the motor loop time to fully shut down

        //going to be checking new vs old rpm to know when the motor has settled
        let mut new_rpm;
        let mut old_rpm = 0;

        //rip through divisions of FLOOR_TBL_CNT pwm signals
        for (i, pwm) in (0..FLOOR_TBL_CNT)
            .map(|i| (1000 as f64 * i as f64 / (FLOOR_TBL_CNT as f64 - 1.0)) as u16)
            .enumerate()
        {
            PWM_WATCH.sender().send(pwm);
            Timer::after(Duration::from_millis(50)).await; //short pause to allow controller to react

            loop {
                new_rpm = rpm_watch.changed().await;

                //Essentially fire when the rpm starts to be constant I prefer checking two in a row over a delay
                if (new_rpm as i32 - old_rpm as i32).abs() < 10 {
                    self.floor_table_rpm[i] = new_rpm;
                    self.floor_table_pwm[i] = pwm;
                    break;
                }

                old_rpm = new_rpm;
            }
        }
        MOT_LOOP_EN.signal(true);
    }

    async fn gen_pid_tbl<'a>(
        &mut self,
        rpm_raw_watch: &mut Receiver<'a, CriticalSectionRawMutex, u16, 100>,
    ) {
        //disable pid controlled motor loop
        MOT_LOOP_EN.signal(false);
        Timer::after_secs(1).await; // give the motor loop time to fully shut down

        //going to be checking new vs old rpm to know when the motor has settled
        let mut new_rpm: i32;
        let mut old_rpm: i32 = 0;

        //Save the times, pwms and rpms into arrays to feed to a core logic fn that
        // will calculate the P & I terms
        let mut times: [u64; 2] = [0, 0]; //times surrounding the 0.6321K time constant
        let mut rpms: [u64; 2] = [0, 0]; //rpms surrounding that same item -- interpolation needed
        let mut pwm_start_finish: [u16; 2] = [0, 0]; //the setpoint start finish
        let mut rpm_start_finish: [u64; 2] = [0, 0]; //resulting steady state rpms

        //start pwm at zero and give the motor sufficient time to settle down
        PWM_WATCH.sender().send(0);
        Timer::after_secs(3).await;

        //rip through divisions of PI_TBL_CNT pwm signals
        for (i, pwm) in (0..PI_TBL_CNT)
            .map(|i| (1000 as f64 * i as f64 / (PI_TBL_CNT as f64 - 1.0)) as u16)
            .enumerate()
        {
            //move over rpm to make room for the finish (the last one's finish is the new ones start)
            rpm_start_finish.rotate_left(1);

            //find the other end of the rpm range
            loop {
                new_rpm = rpm_raw_watch.changed().await as i32;

                //Essentially fire when the rpm starts to be constant I prefer checking two in a row over a delay
                if (new_rpm as i32 - old_rpm as i32).abs() < 10 {
                    rpm_start_finish[1] = new_rpm as u64;
                    break;
                }

                old_rpm = new_rpm;
            }

            //Set the pwm range
            pwm_start_finish.rotate_left(1);
            pwm_start_finish[1] = pwm;

            //rotating the times and rpms as we only want surrounding data of the 0.6321 RPM stpnt
            times.rotate_left(1);
            rpms.rotate_left(1);

            //Set the rpm goal
            let rpm_goal =
                (6321 * (rpm_start_finish[1] - rpm_start_finish[0])) / 10000 + rpm_start_finish[0];

            //Tell mcu to pwm to the finish line and start the timer
            let timer = Instant::now();
            PWM_WATCH.sender().send(pwm);
            loop {
                rpms.rotate_left(1); //move old rpm to first slot
                times.rotate_left(1); //move old time to first slot

                //Once rpm updates update the right slot with the new values
                rpms[1] = rpm_raw_watch.changed().await as u64;
                times[1] = timer.elapsed().as_micros();

                if rpms[0] <= rpm_goal && rpms[1] >= rpm_goal {
                    //TODO Core logic goes here TODO
                    break;
                }
            }
        }
        MOT_LOOP_EN.signal(true);
    }

    //Load up the config perameters from flash
    pub async fn load(
        flash_mut: &Mutex<
            CriticalSectionRawMutex,
            Flash<'static, FLASH, Async, { FLASH_MUT_SIZE }>,
        >,
    ) -> Result<Self, embassy_rp::flash::Error> {
        let mut flash = flash_mut.lock().await;
        let storage_location = (ptr::addr_of!(CONFIG_START) as u32) - 0x10000000 + STORAGE_OFFSET;

        // Allocate a temporary byte buffer matching the size of our struct
        let mut buffer = [0u8; core::mem::size_of::<PidConfig>()];

        // Read directly from flash into our byte buffer
        flash.read(storage_location, &mut buffer).await?;

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
        let storage_location = (ptr::addr_of!(CONFIG_START) as u32) - 0x10000000 + STORAGE_OFFSET;

        // Convert struct reference to a &[u8] safely using bytemuck
        let bytes = bytemuck::bytes_of(self);

        // 1. Flash memory must be erased before writing.
        // Erase a full 2KB sector containing our STORAGE_OFFSET.
        flash.blocking_erase(storage_location, storage_location + SECTOR_SIZE)?;

        // 2. Write the bytes to the flash offset
        flash.blocking_write(storage_location, bytes)?;

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
    let mut rpm_filt_watch = RPM_FILT_WATCH.receiver().unwrap(); //Direct feed from the hall sensor
    let mut rpm_raw_watch = RPM_RAW_WATCH.receiver().unwrap(); //Direct feed from the hall sensor

    //watch for a button press and then fire off the autotune fn
    loop {
        atune_btn.wait_for_rising_edge().await;
        Timer::after_micros(DEBOUNCE_THRESHOLD_US).await;
        PidConfig::run_thy_tune(flash_mut, &mut rpm_filt_watch, &mut rpm_raw_watch).await;
    }
}
