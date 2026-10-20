#![no_std]

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};

use crate::{direction::Direction, safety::ControllerCond};

pub mod bus_init;
pub mod direction;
pub mod motor;
pub mod pwm;
pub mod safety;
pub mod speed_sense;

//Rpms coms
pub static RPM_BFR_DIR_SWTCH: u16 = 100;
pub static RPM_REQ_CHANNEL: Channel<CriticalSectionRawMutex, u16, 100> = Channel::new(); //The rpm we want drops into here
pub static RPM_CHANNEL: Channel<CriticalSectionRawMutex, u16, 100> = Channel::new(); //The real rpm uploads here

//PWM coms to gate driver -- ie directly related to motor speed
pub static PWM_CHANNEL: Channel<CriticalSectionRawMutex, u16, 100> = Channel::new();

//Direction coms
pub static DIR_CHANNEL: Channel<CriticalSectionRawMutex, Direction, 25> = Channel::new();

//Safety Coms
pub static SAFETY_CHANNEL: Channel<CriticalSectionRawMutex, ControllerCond, 25> = Channel::new();
pub static I_CHANNEL: Channel<CriticalSectionRawMutex, u16, 100> = Channel::new(); //Current Sense
pub static V_CHANNEL: Channel<CriticalSectionRawMutex, u16, 100> = Channel::new(); //Voltage Readout of caps
