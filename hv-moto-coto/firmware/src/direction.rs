//Direction control of the motor

use defmt::info;
use embassy_rp::{
    gpio::{Level, Output},
    peripherals::{PIN_16, PIN_21},
    Peri,
};

use crate::DIR_WATCH;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Reverse,
}
//Not sure if I should really do anything more than this for the dir_pin
#[embassy_executor::task]
pub async fn run_dir(dir_a_pin_p: Peri<'static, PIN_21>, dir_b_pin_p: Peri<'static, PIN_16>) {
    let mut dir_pin_a = Output::new(dir_a_pin_p, Level::Low);
    let mut dir_pin_b = Output::new(dir_b_pin_p, Level::Low);
    let mut dir_watch = DIR_WATCH.receiver().unwrap();
    let mut dir;

    loop {
        dir = dir_watch.changed().await;
        info!("got new direction!");

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
