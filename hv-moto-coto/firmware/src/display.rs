//This project is using the SSD1306 oled and this section is dedicated to running that screen

use embedded_graphics::{
    geometry::Point,
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    text::{Baseline, Text},
    Drawable,
};
use ssd1306::{
    mode::DisplayConfigAsync, rotation::DisplayRotation, size::DisplaySize128x32,
    I2CDisplayInterface, Ssd1306Async,
};

use crate::bus_init::TwiDevice;

enum ScreenOptions {
    RpmWCurrent,
    Menu,
    Error,
}

enum MenuOptions {
    ManualTune,
    Autotune,
    SetRpmRange,
}

#[embassy_executor::task]
pub async fn display_runner(i2c_p: TwiDevice) {
    let interface = I2CDisplayInterface::new(i2c_p);
    let mut display = Ssd1306Async::new(interface, DisplaySize128x32, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();

    display.init().await.expect("SSD1306 init failed");
    display.clear_buffer();

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();

    Text::with_baseline("Hello world!", Point::zero(), text_style, Baseline::Top)
        .draw(&mut display)
        .unwrap();

    Text::with_baseline("Hello Rust!", Point::new(0, 16), text_style, Baseline::Top)
        .draw(&mut display)
        .unwrap();

    display.flush().await.expect("SSD1306 flush failed");

    todo!();
}
