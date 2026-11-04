use embassy_rp::{
    flash::{Async, Flash},
    peripherals::FLASH,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};

//as described in lib.rs, basically this is here to generate a lookup table and optimize the pid parameters
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PidConfig {
    kp: f32,
    ki: f32,
    kd: f32,
}

impl PidConfig {
    pub fn load(&mut self) -> Self {
        todo!();
    }
    pub fn save(
        &self,
        flash_mut: &Mutex<
            CriticalSectionRawMutex,
            Flash<'static, FLASH, Async, { 2 * 1024 * 1024 }>,
        >,
    ) {
        let bytes = unsafe {
            core::slice::from_raw_parts(
                (settings as *const MySettings) as *const u8,
                core::mem::size_of::<MySettings>(),
            )
        };

        // 1. Flash memory must be erased before writing.
        // Erase a full 4KB sector containing our STORAGE_OFFSET.
        flash
            .erase(STORAGE_OFFSET, STORAGE_OFFSET + SECTOR_SIZE)
            .await?;

        // 2. Write the bytes to the flash offset
        flash.write(STORAGE_OFFSET, bytes).await?;
    }
}
