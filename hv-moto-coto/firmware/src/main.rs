#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_rp::gpio::{Input, Output};
use hv_moto_coto::autotune::atune_btn_run;
use hv_moto_coto::button::button_runner;
use hv_moto_coto::display::display_runner;
use hv_moto_coto::motor::run_motor;
use hv_moto_coto::safety::{are_we_safe, be_safe};
use hv_moto_coto::saved_param::ControllerConfig;
use hv_moto_coto::{adc_runner, mesa};
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};

use hv_moto_coto::bus_init::{flash_init, i2c_init, spi_init};
use hv_moto_coto::speed_sense::run_hall;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Initialize RP2350 peripherals/i2c/spi
    let p = embassy_rp::init(Default::default());
    let _spi_bus = spi_init(p.SPI1, p.PIN_11, p.PIN_12, p.PIN_10, p.DMA_CH1, p.DMA_CH0);
    info!("SPI initialized on RP2350!");
    let i2c_bus = i2c_init(p.I2C1, p.PIN_18, p.PIN_19);
    let knob_i2c = I2cDevice::new(i2c_bus);
    let disp_i2c = I2cDevice::new(i2c_bus);
    info!("I2C initialized on RP2350!");
    let flash_p = flash_init(p.FLASH, p.DMA_CH2);

    //Initialize all the relavent atomics based on flash mem
    ControllerConfig::init_atomics(flash_p).await;

    //IMPORTANT -- Layout screwup means all these pins need to be useless inputs --IMPORTANT
    let _gp16 = Input::new(p.PIN_16, embassy_rp::gpio::Pull::None);
    let _gp17 = Input::new(p.PIN_17, embassy_rp::gpio::Pull::None);
    let _gp21 = Input::new(p.PIN_21, embassy_rp::gpio::Pull::None);
    let _gp22 = Input::new(p.PIN_22, embassy_rp::gpio::Pull::None);
    //IMPORTANT

    let debug_pin = Output::new(p.PIN_8, embassy_rp::gpio::Level::Low);
    core::mem::forget(debug_pin); // Keeps pin configured, avoids ownership headaches

    // Configure GPIO 25 (onboard LED on Pico 2 W) as an output pin
    // let mut led = Output::new(p.PIN_20, Level::Low);
    let _ = spawner.spawn(run_hall(p.PIN_6.into()).unwrap());
    info!("Hall sense initiated");
    let _ = spawner.spawn(adc_runner::adc_runner(p.PIN_28, p.PIN_27, p.ADC).unwrap());
    info!("adc runner initiated");
    let _ = spawner.spawn(be_safe(p.PIN_9, p.PIN_1).unwrap());
    let _ = spawner.spawn(are_we_safe(p.PIN_20).unwrap());
    let _ = spawner.spawn(mesa::read_mesa_dir(p.PIN_0).unwrap());
    info!("mesa read direction pin initiated");
    let _ = spawner.spawn(
        hv_moto_coto::pwm::run_pwm(
            p.PIN_2,
            p.PIN_3,
            p.PIN_14,
            p.PIN_15,
            p.PWM_SLICE1,
            p.PWM_SLICE7,
        )
        .unwrap(),
    );
    let _ = spawner.spawn(run_motor(flash_p).unwrap());
    let _ = spawner.spawn(atune_btn_run(p.PIN_7, flash_p).unwrap());

    let _ = spawner.spawn(button_runner(knob_i2c).unwrap());
    let _ = spawner.spawn(display_runner(disp_i2c, flash_p).unwrap());

    loop {
        // info!("LED ON");
        // led.set_high();
        Timer::after(Duration::from_millis(3000)).await;

        // info!("LED OFF");
        // led.set_low();
        Timer::after(Duration::from_millis(3000)).await;
    }
}
