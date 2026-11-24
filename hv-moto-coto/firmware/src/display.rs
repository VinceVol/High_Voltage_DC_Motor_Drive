//This project is using the SSD1306 oled and this section is dedicated to running that screen

use defmt::Format;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_futures::select::{select3, Either3};
use embassy_time::{Duration, Timer};
use embedded_graphics::{
    geometry::Point,
    mono_font::{
        ascii::{FONT_10X20, FONT_6X10},
        MonoTextStyleBuilder,
    },
    pixelcolor::BinaryColor,
    text::{Baseline, Text},
    Drawable,
};
use embedded_menu::{
    interaction::{Action, Interaction, Navigation},
    Menu, SelectValue,
};
use heapless::String;
use ssd1306::{
    mode::{BufferedGraphicsModeAsync, DisplayConfigAsync},
    rotation::DisplayRotation,
    size::DisplaySize128x32,
    I2CDisplayInterface, Ssd1306Async,
};

use crate::{
    bus_init::TwiDevice, button::ButtonSignal, safety::ControllerCond, BUTTON_SIGNAL, I_WATCH,
    RPM_FILT_WATCH, RPM_RAW_WATCH, SAFETY_WATCH,
};

#[derive(Copy, Clone, PartialEq, Eq)]
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

#[derive(Copy, Clone, Debug, Default)]
pub struct Telemetry {
    pub rpm: u16,
    pub current_a: f32,
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
    let mut rpm_watch = RPM_RAW_WATCH.receiver().unwrap();

    //Build the menu
    let mut menu = Menu::build("Mode")
        .add_item("Manual Tune", MenuOptions::ManualTune, |val| val)
        .add_item("Autotune", MenuOptions::Autotune, |val| val)
        .add_item("Set RPM Range", MenuOptions::SetRpmRange, |val| val)
        .build();

    //Button handler -- easier to do this as a closure with access to the menu directly rather than creating another fn
    // that passes the menu -- unfortunate
    let mut last_selected = menu.selected_value();
    let mut handle_button = |btn: ButtonSignal| match screen {
        ScreenOptions::Error => match btn {
            ButtonSignal::SingleClick | ButtonSignal::LongPress => {
                active_error = None;
                screen = previous_screen;
            }
            _ => {}
        },

        ScreenOptions::RpmWCurrent => match btn {
            ButtonSignal::LongPress => {
                previous_screen = ScreenOptions::RpmWCurrent;
                screen = ScreenOptions::Menu;
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
                if selected != last_selected {
                    last_selected = selected;
                }
                apply_menu_selection(selected, &mut screen);
            }
            ButtonSignal::DoubleClick | ButtonSignal::LongPress => {
                screen = ScreenOptions::RpmWCurrent;
            }
        },
    };

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
                handle_button(btn);
                flash_visible = true;
            }
            Either3::Second(new_rpm) => {
                //the rpm/current display refreshes based on new rpm information
                // The current should refresh way faster than rpm but just in case take the old
                telemetry = Telemetry {
                    rpm: new_rpm,
                    current_a: i_watch.try_get().unwrap_or(telemetry.current_a),
                };
            }
            Either3::Third(controller_cond) => match controller_cond {
                ControllerCond::Ok | ControllerCond::Wait => {
                    previous_screen = screen;
                }
                ControllerCond::NG(controller_error) => {
                    active_error = Some(controller_error.as_msg());
                    screen = ScreenOptions::Error;
                    flash_visible = true;
                }
            },
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
    DI: display_interface::AsyncWriteOnlyDataCommand,
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
            let _ = write!(curr_buf, "I: {} A", telemetry.current_a);

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
            menu.draw(display);
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
