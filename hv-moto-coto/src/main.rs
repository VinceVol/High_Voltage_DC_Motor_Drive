#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_time::{Duration, Timer};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // Initialize RP2350 peripherals and clocks
    let p = embassy_rp::init(Default::default());
    info!("Embassy initialized on RP2350!");

    // Configure GPIO 25 (onboard LED on Pico 2 W) as an output pin
    let mut led = Output::new(p.PIN_25, Level::Low);

    loop {
        info!("LED ON");
        led.set_high();
        Timer::after(Duration::from_millis(500)).await;

        info!("LED OFF");
        led.set_low();
        Timer::after(Duration::from_millis(500)).await;
    }
}
