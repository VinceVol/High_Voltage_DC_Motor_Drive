//Thinking this represents the motor struct and task runner overall. ~big brain ish portion of the firware that decides overall what the motor is doing
// I think this should include the PID bits but I haven't fully decided on the best way to structure this

use core_logic::interpolation::{interpolate, interpolate_flt};
use embassy_futures::select;
use embassy_rp::{
    flash::{Async, Flash},
    gpio::Output,
    peripherals::FLASH,
    Peri,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, watch::Receiver};
use embassy_time::{Duration, Ticker, Timer};

use crate::{
    autotune::PidConfig, pwm::Direction, speed_sense::HallRpm, DIR_REQ_SIG, DIR_WATCH,
    FLASH_MUT_SIZE, I_REF_RATE_US, I_WATCH, MAX_CURRENT_AMPS, MAX_SPINDLE_SPEED, MOT_LOOP_EN,
    PI_TBL_CNT, PWM_FREQ, PWM_WATCH, RPM_BFR_DIR_SWTCH, RPM_FILT_WATCH, RPM_REF_RATE_MS,
    RPM_REQ_SIG,
};

#[embassy_executor::task]
pub async fn run_motor(
    flash_mut: &'static Mutex<
        CriticalSectionRawMutex,
        Flash<'static, FLASH, Async, { FLASH_MUT_SIZE }>,
    >,
) {
    //Setup PID components
    // direction
    let dir_watch = DIR_WATCH.sender();
    let mut dir_stpt: Direction = Direction::Forward;
    dir_watch.send(dir_stpt); //default to forward

    //current
    let mut i_watch = I_WATCH.receiver().unwrap();
    let mut current = 0.0; //Tracking current measurement pulling from I_WATCH
    let mut current_ticker = Ticker::every(Duration::from_micros(I_REF_RATE_US)); //PID Refresh Rate current loop
    let mut target_current_norm = 0.0;
    let mut current_pid_output = 0.0;

    //rpm/speed
    let mut rpm_stpt: u16 = 0; //Representing the rpm goal
    let mut rpm_watch = RPM_FILT_WATCH.receiver().unwrap(); //Direct feed from the hall sensor
    let mut rpm = 0; //Tacking speed measurement pulling from RPM_FILT_WATCH
    let mut speed_ticker = Ticker::every(Duration::from_millis(RPM_REF_RATE_MS)); //PID Refresh Rate speed loop

    //pwm output
    let mut prev_pwm_out = 0; //PID Loop output each cycle

    //AUTOTUNE
    //load up the flash saved autotune parameters
    let pid_conf = PidConfig::load(flash_mut).await.unwrap();
    let mut mot_en = true; //Watch the motor enabled signal as autotune may need to own the pwm
    let mut feed_forward = 0; //The lowest rpm we should output, based on the lookup table

    //Load the parameters into the current/speed pid helper struct
    let mut current_pid = Pid {
        kp: 1.5,
        ki: 75.0,
        kd: 0.0,
        integral: 0.0,
        prev_error: 0.0,
    };
    let mut speed_pid = Pid {
        kp: 2.0,
        ki: 1.0,
        kd: 0.0,
        integral: 0.0,
        prev_error: 0.0,
    };

    loop {
        //check to make sure the autotune doesn't need the pwm signal
        if let Some(sig) = MOT_LOOP_EN.try_take() {
            mot_en = sig;

            //reset the tickers to avoid weird delay buildups from missed ticks
            current_ticker.reset();
            speed_ticker.reset();
        }

        //Don't let motor_en block the rest of the firmware
        if !mot_en {
            Timer::after_secs(1).await; //yield for a while
            continue;
        }

        //NEW INPUT FOR RPM
        //if mesa has changed the rpm or dir then update our setpoints
        if let Some(new_rpm_set) = RPM_REQ_SIG.try_take() {
            rpm_stpt = new_rpm_set;

            //Feed Forward update based on autotune interpolation
            feed_forward = interpolate::<u16, i32, 24>(
                &pid_conf.floor_table_rpm,
                &pid_conf.floor_table_pwm,
                rpm_stpt,
            );

            // TODO -- FIX AUTOTUNE-----------------------------------------------
            //Update P and I components of motor_pid based on autotune interpolation
            // motor_pid.kp = interpolate_flt::<f32, PI_TBL_CNT>(
            //     &pid_conf.pi_table_rpm,
            //     &pid_conf.pi_table_p,
            //     new_rpm_set as f32,
            // );
            // motor_pid.ki = interpolate_flt::<f32, PI_TBL_CNT>(
            //     &pid_conf.pi_table_rpm,
            //     &pid_conf.pi_table_i,
            //     new_rpm_set as f32,
            // );
            // TODO -- FIX AUTOTUNE-----------------------------------------------
        }

        //TODO move to sep fn----------------------------------------------------
        //Direction change reaction
        if let Some(new_dir_set) = DIR_REQ_SIG.try_take() {
            if new_dir_set == dir_stpt {
                continue;
            }

            change_directions(new_dir_set, &mut rpm_watch).await;

            rpm = 0;
            dir_stpt = new_dir_set;
            Timer::after(Duration::from_millis(1)).await; //short pause to swap direction

            //IMPORTANT -- without resetting the ticker it loops at full speed blocking all other tasks until it gets back to the current ticker
            // without this you get ~1s delay between the low rpm + current draw + direction pin switch and the pwm pin firing
            current_ticker.reset();
            speed_ticker.reset();
        }
        //TODO move to sep fn----------------------------------------------------

        //React to whichever ticker is up next
        match select::select(speed_ticker.next(), current_ticker.next()).await {
            select::Either::First(_) => {
                //RPM LOOP
                if let Some(HallRpm::Good(new_rpm)) = rpm_watch.try_get() {
                    rpm = new_rpm;
                }
                let norm_rpm_stpnt = rpm_stpt as f32 / MAX_SPINDLE_SPEED as f32;
                let norm_rpm = rpm as f32 / MAX_SPINDLE_SPEED as f32;

                //update pwm based on rpm and setpnt /normalize at the same time
                target_current_norm = speed_pid
                    .update(
                        norm_rpm_stpnt,
                        norm_rpm,
                        (RPM_REF_RATE_MS as f32) / 10e3f32, //refresh in seconds
                        false,
                    )
                    .clamp(0.0, 1.0);

                // INFO ------------------------------------------------------------------
                // defmt::info!(
                //     "sending pwm {} for rpm req {}, current rpm: {}, direction: {:?}",
                //     prev_pwm_out,
                //     rpm_stpt,
                //     rpm,
                //     dir_stpt
                // );
                // INFO ------------------------------------------------------------------
                // defmt::info!(
                //     "SPEED PID:\nnorm stpnt: {}\nnorm rpm: {}\nTarget norm curr: {}",
                //     norm_rpm_stpnt,
                //     norm_rpm,
                //     target_current_norm
                // );
            }
            select::Either::Second(_) => {
                //CURRENT LOOP
                if let Some(new_current) = i_watch.try_changed() {
                    current = new_current;
                }
                let current_measured_norm = current / MAX_CURRENT_AMPS;

                current_pid_output = current_pid.update(
                    target_current_norm,
                    current_measured_norm,
                    (I_REF_RATE_US as f32) / 10e6f32, //refresh in seconds
                    prev_pwm_out == 0 || prev_pwm_out >= 1000,
                );
                // defmt::info!(
                //     "CURRENT PID:\nnorm stpnt: {}\nnorm current: {}\noutput: {}",
                //     target_current_norm,
                //     current_measured_norm,
                //     current_pid_output
                // );
            }
        }

        // Back-EMF Feed-Forward
        let total_command = current_pid_output + (feed_forward as f32) / 1000f32;

        // Hardware PWM output scaling (0-1000)
        let pwm_float = total_command * 1000.0;
        prev_pwm_out = pwm_float.clamp(0.0, 1000.0) as i32;

        if rpm_stpt <= 30 {
            prev_pwm_out = 0;
        }

        //Send that pwm to the pin
        PWM_WATCH.sender().send(prev_pwm_out as u16);
        // PWM_WATCH.sender().send(300u16);
    }
}

async fn change_directions<'a>(
    new_direction: Direction,
    rpm_watch: &mut Receiver<'a, CriticalSectionRawMutex, HallRpm, 100>,
) {
    let mut rpm = MAX_SPINDLE_SPEED;
    //Turn motor off before switching
    PWM_WATCH.sender().send(0);
    while rpm > RPM_BFR_DIR_SWTCH {
        Timer::after(Duration::from_millis(20)).await;

        // Update rpm from the watch channel without blocking
        if let Some(HallRpm::Good(new_rpm)) = rpm_watch.try_changed() {
            rpm = new_rpm;
        } else {
            Timer::after(Duration::from_millis(1)).await;
        }
    }
    DIR_WATCH.sender().send(new_direction);
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
