//Thinking this represents the motor struct and task runner overall. ~big brain ish portion of the firware that decides overall what the motor is doing
// I think this should include the PID bits but I haven't fully decided on the best way to structure this

use core_logic::interpolation::{interpolate, interpolate_flt};
use defmt::info;
use embassy_rp::{
    flash::{Async, Flash},
    gpio::Output,
    peripherals::{FLASH, PIN_1, PIN_20},
    Peri,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, watch::Receiver};
use embassy_time::{Duration, Ticker, Timer};

use crate::{
    autotune::PidConfig, cap_precharge::PrechargeState, direction::Direction, speed_sense::HallRpm,
    CAP_PRE_STATE_SIG, DIR_REQ_SIG, DIR_WATCH, FLASH_MUT_SIZE, MAX_SPINDLE_SPEED, MOT_LOOP_EN,
    PWM_WATCH, RPM_BFR_DIR_SWTCH, RPM_FILT_WATCH, RPM_REQ_SIG,
};

#[embassy_executor::task]
pub async fn run_motor(
    q5_en: Peri<'static, PIN_20>,
    dig_drive_en: Peri<'static, PIN_1>,
    flash_mut: &'static Mutex<
        CriticalSectionRawMutex,
        Flash<'static, FLASH, Async, { FLASH_MUT_SIZE }>,
    >,
) {
    //Do nothing until the caps have precharged
    // q5 controls the precharge mosfet
    let mut q5 = Output::new(q5_en, embassy_rp::gpio::Level::Low);
    loop {
        match CAP_PRE_STATE_SIG.wait().await {
            PrechargeState::StartUp => continue,
            PrechargeState::NormalOp => break,
        }
    }
    q5.set_high();

    //enable the drives
    // TODO Move this to a safety circuit TODO
    let dr_en = Output::new(dig_drive_en, embassy_rp::gpio::Level::High);

    //Setup PID components
    let mut rpm_stpt: u16 = 0; //Representing the rpm goal
    let mut rpm_watch = RPM_FILT_WATCH.receiver().unwrap(); //Direct feed from the hall sensor
    let pwm_watch = PWM_WATCH.sender();
    let dir_watch = DIR_WATCH.sender();
    let mut dir_stpt: Direction = Direction::Forward;
    dir_watch.send(dir_stpt); //default to forward

    //load up the flash saved autotune parameters
    let pid_conf = PidConfig::load(flash_mut).await.unwrap();
    let mut mot_en = true; //Watch the motor enabled signal as autotune may need to own the pwm
    let mut pwm_floor = 0; //The lowest rpm we should output, based on the lookup table

    //Load the parameters into the pid helper struct
    let mut motor_pid = Pid {
        kp: 0.35,
        ki: 0.2,
        kd: 0.0,
        integral: 0.0,
        prev_error: 0.0,
    };

    let mut rpm = 0;
    let mut ticker = Ticker::every(Duration::from_millis(1)); //PID Refresh Rate
    let mut prev_pwm_out = 0;
    loop {
        //check to make sure the autotune doesn't need the pwm signal
        if let Some(sig) = MOT_LOOP_EN.try_take() {
            mot_en = sig;
            ticker.reset(); //reset the ticker to avoid weird delay buildups from missed ticks
        }

        //Don't let motor_en block the rest of the firmware
        if !mot_en {
            Timer::after_secs(1).await; //yield for a while
            continue;
        }

        //if mesa has changed the rpm or dir then update our setpoints
        if let Some(new_rpm_set) = RPM_REQ_SIG.try_take() {
            rpm_stpt = new_rpm_set;

            //Update the pwm floor based on autotune interpolation
            pwm_floor = interpolate::<u16, i32, 24>(
                &pid_conf.floor_table_rpm,
                &pid_conf.floor_table_pwm,
                new_rpm_set,
            );

            //Update P and I components of motor_pid based on autotune interpolation
            motor_pid.kp = interpolate_flt::<f32, 6>(
                &pid_conf.pi_table_rpm,
                &pid_conf.pi_table_p,
                new_rpm_set as f32,
            );
            motor_pid.ki = interpolate_flt::<f32, 6>(
                &pid_conf.pi_table_rpm,
                &pid_conf.pi_table_i,
                new_rpm_set as f32,
            );

            pwm_watch.send(pwm_floor); //start off using the lookup table speed
        }

        //Direction change reaction
        if let Some(new_dir_set) = DIR_REQ_SIG.try_take() {
            if new_dir_set == dir_stpt {
                continue;
            }

            //Turn motor off before switching
            pwm_watch.send(0);
            while rpm > RPM_BFR_DIR_SWTCH {
                Timer::after(Duration::from_millis(20)).await;

                // Update rpm from the watch channel without blocking
                if let Some(HallRpm::Good(new_rpm)) = rpm_watch.try_changed() {
                    rpm = new_rpm;
                } else {
                    Timer::after(Duration::from_millis(1)).await;
                }
            }

            rpm = 0;
            dir_watch.send(new_dir_set);
            dir_stpt = new_dir_set;
            Timer::after(Duration::from_millis(1)).await; //short pause to swap direction

            //IMPORTANT -- without resetting the ticker it loops at full speed blocking all other tasks until it gets back to the current ticker
            // without this you get ~1s delay between the low rpm + current draw + direction pin switch and the pwm pin firing
            ticker.reset();
        }

        // Wait for dt to hit hopefully a new rpm was sent out
        // ovtherwise use previous
        ticker.next().await;
        if let Some(HallRpm::Good(new_rpm)) = rpm_watch.try_changed() {
            rpm = new_rpm;
        }

        //update pwm based on rpm and setpnt
        let pid_output = motor_pid.update(
            rpm_stpt as f32 / MAX_SPINDLE_SPEED as f32,
            rpm as f32 / MAX_SPINDLE_SPEED as f32,
            0.001,
            prev_pwm_out >= 1000,
        );

        // Scale and clamp the normalized PID output to your 0-1000 PWM range
        prev_pwm_out = (pid_output * 1000.0).clamp(0.0, 1000.0) as i32;

        if rpm_stpt == 0 {
            prev_pwm_out = 0;
        } else if prev_pwm_out < pwm_floor as i32 {
            //pwm 200 if anywhere between 1-200 a
            prev_pwm_out = pwm_floor as i32;
        }
        //Send that pwm to the pin
        // info!(
        //     "sending pwm {} for rpm req {}, current rpm: {}, direction: {:?}",
        //     prev_pwm_out, rpm_stpt, rpm, dir_stpt
        // );
        pwm_watch.send(prev_pwm_out as u16);
    }
}

pub struct Pid {
    kp: f32,
    ki: f32,
    kd: f32,

    integral: f32,
    prev_error: f32,
}

impl Pid {
    //To make the kp ki and kd a bit easier to adjust, the setpoint and measured inputs should be normalized
    // let setpoint = commanded_rpm / MAX_RPM;
    // let measured = actual_rpm / MAX_RPM;
    // dt is seconds
    pub fn update(&mut self, setpoint: f32, measured: f32, dt: f32, duty_clamped: bool) -> f32 {
        let error = setpoint - measured;

        if !duty_clamped {
            self.integral += error * dt;
        }

        let derivative = (error - self.prev_error) / dt;

        self.prev_error = error;

        self.kp * error + self.ki * self.integral + self.kd * derivative
    }
}
