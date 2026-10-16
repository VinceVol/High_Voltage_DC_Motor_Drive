#![no_std]

pub mod bus_init;
pub mod direction;
pub mod pwm;
pub mod speed_sense;

pub static RPM_BFR_DIR_SWTCH: u16 = 100;
