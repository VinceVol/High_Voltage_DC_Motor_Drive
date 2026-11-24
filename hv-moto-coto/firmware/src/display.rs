//This project is using the SSD1306 oled and this section is dedicated to running that screen

use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_futures::select::{Either3, select3};
use embassy_time::{Duration, Timer};
use embedded_graphics::{
    geometry::Point,
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    text::{Baseline, Text},
    Drawable,
};
use embedded_menu::{Menu, SelectValue, interaction::{Action, Interaction, Navigation}};
use ssd1306::{
    I2CDisplayInterface, Ssd1306Async, mode::{BufferedGraphicsModeAsync, DisplayConfigAsync}, rotation::DisplayRotation, size::DisplaySize128x32,
};

use crate::{BUTTON_SIGNAL, I_WATCH, RPM_FILT_WATCH, SAFETY_WATCH, bus_init::TwiDevice, button::ButtonSignal};

#[derive(Copy,Clone,PartialEq,Eq)]
enum ScreenOptions {
    RpmWCurrent,
    Menu,
    Error,
}

#[derive(Copy, Clone, PartialEq, Eq, SelectValue)]
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

    let mut screen = ScreenOptions::RpmWCurrent;
    let mut previous_screen = ScreenOptions::RpmWCurrent;
    let mut telemetry = Telemetry::default();
    let mut active_error: Option<&'static str> = None;
    let mut flash_visible = true;

    //keep track on incoming information
    let mut safety_watch = SAFETY_WATCH.receiver().unwrap();
    let mut i_watch = I_WATCH.receiver().unwrap();
    let mut rpm_watch = RPM_FILT_WATCH.receiver().unwrap();

    let mut menu = Menu::build("Mode")
        .add_item("Manual Tune", MenuOptions::ManualTune, |val| val)
        .add_item("Autotune", MenuOptions::Autotune, |val| val)
        .add_item("Set RPM Range", MenuOptions::SetRpmRange, |val| val)
        .build();

    let mut last_selected = menu.selected_value();

    draw_screen(
        &mut display,
        screen,
        telemetry,
        active_error,
        flash_visible,
        &mut menu,
    )
    .await;

    loop {
        let flash_delay = if screen == ScreenOptions::Error {
            Duration::from_millis(350)
        } else {
            // Long idle timer when not actively flashing an error
            Duration::from_secs(60)
        };

        match select3(
            BUTTON_SIGNAL.wait(),
            rpm_watch.changed(),
            safety_watch.changed(),
            )
        .await
        {
            Either3::First(btn) => {
                handle_button(
                    btn,
                    &mut screen,
                    &mut previous_screen,
                    &mut active_error,
                    &mut menu,
                    &mut last_selected,
                );
                flash_visible = true;
            }
            Either3::Second(new_telemetry) => {
                telemetry = new_telemetry;
            }
            Either3::Third(ErrorEvent::Signal(err_msg)) => {
                if let Some(msg) = err_msg {
                    if screen != ScreenOptions::Error {
                        previous_screen = screen;
                    }
                    active_error = Some(msg);
                    screen = ScreenOptions::Error;
                    flash_visible = true;
                } else {
                    // `Some(err)` sets an error; `None` clears it
                    active_error = None;
                    if screen == ScreenOptions::Error {
                        screen = previous_screen;
                    }
                }
            }
            Either3::Third(ErrorEvent::FlashTick) => {
                if screen == ScreenOptions::Error {
                    flash_visible = !flash_visible;
                }
            }
        }

        draw_screen(
            &mut display,
            screen,
            telemetry,
            active_error,
            flash_visible,
            &mut menu,
        )
        .await;
    }
}

