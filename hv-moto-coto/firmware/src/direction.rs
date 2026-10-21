//Direction control of the motor

use embassy_rp::{
    gpio::{Level, Output},
    peripherals::{PIN_16, PIN_21},
    Peri,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Receiver, Sender},
};
use embassy_time::{Duration, Timer};

use crate::{DIR_WATCH, RPM_BFR_DIR_SWTCH};

#[derive(Clone, Copy)]
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
) {
    let mut dir_pin_a = Output::new(dir_a_pin_p, Level::Low);
    let mut dir_pin_b = Output::new(dir_b_pin_p, Level::Low);
    loop {
        let dir = DIR_WATCH.receiver().unwrap().get().await; //Unwraping as I should be within the Watch max cnt

        //CONFIRM RPMS ARE LOW BEFORE SWAP
        while rpm_receiver.receive().await > RPM_BFR_DIR_SWTCH {}

        //Currently no checks on whether we are switching to a direction we're already in
        // downside is the motor is going to shut down for no reason if this is the case
        // should be an avoidable bug??
        // TODO make sure the pwm signal is off for both sides before swapping leads -- probably happening in motor loop
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
