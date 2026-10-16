//Use this file to control the speed of the motor

use embassy_rp::{
    gpio::Output,
    peripherals::{PIN_17, PIN_22, PWM_SLICE0, PWM_SLICE3},
    pwm::{Config, Pwm},
    Peri,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};

use crate::direction::DIR_CHANNEL;

pub static PWM_CHANNEL: Channel<CriticalSectionRawMutex, u64, 100> = Channel::new();

//Not sure if I should really do anything more than this for the dir_pin
#[embassy_executor::task]
pub async fn run_pwm(
    pwm_p_a: Peri<'static, PIN_22>,
    pwm_p_b: Peri<'static, PIN_17>,
    pwm_slice_3: Peri<'static, PWM_SLICE3>,
    pwm_slice_0: Peri<'static, PWM_SLICE0>,
) {
    let mut config = Config::default();
    config.top = 999;
    config.compare_a = 500; // 50% duty cycle

    let pwm_a = Pwm::new_output_a(pwm_slice_3, pwm_p_a, config);
    let pwm_b = Pwm::new_output_b(pwm_slice_0, pwm_p_b, config);
    loop {
        let dir = DIR_CHANNEL.receive().await;
        //TODO CONFIRM RPMS ARE LOW BEFORE SWAP
        if dir {
            dir_pin_b.set_low();
            dir_pin_a.set_high();
        } else {
            dir_pin_a.set_low();
            dir_pin_b.set_high();
        }
    }
}
