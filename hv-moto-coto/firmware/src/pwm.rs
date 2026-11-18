//Use this file to control the speed of the motor

use core::sync::atomic::Ordering::Relaxed;

use defmt::{info, Format};
use embassy_rp::{
    interrupt::InterruptExt,
    peripherals::{PIN_14, PIN_15, PIN_2, PIN_3, PIN_9, PWM_SLICE1, PWM_SLICE4, PWM_SLICE7},
    pwm::{Config, Pwm, SetDutyCycle},
    Peri,
};
use embassy_time::{block_for, Duration};
use rp_pac::interrupt;

use crate::{DIR_WATCH, I_INTR, I_REF_RATE_US, PWM_FREQ, PWM_INT_CNT, PWM_WATCH};

//Not sure if I should really do anything more than this for the dir_pin
#[embassy_executor::task]
pub async fn run_pwm(
    pwm_p_a: Peri<'static, PIN_2>,
    dir_p_b: Peri<'static, PIN_3>,
    pwm_p_b: Peri<'static, PIN_14>,
    dir_p_a: Peri<'static, PIN_15>,
    cntr_p: Peri<'static, PIN_9>,
    pwm_slice_1: Peri<'static, PWM_SLICE1>,
    pwm_slice_7: Peri<'static, PWM_SLICE7>,
    pwm_slice_4: Peri<'static, PWM_SLICE4>,
) {
    //watch direction and pwm
    let mut dir_watch = DIR_WATCH.receiver().unwrap();
    let mut pwm_watch = PWM_WATCH.receiver().unwrap();

    //INTERRUPT SETUP --------
    //Unmask the PWM wrap interrupt in the NVIC
    unsafe {
        cortex_m::peripheral::NVIC::unmask(rp_pac::Interrupt::PWM_IRQ_WRAP_0);
    }

    //set the pwm wrap interrupt to the highest priority
    rp_pac::interrupt::PWM_IRQ_WRAP_0.set_priority(embassy_rp::interrupt::Priority::P3);
    //INTERRUPT SETUP --------

    //Bit of copy and paste code here to set up the a and b channels so one can be off whilst the other is on
    // Invert b to help prevent heat buildup through motor inductance induced current pushing through lower
    // leg fet. The gate drivers have built in dead time to help us out
    // Left Leg
    let mut config = Config::default();
    config.top = ((PWM_FREQ * 3) / 8 - 1) as u16;
    config.compare_a = 0;
    config.compare_b = 0;
    config.invert_b = true;

    rp_pac::PWM.en().write(|w| {
        w.set_ch7(true);
        w.set_ch1(true);
        w.set_ch4(true);
    });

    let mut left_leg = Pwm::new_output_ab(pwm_slice_1, pwm_p_a, dir_p_b, config.clone());
    let mut right_leg = Pwm::new_output_ab(pwm_slice_7, pwm_p_b, dir_p_a, config);

    //CENTER ALIGNED INTERRUPT FOR ADC ---------------------------------------
    let mut config_b = Config::default();
    config_b.top = ((PWM_FREQ * 3) / 8 - 1) as u16;
    config_b.compare_b = 0;
    let mut cent_align = Pwm::new_output_b(pwm_slice_4, cntr_p, config_b);
    rp_pac::PWM.irq0_inte().modify(|w| {
        w.set_ch4(true);
    });
    //CENTER ALIGNED INTERRUPT FOR ADC ---------------------------------------

    //Make sure we have direction instruction to start
    let mut dir = dir_watch.get().await;
    let mut pwm = 0;
    loop {
        //only do something if a new pwm instruction is givin
        let new_pwm = pwm_watch.changed().await;
        if new_pwm != pwm {
            pwm = new_pwm.clamp(0, 1000);
        } else {
            continue;
        }

        //check to make sure the direction hasn't changed but don't wait on it
        match dir_watch.try_changed() {
            Some(new_dir) => dir = new_dir,
            None => (),
        }

        //Update center aligned pwm "-100" there to make up for some of the lag when reading the adc
        cent_align
            .set_duty_cycle_fraction((pwm as i32 - 100).clamp(0, 1000) as u16, 2000)
            .unwrap();

        //Adjust H bridge PWM signals
        match dir {
            Direction::Forward => {
                right_leg.set_duty_cycle_fully_off().unwrap();
                // defmt::info!("setting pwm_a to {}", pwm);
                left_leg.set_duty_cycle_fraction(pwm, 1000).unwrap();
            }
            Direction::Reverse => {
                left_leg.set_duty_cycle_fully_off().unwrap();
                // defmt::info!("setting pwm_a to {}", pwm);
                right_leg.set_duty_cycle_fraction(pwm, 1000).unwrap();
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Format)]
pub enum Direction {
    Forward,
    Reverse,
}

//Current pwm frequency should be ~ 20khz, we want the adc read for current sense
// to be aligned on the center of the pwm with a 1ms refresh rate which should
// equate to every 80 cycles
#[interrupt]
fn PWM_IRQ_WRAP_0() {
    //If its one of the H bridge slices then check whether we've hit the 1ms count
    let cur_cnt = PWM_INT_CNT.load(Relaxed);

    if cur_cnt >= ((I_REF_RATE_US * PWM_FREQ) / 1000000) as u16 {
        //reset the count
        PWM_INT_CNT.store(0, Relaxed);

        // --- 1. SET DEBUG PIN HIGH (GPIO 8) ---
        debug_high(8);

        // --- INSTANT ADC READ (PAC LEVEL) ---
        // 1. Select ADC input channel for PIN_26 (which is ADC channel 0)
        rp_pac::ADC.cs().modify(|w| {
            w.set_en(true); // Select channel 0 for pin 26
            w.set_ainsel(0);
            w.set_start_once(true); // Trigger a single conversion
        });

        // 2. Poll until conversion is complete (takes ~2us, safe inside an ISR)
        while !rp_pac::ADC.cs().read().ready() {
            // Spin-wait briefly for hardware flag
        }
        debug_low(8);

        // 3. Read the 12-bit result register
        let good_v = rp_pac::ADC.result().read().result();
        I_INTR.signal(good_v);

        // --- 3. SET DEBUG PIN LOW (GPIO 8) ---
    } else {
        PWM_INT_CNT.fetch_add(1, Relaxed);
    }

    //IMPORTANT RESET THE INTERRUPT
    rp_pac::PWM.intr().write(|w| w.set_ch4(true));
}

// Define the SIO register base addresses as constants
const SIO_BASE: usize = 0xD000_0000;
const GPIO_OUT_SET: usize = SIO_BASE + 0x018;
const GPIO_OUT_CLR: usize = SIO_BASE + 0x020;

#[inline(always)]
fn debug_high(pin: u32) {
    unsafe {
        // Writing a 1 to a bit in GPIO_OUT_SET forces that pin high instantly
        core::ptr::write_volatile(GPIO_OUT_SET as *mut u32, 1 << pin);
    }
}

#[inline(always)]
fn debug_low(pin: u32) {
    unsafe {
        // Writing a 1 to a bit in GPIO_OUT_CLR forces that pin low instantly
        core::ptr::write_volatile(GPIO_OUT_CLR as *mut u32, 1 << pin);
    }
}
