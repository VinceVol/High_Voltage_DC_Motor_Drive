use defmt::Format;
use embassy_time::{Duration, Instant, Ticker, Timer};
//tasks related to interfacing with the rotary encoder/ button
use modulino::async_feature::Knob;

use crate::{bus_init::TwiDevice, BUTTON_SIGNAL};

const BUT_REF_MS: u64 = 10;
const BUT_DBNC_MS: u64 = 200;
const DBL_CLICK_TIME_MS: u64 = 1000;

#[derive(Copy, Clone, Format)]
pub enum ButtonSignal {
    LongPress,
    SingleClick,
    DoubleClick,
    GoRight,
    GoLeft,
}

#[embassy_executor::task]
pub async fn button_runner(i2c_p: TwiDevice) {
    let mut knob = Knob::new(i2c_p).await.expect("Unable to talk to knob");

    // Set a range for the encoder value
    knob.clear_range();

    //Set up ticker for button refresh rate
    let mut button_ticker = Ticker::every(Duration::from_millis(BUT_REF_MS));
    let mut encoder_timer = Instant::now();

    loop {
        button_ticker.next().await;

        match knob.update().await {
            Ok(val_changed) => {
                //skip checking if nothing has changed
                if !val_changed {
                    continue;
                }
                //Check for encoder turn clockwise or anti clockwise first-----
                match knob
                    .direction(encoder_timer.elapsed().as_millis() as u32)
                    .await
                    .expect("Button direction read error!")
                {
                    1 => BUTTON_SIGNAL.signal(ButtonSignal::GoRight),
                    -1 => BUTTON_SIGNAL.signal(ButtonSignal::GoLeft),
                    _ => (),
                }
                //knob abstraction already does a debounce check and requires time passed
                encoder_timer = Instant::now();

                //Check if the button on the knob was clicked next
                if knob.pressed() {
                    Timer::after_millis(BUT_DBNC_MS).await;
                    let _ = knob.update().await;
                    if knob.pressed() {
                        BUTTON_SIGNAL.signal(ButtonSignal::LongPress);
                    } else {
                        BUTTON_SIGNAL.signal(ButtonSignal::SingleClick);
                    }
                }
            }
            Err(_e) => defmt::info!("Error reading knob!"),
        }
        if knob.update().await.expect("Bad coms with rotary encoder") {
            defmt::info!("Value: {}, Pressed: {}", knob.value(), knob.pressed());
        }
    }
}
// --- 1. SET DEBUG PIN HIGH (GPIO 8) ---
// debug_high(8);
// --- 3. SET DEBUG PIN LOW (GPIO 8) ---
// debug_low(8);
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
