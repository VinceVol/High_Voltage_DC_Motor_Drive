//This is here to keep track of how the controller is doing and basically determining whether the
// gate driver should be enabled and whether current/voltage is too high

use embassy_rp::{
    gpio::Output,
    peripherals::{PIN_1, PIN_20, PIN_9},
    Peri,
};

use crate::SAFETY_WATCH;

#[derive(Clone, Copy)]
pub enum ControllerCond {
    Ok,
    NG(ControllerError),
}

#[derive(Clone, Copy)]
pub enum ControllerError {
    CurrentOOR,     //Current out of preset range
    VoltageOOR,     //Voltage out of preset range
    Stall,          //If current is high and we stop rotating
    HallError,      //If we stop hearing back from the hall
    PreChargeError, //if the caps take too long to precharge
}

//The e stop out pin "GPIO 9" is used to tell the mesa controller to stop
// operations if there's some kind of motor fault. obviously if this thing
// is stalled out we don't want linux cnc to keep cookin
#[embassy_executor::task]
pub async fn be_safe(
    e_stop_out_p: Peri<'static, PIN_9>,
    q5_en: Peri<'static, PIN_20>,
    dig_drive_en_p: Peri<'static, PIN_1>,
) {
    //Tell others to stop if we're having issues
    let mut e_stop_out = Output::new(e_stop_out_p, embassy_rp::gpio::Level::Low);

    //Controls the enable pin on the dig gate drives -- shared with the OC out on the current sense
    // as in they both need to say things are good for the drives to be enabled
    let dr_en = Output::new(dig_drive_en_p, embassy_rp::gpio::Level::Low);

    //Controls the resistor feeding the caps -- when off all current needs to pass through
    // a resistor to fill the caps rather than letting current climb to infinity
    let mut q5 = Output::new(q5_en, embassy_rp::gpio::Level::Low);

    //Do nothing until the caps have precharged
    loop {
        match CAP_PRE_STATE_SIG.wait().await {
            PrechargeState::StartUp => continue,
            PrechargeState::NormalOp => break,
        }
    }
    q5.set_high();

    let mut safe_watch = SAFETY_WATCH.receiver().unwrap();

    loop {
        match safe_watch.changed().await {
            ControllerCond::Ok => {}
        }
    }
}
