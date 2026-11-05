#![no_std]

use core::sync::atomic::{AtomicI32, AtomicU16};

use embassy_rp::{
    flash::{Async, Flash},
    peripherals::FLASH,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, mutex::Mutex, signal::Signal,
    watch::Watch,
};
use static_cell::StaticCell;

use crate::{
    autotune::PidConfig, cap_precharge::PrechargeState, direction::Direction,
    safety::ControllerCond,
};

pub mod bus_init;

//motor loop driven by req signals
pub mod motor;
pub static RPM_REQ_SIG: Signal<CriticalSectionRawMutex, u16> = Signal::new(); //Voltage Readout of caps
pub static DIR_REQ_SIG: Signal<CriticalSectionRawMutex, Direction> = Signal::new(); //Voltage Readout of caps

//Rpms coms
pub mod speed_sense;
pub static MAX_SPINDLE_SPEED: u16 = 1000; //This should match the linuxCNC config so they scale the same
pub static RPM_BFR_DIR_SWTCH: u16 = 100;
pub static RPM_WATCH: Watch<CriticalSectionRawMutex, u16, 100> = Watch::new(); //The real rpm uploads here
pub static HALL_CNT: usize = 7; //how many samples needed for rpm feedback
pub static CURR_DRAW_THRESH_FOR_ZERO: f32 = 0.02; //The current draw at which we say with some confidence motor is off
pub static SPEED_FLOOR_RPM: u64 = 200; //stall speed, TODO have the autotune set this TODO

//PWM coms to gate driver -- ie directly related to motor speed
pub mod pwm;
pub static PWM_SIG: Signal<CriticalSectionRawMutex, u16> = Signal::new();
pub static PWM_FLOOR: AtomicI32 = AtomicI32::new(200); //pwm to avoid stall without load

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
pub static I_WATCH: Watch<CriticalSectionRawMutex, f32, 5> = Watch::new(); //Current Sense
pub static I_CORR_NUM: f32 = 32.52; //Correction factor for the current sense/ tune on bench
pub static I_CORR_OFF: Mutex<CriticalSectionRawMutex, f32> = Mutex::new(-16.375); //Current offset from zero in the current sense loop. Calibrate when motor off
pub static I_DEL_MS: u64 = 50; //how often should we even check the current

//Voltage sense
pub static V_WATCH: Watch<CriticalSectionRawMutex, i64, 5> = Watch::new(); //Voltage Readout of caps
pub static V_CORR_NUM: i64 = 683; //Correction factor for the voltage sense/ tune on bench
pub static V_CORR_OFF: i64 = -300; //Votlage offset from zero in the current sense loop
pub static V_DEL_MS: u64 = 500; //how often should we even check the voltage

//Mesa coms -- communications from either a mesa board or user buttons (CNC or manual machine)
pub mod mesa;
pub static CHANGE_THRESHOLD: i16 = 150; //Assuming the voltage output is imperfect out of the mesa, what's the threshold
pub static MESA_MON_DEL_MS: u64 = 500; //how often should we even check the mesa board outputs

//--------------------------------------------------------------------------
//Cap precharge -- dont want to damage internals when caps are initially charged so we watch the voltage rise
pub mod cap_precharge;
pub static V_PRECH_THRESH: i64 = 5; //TODO Testing things at a low voltage for now so 5 will need to be changed TODO
pub static CAP_PRE_STATE_SIG: Signal<CriticalSectionRawMutex, PrechargeState> = Signal::new();

//Autotune -- Want to use a lookup table to get close to the requested speed and obviously tune the PID perameters to prevent oscillations but promote fast response
pub mod autotune;
pub static AUTOTUNE_OVERRIDE: Watch<CriticalSectionRawMutex, bool, 5> = Watch::new(); //tells mesa loop to stop requesting direction and rpm

//FLASH --------------------------------------------------------------------
//We created the variable CONFIG_START in the memory.x file to call out the memory location of the PID config
// its unsafe as we're telling rust to trust that CONFIG_START exists
unsafe extern "C" {
    static CONFIG_START: u32;
}
static FLASH_MUT_SIZE: usize = 8 * 1024; //The allocated space for all saved variables

//The flash peripheral initialized at runtime and shared as a mutex
static FLASH_MUT: StaticCell<
    Mutex<CriticalSectionRawMutex, Flash<'static, FLASH, Async, { FLASH_MUT_SIZE }>>,
> = StaticCell::new();
// static PID_CONFIG: Mutex<CriticalSectionRawMutex, PidConfig> = Mutex::new(PidConf);
