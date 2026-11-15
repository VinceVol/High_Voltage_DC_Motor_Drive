//Use this file to control the speed of the motor

use core::sync::atomic::Ordering::Relaxed;

use defmt::info;
use embassy_rp::{
    bind_interrupts,
    interrupt::InterruptExt,
    peripherals::{
        PIN_14, PIN_15, PIN_16, PIN_17, PIN_2, PIN_21, PIN_22, PIN_3, PWM_SLICE0, PWM_SLICE1,
        PWM_SLICE2, PWM_SLICE3, PWM_SLICE7,
    },
    pwm::{Config, Pwm, SetDutyCycle},
    Peri,
};
use embassy_time::Timer;
use rp_pac::{interrupt, io::regs::GpioCtrl};

use crate::{direction::Direction, DIR_WATCH, I_INTR, PWM_INT_CNT, PWM_WATCH};

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

    // 1. Enable IRQ for slices 1 and 7 in the PWM peripheral
    rp_pac::PWM.irq0_inte().modify(|w| {
        w.set_ch1(true);
        w.set_ch7(true);
    });

    // 2. Unmask the PWM wrap interrupt in the NVIC
    unsafe {
        cortex_m::peripheral::NVIC::unmask(rp_pac::Interrupt::PWM_IRQ_WRAP_0); // or PWM_IRQ_WRAP depending on your PAC version
    }

    // 3. set the pwm wrap interrupt to the highest priority
    rp_pac::interrupt::PWM_IRQ_WRAP_0.set_priority(embassy_rp::interrupt::Priority::P3);

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

//Current pwm frequency should be ~ 20khz, we want the adc read for current sense
// to be aligned on the center of the pwm with a 1ms refresh rate which should
// equate to every 80 cycles
#[interrupt]
fn PWM_IRQ_WRAP_0() {
    //If its one of the H bridge slices then check whether we've hit the 1ms count
    let cur_cnt = PWM_INT_CNT.load(Relaxed);

    if cur_cnt >= 81 {
        // --- 1. SET DEBUG PIN HIGH (GPIO 8) ---
        debug_high(8);

        PWM_INT_CNT.store(0, Relaxed);
        // --- INSTANT ADC READ (PAC LEVEL) ---
        // 1. Select ADC input channel for PIN_26 (which is ADC channel 0)
        // (GPIO 26 = ADC0, GPIO 27 = ADC1, GPIO 28 = ADC2)
        rp_pac::ADC.cs().modify(|w| {
            // w.set_en(true); // Select channel 0 for pin 26
            w.set_ainsel(0);
            w.set_start_once(true); // Trigger a single conversion
        });

        // 2. Poll until conversion is complete (takes ~2us, safe inside an ISR)
        while !rp_pac::ADC.cs().read().ready() {
            // Spin-wait briefly for hardware flag
        }

        // 3. Read the 12-bit result register
        let good_v = rp_pac::ADC.result().read().result();
        I_INTR.signal(good_v);

        // --- 3. SET DEBUG PIN LOW (GPIO 8) ---
        debug_low(8);
    } else {
        PWM_INT_CNT.fetch_add(1, Relaxed);
    }
    // Check which slice triggered the wrap and clear its interrupt flag
    let ints = rp_pac::PWM.irq0_ints().read();

    if ints.ch1() {
        rp_pac::PWM.intr().write(|w| w.set_ch1(true));
    }

    if ints.ch7() {
        rp_pac::PWM.intr().write(|w| w.set_ch7(true));
    }
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
