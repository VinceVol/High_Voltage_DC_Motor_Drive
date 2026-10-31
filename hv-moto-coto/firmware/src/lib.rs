#![no_std]

use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal, watch::Watch,
};

use crate::{cap_precharge::PrechargeState, direction::Direction, safety::ControllerCond};

pub mod bus_init;
pub mod motor;

//Rpms coms
pub mod speed_sense;
pub static MAX_SPINDLE_SPEED: u16 = 5000; //This should match the linuxCNC config so they scale the same
pub static RPM_BFR_DIR_SWTCH: u16 = 100;
pub static RPM_WATCH: Watch<CriticalSectionRawMutex, u16, 100> = Watch::new(); //The real rpm uploads here

//PWM coms to gate driver -- ie directly related to motor speed
pub mod pwm;
pub static PWM_SIG: Signal<CriticalSectionRawMutex, u16> = Signal::new();

//Direction coms
pub mod direction;
pub static DIR_WATCH: Watch<CriticalSectionRawMutex, Direction, 5> = Watch::new(); //The direction set by the motor loop for others to act on

//Safety Coms
pub mod safety;
pub static SAFETY_WATCH: Watch<CriticalSectionRawMutex, ControllerCond, 5> = Watch::new();

//--------------------------------------------------------------------------
//ADC Pins -- need to be checked together -- Current/Voltage/mesa stpt sense
pub mod adc_runner;
// Current sense
pub static I_WATCH: Watch<CriticalSectionRawMutex, u16, 5> = Watch::new(); //Current Sense
pub static I_CORR_NUM: u64 = 40; //Correction factor for the current sense/ tune on bench
pub static I_DEL_MS: u64 = 500; //how often should we even check the current

//Voltage sense
pub static V_WATCH: Watch<CriticalSectionRawMutex, u16, 5> = Watch::new(); //Voltage Readout of caps
pub static V_CORR_NUM: i64 = 683; //Correction factor for the voltage sense/ tune on bench
pub static V_CORR_OFF: i64 = -300; //Votlage offset from zero in the current sense loop
pub static V_DEL_MS: u64 = 500; //how often should we even check the voltage

//Mesa coms
pub mod mesa;
pub static CHANGE_THRESHOLD: i16 = 150; //Assuming the voltage output is imperfect out of the mesa, what's the threshold
pub static MESA_MON_DEL_MS: u64 = 500; //how often should we even check the mesa board outputs
pub static MESA_RPM_REQ_SIG: Signal<CriticalSectionRawMutex, u16> = Signal::new(); //Voltage Readout of caps
pub static MESA_DIR_REQ_SIG: Signal<CriticalSectionRawMutex, Direction> = Signal::new(); //Voltage Readout of caps
                                                                                         //--------------------------------------------------------------------------

//Cap precharge
pub mod cap_precharge;
pub static V_PRECH_THRESH: u16 = 5; //TODO Testing things at a low voltage for now so 5 will need to be changed TODO
pub static CAP_PRE_STATE_SIG: Signal<CriticalSectionRawMutex, PrechargeState> = Signal::new();
