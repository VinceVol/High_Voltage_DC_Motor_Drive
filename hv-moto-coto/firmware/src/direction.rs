//Direction control of the motor

use embassy_rp::{
    gpio::{Level, Output},
    peripherals::{PIN_16, PIN_21},
    Peri,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};

pub static DIR_CHANNEL: Channel<CriticalSectionRawMutex, bool, 25> = Channel::new();

//Not sure if I should really do anything more than this for the dir_pin
#[embassy_executor::task]
pub async fn run_dir(dir_a_pin_p: Peri<'static, PIN_21>, dir_b_pin_p: Peri<'static, PIN_16>) {
    let mut dir_pin_a = Output::new(dir_a_pin_p, Level::Low);
    let mut dir_pin_b = Output::new(dir_b_pin_p, Level::Low);
    loop {
        let dir = DIR_CHANNEL.receive().await;
        //TODO CONFIRM RPMS ARE LOW BEFORE SWAP
        if dir {
            dir_pin_b.set_low();
            dir_pin_a.set_high();
        } else {
            dir_pin_a.set_low();
            dir_pin_b.set_high();
        }
    }
}