enum ErrorEvent {
    Signal(Option<&'static str>),
    FlashTick,
}

fn handle_button(
    btn: ButtonSignal,
    screen: &mut ScreenOptions,
    previous_screen: &mut ScreenOptions,
    active_error: &mut Option<&'static str>,
    menu: &mut Menu<&str, ...>, // Replace with your concrete `embedded_menu::Menu` type or infer via `let _`
    last_selected: &mut MenuOptions,
) {
    match *screen {
        ScreenOptions::Error => match btn {
            // Allow single click or long press to dismiss/acknowledge the error display
            ButtonSignal::SingleClick | ButtonSignal::LongPress => {
                *active_error = None;
                *screen = *previous_screen;
            }
            _ => {}
        },

        ScreenOptions::RpmWCurrent => match btn {
            ButtonSignal::LongPress => {
                *previous_screen = ScreenOptions::RpmWCurrent;
                *screen = ScreenOptions::Menu;
            }
            _ => {}
        },

        ScreenOptions::Menu => match btn {
            ButtonSignal::GoRight => {
                let _ = menu.interact(Interaction::Navigation(Navigation::Next));
            }
            ButtonSignal::GoLeft => {
                let _ = menu.interact(Interaction::Navigation(Navigation::Previous));
            }
            ButtonSignal::SingleClick => {
                let _ = menu.interact(Interaction::Action(Action::Select));
                let selected = menu.selected_value();
                if selected != *last_selected {
                    *last_selected = selected;
                }
                apply_menu_selection(selected, screen);
            }
            ButtonSignal::DoubleClick | ButtonSignal::LongPress => {
                // Exit menu back to main RPM/Current screen
                *screen = ScreenOptions::RpmWCurrent;
            }
        },
    }
}

fn apply_menu_selection(selected: MenuOptions, screen: &mut ScreenOptions) {
    match selected {
        MenuOptions::ManualTune => {
            // Dispatch manual tune mode to your motor/control task here
            *screen = ScreenOptions::RpmWCurrent;
        }
        MenuOptions::Autotune => {
            // Dispatch autotune start to your motor/control task here
            *screen = ScreenOptions::RpmWCurrent;
        }
        MenuOptions::SetRpmRange => {
            // Switch to an RPM range sub-screen or return once configured
            *screen = ScreenOptions::RpmWCurrent;
        }
    }
}
async fn draw_screen<DI, M>(
    display: &mut Ssd1306Async<DI, DisplaySize128x32, BufferedGraphicsModeAsync<DisplaySize128x32>>,
    screen: ScreenOptions,
    telemetry: Telemetry,
    active_error: Option<&str>,
    flash_visible: bool,
    menu: &mut M,
) where
    M: embedded_graphics::Drawable<Color = BinaryColor>,
{
    display.clear_buffer();

    match screen {
        ScreenOptions::RpmWCurrent => {
            let rpm_style = MonoTextStyleBuilder::new()
                .font(&FONT_10X20)
                .text_color(BinaryColor::On)
                .build();

            let small_style = MonoTextStyleBuilder::new()
                .font(&FONT_6X10)
                .text_color(BinaryColor::On)
                .build();

            let mut rpm_buf: String<16> = String::new();
            let _ = write!(rpm_buf, "{} RPM", telemetry.rpm);

            let mut curr_buf: String<24> = String::new();
            let _ = write!(curr_buf, "I: {} mA", telemetry.current_ma);

            Text::with_baseline(&rpm_buf, Point::new(0, 0), rpm_style, Baseline::Top)
                .draw(display)
                .unwrap();

            Text::with_baseline(&curr_buf, Point::new(0, 22), small_style, Baseline::Top)
                .draw(display)
                .unwrap();
        }

        ScreenOptions::Menu => {
            // Note: depending on your `embedded-menu` version, call `menu.update(display)`
            // before `menu.draw(display)` if available.
            let _ = menu.draw(display);
        }

        ScreenOptions::Error => {
            if flash_visible {
                let err_style = MonoTextStyleBuilder::new()
                    .font(&FONT_6X10)
                    .text_color(BinaryColor::On)
                    .build();

                let header_style = MonoTextStyleBuilder::new()
                    .font(&FONT_10X20)
                    .text_color(BinaryColor::On)
                    .build();

                Text::with_baseline("ERROR", Point::new(0, 0), header_style, Baseline::Top)
                    .draw(display)
                    .unwrap();

                let msg = active_error.unwrap_or("FAULT");
                Text::with_baseline(msg, Point::new(0, 22), err_style, Baseline::Top)
                    .draw(display)
                    .unwrap();
            }
        }
    }

    display.flush().await.expect("SSD1306 flush failed");
}

