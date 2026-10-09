#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp as hal;
use embassy_rp::block::ImageDef;
use embassy_rp::gpio::Output;
use embassy_time::Timer;

//Panic Handler
use panic_probe as _;
// Defmt Logging
use defmt_rtt as _;

// mod bus_init;

/// Tell the Boot ROM about our application
#[unsafe(link_section = ".start_block")]
#[used]
pub static IMAGE_DEF: ImageDef = hal::block::ImageDef::secure_exe();

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // Configure GPIO25 (onboard LED on standard Pico) as a digital output starting LOW
    let mut led = Output::new(p.PIN_25, embassy_rp::gpio::Level::Low);

    let mut count: u32 = 0;

    loop {
        // Toggle LED state
        led.toggle();

        // Print over RTT (captured via `probe-rs run` or `defmt-print`)
        defmt::info!("Tick #{} - LED is now active", count);
        count = count.wrapping_add(1);

        // Wait 1 second (1000 ms) before the next iteration
        Timer::after_secs(1).await;
    }
}

// Program metadata for `picotool info`.
// This isn't needed, but it's recommended to have these minimal entries.
#[unsafe(link_section = ".bi_entries")]
#[used]
pub static PICOTOOL_ENTRIES: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"hv-moto-coto"),
    embassy_rp::binary_info::rp_program_description!(c"your program description"),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];

// End of file
