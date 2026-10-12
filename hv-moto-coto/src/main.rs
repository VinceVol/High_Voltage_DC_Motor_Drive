#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_time::{Duration, Timer};

use crate::bus_init::{i2c_init, spi_init};

mod bus_init;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // Initialize RP2350 peripherals/i2c/spi
    let p = embassy_rp::init(Default::default());
    let _spi_bus = spi_init(p.SPI1, p.PIN_11, p.PIN_12, p.PIN_10, p.DMA_CH1, p.DMA_CH0);
    info!("SPI initialized on RP2350!");
    let _i2c_bus = i2c_init(p.I2C1, p.PIN_18, p.PIN_19);
    info!("I2C initialized on RP2350!");

    // Configure GPIO 25 (onboard LED on Pico 2 W) as an output pin
    let mut led = Output::new(p.PIN_20, Level::Low);

    loop {
        info!("LED ON");
        led.set_high();
        Timer::after(Duration::from_millis(500)).await;

        info!("LED OFF");
        led.set_low();
        Timer::after(Duration::from_millis(500)).await;
    }
}
