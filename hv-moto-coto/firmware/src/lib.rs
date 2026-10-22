#![no_std]

use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal, watch::Watch,
};

use crate::{direction::Direction, safety::ControllerCond};

pub mod bus_init;
pub mod direction;
pub mod mesa;
pub mod motor;
pub mod pwm;
pub mod safety;
pub mod speed_sense;

//Rpms coms
pub static MAX_SPINDLE_SPEED: u16 = 5000; //This should match the linuxCNC config so they scale the same
pub static RPM_BFR_DIR_SWTCH: u16 = 100;
// pub static RPM_REQ_CHANNEL: Channel<CriticalSectionRawMutex, u16, 100> = Channel::new(); //The rpm we want drops into here
pub static RPM_WATCH: Watch<CriticalSectionRawMutex, u16, 100> = Watch::new(); //The real rpm uploads here

//PWM coms to gate driver -- ie directly related to motor speed
pub static PWM_SIG: Signal<CriticalSectionRawMutex, u16> = Signal::new();

//Direction coms
pub static DIR_WATCH: Watch<CriticalSectionRawMutex, Direction, 5> = Watch::new(); //The direction set by the motor loop for others to act on

//Safety Coms
pub static SAFETY_WATCH: Watch<CriticalSectionRawMutex, ControllerCond, 5> = Watch::new();
pub static I_CHANNEL: Channel<CriticalSectionRawMutex, u16, 100> = Channel::new(); //Current Sense
pub static V_CHANNEL: Channel<CriticalSectionRawMutex, u16, 100> = Channel::new(); //Voltage Readout of caps

//Mesa coms
pub static CHANGE_THRESHOLD: i16 = 150; //Assuming the voltage output is imperfect out of the mesa, what's the threshold
pub static MESA_MON_DEL_MS: u64 = 500; //how often should we even check the mesa board outputs
pub static MESA_RPM_REQ_SIG: Signal<CriticalSectionRawMutex, u16> = Signal::new(); //Voltage Readout of caps
pub static MESA_DIR_REQ_SIG: Signal<CriticalSectionRawMutex, Direction> = Signal::new(); //Voltage Readout of caps
