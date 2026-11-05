use bytemuck::{Pod, Zeroable};
use embassy_rp::{
    flash::{Async, Flash},
    peripherals::{FLASH, PIN_7},
    Peri,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};

use crate::{CONFIG_START, FLASH_MUT_SIZE};

//as described in lib.rs, basically this is here to generate a lookup table and optimize the pid parameters
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PidConfig {
    pub kp: f32,
    pub ki: f32,
    pub kd: f32,
    pub lookup_table_x: [u16; 24],
    pub lookup_table_y: [u16; 24],
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
            flash_check: FLASH_CHECK,
        }
    }
}

const SECTOR_SIZE: u32 = 2 * 1024 * 1024; //Size allocated for this struct
const STORAGE_OFFSET: u32 = 0; //offset from the CONFIG_START
const FLASH_CHECK: [u8; 8] = *b"PidConfi";

impl PidConfig {
    pub async fn autotune(&mut self) {
        todo!();
    }
    pub async fn load(
        &self,
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
    pub async fn save(
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
#[embassy_executor::task]
pub async fn atune_btn_run(autotune_button_p: Peri<'static, PIN_7>) {
    todo!();
}
