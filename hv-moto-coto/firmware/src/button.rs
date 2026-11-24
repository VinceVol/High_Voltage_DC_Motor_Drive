use embassy_time::{Duration, Instant, Ticker, Timer};
//tasks related to interfacing with the rotary encoder/ button
use modulino::async_feature::Knob;

use crate::{bus_init::TwiDevice, BUTTON_SIGNAL};

const BUT_REF_MS: u64 = 10;
const BUT_DBNC_MS: u64 = 30;
const DBL_CLICK_TIME_MS: u64 = 1000;

#[derive(Copy, Clone)]
pub enum ButtonSignal {
    LongPress,
    SingleClick,
    DoubleClick,
    GoRight,
    GoLeft,
}

#[embassy_executor::task]
pub async fn button_runner(i2c_p: TwiDevice) {
    let mut knob = Knob::new(i2c_p).await.unwrap();

    // Set a range for the encoder value
    knob.clear_range();

    //Set up ticker for button refresh rate
    let mut button_ticker = Ticker::every(Duration::from_millis(BUT_REF_MS));
    let mut encoder_timer = Instant::now();
    let mut last_click_timer = Instant::now();

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
                    _ => continue,
                }
                //knob abstraction already does a debounce check and requires time passed
                encoder_timer = Instant::now();

                //Check if the button on the knob was clicked next
                if knob.pressed() {
                    Timer::after_millis(BUT_DBNC_MS).await;
                    let _ = knob.update().await;
                    if knob.pressed() {
                        BUTTON_SIGNAL.signal(ButtonSignal::LongPress);
                    } else if last_click_timer.elapsed().as_millis() < DBL_CLICK_TIME_MS {
                        BUTTON_SIGNAL.signal(ButtonSignal::DoubleClick);
                    }
                }
                last_click_timer = Instant::now();
            }
            Err(_e) => defmt::info!("Error reading knob!"),
        }
        if knob.update().await.expect("Bad coms with rotary encoder") {
            defmt::info!("Value: {}, Pressed: {}", knob.value(), knob.pressed());
        }
    }
}
