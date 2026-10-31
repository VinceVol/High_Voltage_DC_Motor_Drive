#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use hv_moto_coto::motor::run_motor;
use hv_moto_coto::{adc_runner, cap_precharge, direction, mesa};
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};

use hv_moto_coto::bus_init::{i2c_init, spi_init};
use hv_moto_coto::speed_sense::run_hall;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Initialize RP2350 peripherals/i2c/spi
    let p = embassy_rp::init(Default::default());
    let _spi_bus = spi_init(p.SPI1, p.PIN_11, p.PIN_12, p.PIN_10, p.DMA_CH1, p.DMA_CH0);
    info!("SPI initialized on RP2350!");
    let _i2c_bus = i2c_init(p.I2C1, p.PIN_18, p.PIN_19);
    info!("I2C initialized on RP2350!");

    // Configure GPIO 25 (onboard LED on Pico 2 W) as an output pin
    // let mut led = Output::new(p.PIN_20, Level::Low);
    let _ = spawner.spawn(run_hall(p.PIN_6.into()).unwrap());
    info!("Hall sense initiated");
    let _ = spawner.spawn(adc_runner::adc_runner(p.PIN_26, p.PIN_28, p.PIN_27, p.ADC).unwrap());
    info!("adc runner initiated");
    let _ = spawner.spawn(cap_precharge::run_precharge().unwrap());
    info!("cap precharge initiated");
    let _ = spawner.spawn(direction::run_dir(p.PIN_21, p.PIN_16).unwrap());
    info!("direction pin control initiated");
    let _ = spawner.spawn(mesa::read_mesa_dir(p.PIN_0).unwrap());
    info!("mesa read direction pin initiated");
    let _ = spawner
        .spawn(hv_moto_coto::pwm::run_pwm(p.PIN_22, p.PIN_17, p.PWM_SLICE3, p.PWM_SLICE0).unwrap());
    let _ = spawner.spawn(run_motor(p.PIN_20, p.PIN_1).unwrap());

    loop {
        // info!("LED ON");
        // led.set_high();
        Timer::after(Duration::from_millis(3000)).await;

        // info!("LED OFF");
        // led.set_low();
        Timer::after(Duration::from_millis(3000)).await;
    }
}
