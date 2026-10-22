//Thinking this represents the motor struct and task runner overall. ~big brain ish portion of the firware that decides overall what the motor is doing
// I think this should include the PID bits but I haven't fully decided on the best way to structure this

use embassy_time::{Duration, Ticker};

use crate::{
    direction::Direction, DIR_WATCH, MAX_SPINDLE_SPEED, MESA_DIR_REQ_SIG, MESA_RPM_REQ_SIG,
    PWM_SIG, RPM_BFR_DIR_SWTCH, RPM_WATCH,
};

#[embassy_executor::task]
pub async fn run_motor() {
    let mut rpm_stpt: u16 = 0; //Representing the rpm goal
    let mut rpm_watch = RPM_WATCH.receiver().unwrap(); //Direct feed from the hall sensor
    let dir_watch = DIR_WATCH.sender();
    let mut dir_stpt: Direction = Direction::Forward;
    let mut motor_pid = Pid {
        kp: 1.0,
        ki: 0.5,
        kd: 0.0,
        integral: 0.0,
        prev_error: 0.0,
    };

    let mut rpm = 0;
    let mut ticker = Ticker::every(Duration::from_millis(1));
    let mut prev_pwm_out = 0;
    loop {
        //if mesa has changed the rpm or dir then update our setpoints
        if let Some(new_rpm_set) = MESA_RPM_REQ_SIG.try_take() {
            rpm_stpt = new_rpm_set;
        }
        if let Some(new_dir_set) = MESA_DIR_REQ_SIG.try_take() {
            if new_dir_set == dir_stpt {
                continue;
            }
            //Turn motor off before switching
            while rpm > RPM_BFR_DIR_SWTCH {
                //Tell pwm to shut things down
                PWM_SIG.signal(0);
            }

            dir_watch.send(new_dir_set);
            dir_stpt = new_dir_set;
        }

        // Wait for dt to hit hopefully a new rpm was sent out
        // ovtherwise use previous
        ticker.next().await;
        if let Some(new_rpm) = rpm_watch.try_get() {
            rpm = new_rpm;
        }

        //update pwm based on rpm and setpnt
        prev_pwm_out = motor_pid.update(
            rpm_stpt as f32 / MAX_SPINDLE_SPEED as f32,
            rpm as f32 / MAX_SPINDLE_SPEED as f32,
            0.001,
            prev_pwm_out >= 1000,
        ) as i32;

        //Send that pwm to the pin
        PWM_SIG.signal(prev_pwm_out as u16);
    }
}

//TODO add autotune funcionality:
// Command 1000 RPM.
// Slowly increase kp until the motor responds quickly.
// Stop increasing when RPM begins oscillating.
// Cut kp approximately in half.
// Add a small ki.
//
// Example:
//
// kp = 0.08
// ki = 0.1
// kd = 0.0
//
// If RPM is still 50-100 RPM below target under load:
//
// ki += 0.1
//
// until the steady-state error disappears.
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
