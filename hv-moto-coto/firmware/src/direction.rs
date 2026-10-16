//Direction control of the motor

use embassy_rp::{
    gpio::{Level, Output},
    peripherals::{PIN_16, PIN_21},
    Peri,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Channel, Receiver, Sender},
};
use embassy_time::{Duration, Timer};

use crate::RPM_BFR_DIR_SWTCH;

pub static DIR_CHANNEL: Channel<CriticalSectionRawMutex, Direction, 25> = Channel::new();

pub enum Direction {
    Forward,
    Reverse,
}
//Not sure if I should really do anything more than this for the dir_pin
#[embassy_executor::task]
pub async fn run_dir(
    dir_a_pin_p: Peri<'static, PIN_21>,
    dir_b_pin_p: Peri<'static, PIN_16>,
    rpm_receiver: Receiver<'static, CriticalSectionRawMutex, u16, 100>,
    rpm_sender: Sender<'static, CriticalSectionRawMutex, u16, 100>,
) {
    let mut dir_pin_a = Output::new(dir_a_pin_p, Level::Low);
    let mut dir_pin_b = Output::new(dir_b_pin_p, Level::Low);
    loop {
        let dir = DIR_CHANNEL.receive().await;

        //CONFIRM RPMS ARE LOW BEFORE SWAP
        rpm_sender.send(RPM_BFR_DIR_SWTCH).await;
        while rpm_receiver.receive().await > RPM_BFR_DIR_SWTCH {
            Timer::after(Duration::from_millis(100)).await;
        }

        //Currently no checks on whether we are switching to a direction we're already in
        // downside is the motor is going to shut down for no reason if this is the case
        // should be an avoidable bug??
        match dir {
            Direction::Forward => {
                dir_pin_b.set_low();
                dir_pin_a.set_high();
            }
            Direction::Reverse => {
                dir_pin_a.set_low();
                dir_pin_b.set_high();
            }
        }
    }
}
