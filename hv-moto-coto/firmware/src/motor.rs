//Thinking this represents the motor struct and task runner overall. ~big brain ish portion of the firware that decides overall what the motor is doing
// I think this should include the PID bits but I haven't fully decided on the best way to structure this

use embassy_rp::{
    adc::{Adc, Channel, Config},
    bind_interrupts,
    peripherals::{ADC, PIN_0, PIN_1, PIN_27},
    Peri,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Receiver, Sender},
};
use embassy_time::{Duration, Instant, Ticker};

use crate::{direction::Direction, MAX_SPINDLE_SPEED, MESA_DIR_REQ_SIG, MESA_RPM_REQ_SIG};

#[embassy_executor::task]
pub async fn run_motor(
    en_drives: Peri<'static, PIN_1>,
    dir_sender: Sender<'static, CriticalSectionRawMutex, Direction, 25>,
    pwm_sender: Sender<'static, CriticalSectionRawMutex, u16, 25>,
) {
    let mut rpm_stpt: u16 = 0; //Representing the rpm goal
    let mut dir_stpt: Direction = Direction::Forward;
    let mut motor_pid = Pid {
        kp: 1.0,
        ki: 0.5,
        kd: 0.0,
        integral: 0.0,
        prev_error: 0.0,
    };

    let mut ticker = Ticker::every(Duration::from_millis(1));
    loop {
        //if mesa has changed the rpm or dir then update our setpoints
        if let Some(new_rpm_set) = MESA_RPM_REQ_SIG.try_take() {
            rpm_stpt = new_rpm_set;
        }
        if let Some(new_dir_set) = MESA_DIR_REQ_SIG.try_take() {
            dir_stpt = new_dir_set;
        }

        ticker.next().await;
        todo!();
        motor_pid.update((rpm_stpt as f32 / MAX_SPINDLE_SPEED as f32));
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
