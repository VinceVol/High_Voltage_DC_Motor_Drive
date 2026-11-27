//This is here to keep track of how the controller is doing and basically determining whether the
// gate driver should be enabled and whether current/voltage is too high

use defmt::Format;
use embassy_futures::select::select4;
use embassy_rp::{
    gpio::Output,
    peripherals::{PIN_1, PIN_20, PIN_9},
    Peri,
};
use embassy_time::Instant;

use crate::{
    roll_current::RollingCurrent, I_CORR_OFF, I_WATCH, MAX_CURRENT_MA, PWM_WATCH, RPM_FILT_WATCH,
    SAFETY_WATCH, V_WATCH,
};

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

impl ControllerError {
    pub const fn as_msg(self) -> &'static str {
        match self {
            ControllerError::CurrentOOR => "CURR OOR",
            ControllerError::VoltageOOR => "VOLT OOR",
            ControllerError::Stall => "MOTOR STALL",
            ControllerError::HallError => "HALL FAULT",
            ControllerError::PreChargeError => "PRECHARGE ERR",
        }
    }
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
                //FIX re-enable these later
                // dr_en.set_low();
                defmt::info!("Waiting to re-enable the drives!");
            }
            ControllerCond::NG(contr_err) => {
                defmt::info!("Controller Error: {:?}", contr_err);
                //FIX re-enable these later
                // dr_en.set_low();
                e_stop_out.set_high();
            }
        }
    }
}

const TIME_PRE_CHG_PANIC_MS: u64 = 100; //if caps take longer than 300 ms to start up throw flag
const V_PRECH_THRESH: i64 = 5; //FIX Testing things at a low voltage for now so 5 will need to be changed TODO
const UNDER_VOLT: i64 = 3; //FIX LATER and update to ~105
const OVER_VOLT: i64 = 125;
const STALL_RPM: u16 = 20;
// const STALL_PWM: u16 = 100;
// const HALL_ERR_CNT_THRESH: u8 = 4; //how many filtered rpm reads can return an error before we throw flag
#[embassy_executor::task]
pub async fn are_we_safe(q5_en: Peri<'static, PIN_20>) {
    let mut i_watch = I_WATCH.receiver().unwrap();
    let mut v_watch = V_WATCH.receiver().unwrap();
    let mut rpm_watch = RPM_FILT_WATCH.receiver().unwrap();
    let mut pwm_watch = PWM_WATCH.receiver().unwrap();

    let mut error_state = false; //Keep track of whether we're in a emergency state

    //ZERO CURRENT SENSE
    let mut curr_roll: RollingCurrent<20> = RollingCurrent::fill(&mut i_watch).await;
    curr_roll.wait_till_steady(0.01, &mut i_watch).await;
    *I_CORR_OFF.lock().await -= curr_roll.mean();
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

    let mut _cur_pwm = pwm_watch.get().await;
    let mut _cur_rpm = STALL_RPM + 1;
    let mut _hall_error_cnt = 0;

    //Monitor other systems
    while !error_state {
        match select4(
            curr_roll.update(&mut i_watch),
            v_watch.changed(),
            rpm_watch.changed(),
            pwm_watch.changed(),
        )
        .await
        {
            embassy_futures::select::Either4::First(_) => {
                if curr_roll.mean()
                    > MAX_CURRENT_MA.load(core::sync::atomic::Ordering::Relaxed) as f32
                {
                    SAFETY_WATCH
                        .sender()
                        .send(ControllerCond::NG(ControllerError::CurrentOOR));
                    error_state = true;
                }
            }
            embassy_futures::select::Either4::Second(new_volt) => {
                if new_volt >= OVER_VOLT {
                    defmt::info!(
                        "Read a voltage = {} which is over {} or under {}",
                        new_volt,
                        OVER_VOLT,
                        UNDER_VOLT
                    );
                    SAFETY_WATCH
                        .sender()
                        .send(ControllerCond::NG(ControllerError::VoltageOOR));
                    error_state = true;
                }
            }
            embassy_futures::select::Either4::Third(new_rpm) => match new_rpm {
                crate::speed_sense::HallRpm::Good(rpm) => {
                    _cur_rpm = rpm;
                    _hall_error_cnt = 0;
                }
                crate::speed_sense::HallRpm::Stall => {
                    SAFETY_WATCH
                        .sender()
                        .send(ControllerCond::NG(ControllerError::Stall));
                    error_state = true;
                }
                _ => _hall_error_cnt += 1,
            },
            embassy_futures::select::Either4::Fourth(new_pwm) => {
                _cur_pwm = new_pwm;
            }
        }
    }
}
