//Use this file to control the speed of the motor

use defmt::info;
use embassy_rp::{
    peripherals::{
        PIN_14, PIN_15, PIN_16, PIN_17, PIN_2, PIN_21, PIN_22, PIN_3, PWM_SLICE0, PWM_SLICE1,
        PWM_SLICE2, PWM_SLICE3, PWM_SLICE7,
    },
    pwm::{Config, Pwm, SetDutyCycle},
    Peri,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Receiver};

use crate::{direction::Direction, DIR_WATCH, PWM_WATCH};

//Not sure if I should really do anything more than this for the dir_pin
#[embassy_executor::task]
pub async fn run_pwm(
    pwm_p_a: Peri<'static, PIN_2>,
    dir_p_b: Peri<'static, PIN_3>,
    pwm_p_b: Peri<'static, PIN_14>,
    dir_p_a: Peri<'static, PIN_15>,
    pwm_slice_1: Peri<'static, PWM_SLICE1>,
    pwm_slice_7: Peri<'static, PWM_SLICE7>,
) {
    //watch direction and pwm
    let mut dir_watch = DIR_WATCH.receiver().unwrap();
    let mut pwm_watch = PWM_WATCH.receiver().unwrap();

    //Bit of copy and paste code here to set up the a and b channels so one can be off whilst the other is on
    // Invert b to help prevent heat buildup through motor inductance induced current pushing through lower
    // leg fet. The gate drivers have built in dead time to help us out
    // Left Leg
    let mut config = Config::default();
    config.top = 7499;
    config.compare_a = 0; // 50% duty cycle
    config.compare_b = 0;
    config.invert_b = true;

    let mut left_leg = Pwm::new_output_ab(pwm_slice_1, pwm_p_a, dir_p_b, config.clone());
    let mut right_leg = Pwm::new_output_ab(pwm_slice_7, pwm_p_b, dir_p_a, config.clone());

    //Make sure we have direction instruction to start
    // TODO add a loop here to make sure we request and receive a direction
    let mut dir = dir_watch.get().await;
    let mut pwm = 0;
    loop {
        //only do something if a new pwm instruction is givin
        let new_pwm = pwm_watch.changed().await;
        pwm = if new_pwm != pwm { new_pwm } else { continue };

        //check to make sure the direction hasn't changed but don't wait on it
        match dir_watch.try_changed() {
            Some(new_dir) => dir = new_dir,
            None => (),
        }

        //pwm the proper direction and turn off the pwm to the other direciton
        pwm = if pwm > 1000 { 1000 } else { pwm };
        match dir {
            Direction::Forward => {
                right_leg.set_duty_cycle_fully_off().unwrap();
                info!("setting pwm_a to {}", pwm);
                left_leg.set_duty_cycle_fraction(pwm, 1000).unwrap();
            }
            Direction::Reverse => {
                left_leg.set_duty_cycle_fully_off().unwrap();
                info!("setting pwm_a to {}", pwm);
                right_leg.set_duty_cycle_fraction(pwm, 1000).unwrap();
            }
        }
    }
}
