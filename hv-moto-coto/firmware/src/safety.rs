//This is here to keep track of how the controller is doing and basically determining whether the
// gate driver should be enabled and whether current/voltage is too high

use defmt::Format;
use embassy_rp::{
    gpio::Output,
    peripherals::{PIN_1, PIN_20, PIN_9},
    Peri,
};
use embassy_time::Instant;

use crate::{I_CORR_OFF, I_WATCH, PWM_WATCH, RPM_FILT_WATCH, SAFETY_WATCH, V_WATCH};

#[derive(Clone, Copy)]
pub enum ControllerCond {
    Ok,
    Wait,
    NG(ControllerError),
}

#[derive(Clone, Copy, Format)]
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
pub async fn be_safe(e_stop_out_p: Peri<'static, PIN_9>, dig_drive_en_p: Peri<'static, PIN_1>) {
    //Tell others to stop if we're having issues
    let mut e_stop_out = Output::new(e_stop_out_p, embassy_rp::gpio::Level::Low);

    //Controls the enable pin on the dig gate drives -- shared with the OC out on the current sense
    // as in they both need to say things are good for the drives to be enabled
    let mut dr_en = Output::new(dig_drive_en_p, embassy_rp::gpio::Level::Low);

    let mut safe_watch = SAFETY_WATCH.receiver().unwrap();

    loop {
        match safe_watch.changed().await {
            ControllerCond::Ok => {
                dr_en.set_high();
                defmt::info!("Enabling the drives!");
            }
            ControllerCond::Wait => {
                dr_en.set_low();
                defmt::info!("Waiting to re-enable the drives!");
            }
            ControllerCond::NG(contr_err) => {
                defmt::info!("Controller Error: {:?}", contr_err);
                dr_en.set_low();
                e_stop_out.set_high();
            }
        }
    }
}

const TIME_PRE_CHG_PANIC_MS: u64 = 100; //if caps take longer than 300 ms to start up throw flag
const V_PRECH_THRESH: i64 = 5; //TODO Testing things at a low voltage for now so 5 will need to be changed TODO
#[embassy_executor::task]
pub async fn are_we_safe(q5_en: Peri<'static, PIN_20>) {
    let mut i_watch = I_WATCH.receiver().unwrap();
    let mut v_watch = V_WATCH.receiver().unwrap();
    let mut rpm_watch = RPM_FILT_WATCH.receiver().unwrap();
    let mut pwm_watch = PWM_WATCH.receiver().unwrap();

    let mut error_state = false; //Keep track of whether we're in a emergency state

    //ZERO CURRENT SENSE
    todo!(); //FIX the code below to properly add offset
    *I_CORR_OFF.lock().await = i_watch.changed().await;
    //ZERO CURRENT SENSE

    //CAP PRE CHARGE -------------------------------------------------------------
    //Precharge the caps and tell the program to wait
    //
    //Controls the resistor feeding the caps -- when off all current needs to pass through
    // a resistor to fill the caps rather than letting current climb to infinity
    let mut q5 = Output::new(q5_en, embassy_rp::gpio::Level::Low);

    let cap_timer = Instant::now();
    SAFETY_WATCH.sender().send(ControllerCond::Wait);
    let mut v;
    loop {
        v = v_watch.changed().await;
        defmt::info!("The current precharge voltage = {}", v);

        if v >= V_PRECH_THRESH {
            defmt::info!("Finished precharging the caps!");
            q5.set_high();
            SAFETY_WATCH.sender().send(ControllerCond::Ok);
            break;
        } else if cap_timer.elapsed().as_millis() > TIME_PRE_CHG_PANIC_MS {
            SAFETY_WATCH
                .sender()
                .send(ControllerCond::NG(ControllerError::PreChargeError));
            error_state = true;
            break;
        }
    }
    //CAP PRE CHARGE -------------------------------------------------------------

    //Monitor other systems
    while !error_state {}
}
