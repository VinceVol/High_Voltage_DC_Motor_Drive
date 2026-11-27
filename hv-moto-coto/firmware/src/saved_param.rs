use bytemuck::{Pod, Zeroable};
use core::ptr;
use embassy_rp::{
    flash::{Async, Flash},
    peripherals::FLASH,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};

use crate::{CONFIG_START, FLASH_MUT_SIZE, FLOOR_TBL_CNT};

//The number of things needed to save on the flash are growing so this is here to give a way to access and
// save new information
//
//Repr(C) and Pod/ Zeroable needed to actually save on the flash
// things will fight with Pod if you dont keep an even number
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ControllerConfig {
    pub floor_table_rpm: [u16; FLOOR_TBL_CNT],
    pub floor_table_pwm: [u16; FLOOR_TBL_CNT],
    pub rpm_kp: f32, //PID kP for rpm (outer) loop
    pub rpm_ki: f32, //PID ki for rpm (outer) loop
    pub i_kp: f32,   //PID kp for current (inner) loop
    pub i_ki: f32,   //PID ki for current (inner) loop
    pub max_rpm: u16,
    pub max_current_ma: u16,
    pub man_mod: u8,
    flash_check: [u8; 8],
    _pad: [u8; 3],
}

const FLASH_CHECK: [u8; 8] = *b"PidConfi";
const SECTOR_SIZE: u32 = 4 * 1024; //Size allocated for this struct
const STORAGE_OFFSET: u32 = 0; //offset from the CONFIG_START

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            floor_table_rpm: [0; FLOOR_TBL_CNT],
            floor_table_pwm: [0; FLOOR_TBL_CNT],
            rpm_kp: 0.35,
            rpm_ki: 0.2,
            i_kp: 0.35,
            i_ki: 0.2,
            man_mod: 0,
            max_rpm: 2000,
            max_current_ma: 1000,
            flash_check: FLASH_CHECK,
            _pad: [0, 0, 0],
        }
    }
}

impl ControllerConfig {
    pub async fn init_atomics(
        flash_mut: &Mutex<
            CriticalSectionRawMutex,
            Flash<'static, FLASH, Async, { FLASH_MUT_SIZE }>,
        >,
    ) {
        todo!();
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
        let mut buffer = [0u8; core::mem::size_of::<ControllerConfig>()];

        // Read directly from flash into our byte buffer
        flash.read(storage_location, &mut buffer).await?;

        //Check if the sector is completely uninitialized (erased flash is all 0xFF)
        if buffer.iter().all(|&b| b == 0xFF) {
            return Ok(ControllerConfig::default()); // or your initial default values
        }

        //Safe to cast via bytemuck now that we know it's not empty
        let pid_config = *bytemuck::from_bytes::<ControllerConfig>(&buffer);
        if pid_config.flash_check != FLASH_CHECK {
            return Ok(ControllerConfig::default());
        }

        //if it wasn't all zeros and the flash check is valid then open the config
        return Ok(pid_config);
    }
    pub async fn save(
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
