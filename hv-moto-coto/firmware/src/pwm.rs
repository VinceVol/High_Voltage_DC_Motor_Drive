//Use this file to control the speed of the motor

use defmt::info;
use embassy_rp::{
    peripherals::{PIN_17, PIN_22, PWM_SLICE0, PWM_SLICE3},
    pwm::{Config, Pwm},
    Peri,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Receiver};

use crate::{direction::Direction, DIR_WATCH, PWM_SIG};

//Not sure if I should really do anything more than this for the dir_pin
#[embassy_executor::task]
pub async fn run_pwm(
    pwm_p_a: Peri<'static, PIN_22>,
    pwm_p_b: Peri<'static, PIN_17>,
    pwm_slice_3: Peri<'static, PWM_SLICE3>,
    pwm_slice_0: Peri<'static, PWM_SLICE0>,
) {
    //watch direction
    let mut dir_watch = DIR_WATCH.receiver().unwrap();

    //Bit of copy and paste code here to set up the a and b channels so one can be off whilst the other is on
    let mut config_a = Config::default();
    config_a.top = 999;
    config_a.compare_a = 500; // 50% duty cycle
    let mut config_b = Config::default();
    config_b.top = 999;
    config_b.compare_b = 500; // 50% duty cycle

    let pwm_a = Pwm::new_output_a(pwm_slice_3, pwm_p_a, config_a.clone());
    let pwm_b = Pwm::new_output_b(pwm_slice_0, pwm_p_b, config_b.clone());

    let mut pwm_struct_a = MotorPwm {
        pwm: pwm_a,
        config: config_a,
        channel: PwmChannel::ChannelA,
    };
    let mut pwm_struct_b = MotorPwm {
        pwm: pwm_b,
        config: config_b,
        channel: PwmChannel::ChannelB,
    };
    //Make sure we have direction instruction to start
    // TODO add a loop here to make sure we request and receive a direction
    let mut dir = dir_watch.get().await;
    loop {
        //only do something if a new pwm instruction is givin
        let pwm = PWM_SIG.wait().await;
        //check to make sure the direction hasn't changed but don't wait on it
        match dir_watch.try_changed() {
            Some(new_dir) => dir = new_dir,
            None => (),
        }

        //pwm the proper direction and turn off the pwm to the other direciton
        match dir {
            Direction::Forward => {
                pwm_struct_b.set_duty(0);
                // info!("setting pwm_a to {}", pwm);
                pwm_struct_a.set_duty(pwm);
            }
            Direction::Reverse => {
                pwm_struct_a.set_duty(0);
                // info!("setting pwm_b to {}", pwm);
                pwm_struct_b.set_duty(pwm);
            }
        }
    }
}

enum PwmChannel {
    ChannelA,
    ChannelB,
}
//Using this MotorPwm struct to avoid constantly copying or cloning the config
struct MotorPwm<'d> {
    pwm: Pwm<'d>,
    config: Config,
    channel: PwmChannel,
}

impl<'d> MotorPwm<'d> {
    fn set_duty(&mut self, mut per_mille: u16) {
        if per_mille > 1000 {
            per_mille = 1000;
        }
        match self.channel {
            PwmChannel::ChannelA => {
                self.config.compare_a =
                    ((self.config.top as u32 + 1) * per_mille as u32 / 1000) as u16;
            }
            PwmChannel::ChannelB => {
                self.config.compare_b =
                    ((self.config.top as u32 + 1) * per_mille as u32 / 1000) as u16;
            }
        }
        self.pwm.set_config(&self.config);
    }
}
