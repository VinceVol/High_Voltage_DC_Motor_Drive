#![no_std]

use core::sync::atomic::{AtomicU16, AtomicU32};

use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, signal::Signal, watch::Watch,
};

use crate::{
    cap_precharge::PrechargeState, pwm::Direction, safety::ControllerCond, speed_sense::HallRpm,
};

pub mod bus_init;

//motor loop driven by req signals
pub mod motor;
pub static RPM_REQ_SIG: Signal<CriticalSectionRawMutex, u16> = Signal::new(); //Voltage Readout of caps
pub static DIR_REQ_SIG: Signal<CriticalSectionRawMutex, Direction> = Signal::new(); //Voltage Readout of caps
pub static MOT_LOOP_EN: Signal<CriticalSectionRawMutex, bool> = Signal::new(); //during autotune we dont want motor to own the pwm
pub static I_REF_RATE_US: u64 = (5 * 1000000) / PWM_FREQ; //denom is 20KHZ 5 is measureing i every 5 pwm
pub static RPM_REF_RATE_MS: u64 = 20;

//Rpms coms
pub mod speed_sense;
pub static MAX_SPINDLE_SPEED: u16 = 1800; //This should match the linuxCNC config so they scale the same
pub static RPM_BFR_DIR_SWTCH: u16 = 100;
pub static RPM_FILT_WATCH: Watch<CriticalSectionRawMutex, HallRpm, 100> = Watch::new(); //The real rpm uploads here
pub static HALL_CNT: usize = 4; //how many samples needed for rpm feedback
pub static CURR_DRAW_THRESH_FOR_ZERO: f32 = 0.02; //The current draw at which we say with some confidence motor is off
pub static RPM_RAW_WATCH: Watch<CriticalSectionRawMutex, u16, 100> = Watch::new(); //The real rpm uploads here

//PWM coms to gate driver -- ie directly related to motor speed
pub mod pwm;
pub static PWM_FREQ: u64 = 20000; //20KHZ
pub static PWM_WATCH: Watch<CriticalSectionRawMutex, u16, 5> = Watch::new();
pub static PWM_INT_CNT: AtomicU16 = AtomicU16::new(0);
pub static PWM_HALF_TIME_US: AtomicU32 = AtomicU32::new(0);

//Direction coms
pub static DIR_WATCH: Watch<CriticalSectionRawMutex, Direction, 5> = Watch::new(); //The direction set by the motor loop for others to act on

//Safety Coms
pub mod safety;
pub static SAFETY_WATCH: Watch<CriticalSectionRawMutex, ControllerCond, 5> = Watch::new();

//--------------------------------------------------------------------------
//ADC Pins -- need to be checked together -- Current/Voltage/mesa stpt sense
pub mod adc_runner;
// Current sense
pub static I_WATCH: Watch<CriticalSectionRawMutex, f32, 5> = Watch::new(); //Current Sense
pub static I_CORR_NUM: f32 = 23.777; //Correction factor for the current sense/ tune on bench
pub static I_CORR_OFF: Mutex<CriticalSectionRawMutex, f32> = Mutex::new(-12.0); //Current offset from zero in the current sense loop. Calibrate when motor off
pub static I_DEL_MS: u64 = 50; //how often should we even check the current
pub static I_INTR: Signal<CriticalSectionRawMutex, u16> = Signal::new();
pub mod roll_current;
pub static MAX_CURRENT_AMPS: f32 = 2.0;

//Voltage sense
pub static V_WATCH: Watch<CriticalSectionRawMutex, i64, 5> = Watch::new(); //Voltage Readout of caps
pub static V_CORR_NUM: i64 = 683; //Correction factor for the voltage sense/ tune on bench
pub static V_CORR_OFF: i64 = -300; //Votlage offset from zero in the current sense loop
pub static V_DEL_MS: u64 = 500; //how often should we even check the voltage

//Mesa coms -- communications from either a mesa board or user buttons (CNC or manual machine)
pub mod mesa;
pub static CHANGE_THRESHOLD: i16 = 150; //Assuming the voltage output is imperfect out of the mesa, what's the threshold
pub static MESA_MON_DEL_MS: u64 = 500; //how often should we even check the mesa board outputs

//Autotune -- Want to use a lookup table to get close to the requested speed and obviously tune the PID perameters to prevent oscillations but promote fast response
pub mod autotune;
pub static AUTOTUNE_OVERRIDE: Watch<CriticalSectionRawMutex, bool, 5> = Watch::new(); //tells mesa loop to stop requesting direction and rpm
pub static FLOOR_TBL_CNT: usize = 24; //Number of items in the floor lookup table for pwm to rpm
pub static PI_TBL_CNT: usize = 3; //Number of items in the PID lookup table

//FLASH --------------------------------------------------------------------
//We created the variable CONFIG_START in the memory.x file to call out the memory location of the PID config
// its unsafe as we're telling rust to trust that CONFIG_START exists
unsafe extern "C" {
    static CONFIG_START: u32;
}
pub static FLASH_MUT_SIZE: usize = 2048 * 1024; //The allocated space for all saved variables
