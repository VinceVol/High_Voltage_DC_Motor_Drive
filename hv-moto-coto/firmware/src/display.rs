//This project is using the SSD1306 oled and this section is dedicated to running that screen

use core::fmt::write;

use defmt::{info, Format};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_futures::select::{select3, Either3};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Receiver};
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

#[derive(Copy, Clone, PartialEq, Eq, SelectValue, Format)]
enum MenuOptions {
    ManualTune,
    Autotune,
    SetRpmRange,
    SetManualAuto,
    GoBack,
    None,
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

    //Loop and check based on new knob info/ new rpm info or a new safety trip
    loop {
        let btn = BUTTON_SIGNAL.wait().await;

        //For a knob change go thrugh the various scenarios to determine menu actions, quite bloated but the
        // statically typed menu TYPE is akward to hand off to other functions so for now this helps keep things
        // clean ish
        //MAIN RPM/ CURRENT Screen
        //
        // Depending on if the controller is in auto (takes commands from the mesa) or manual
        // mode, we need to react differently to the knob.
        //
        // AUTO
        // in auto the only functionality is entering the menu via a long press
        //
        // MANUAL
        // in manual you add the direction switch and speed stpnt adjustment
        match btn {
            ButtonSignal::LongPress => {
                enter_menu(
                    &mut rpm_watch,
                    &mut i_watch,
                    &mut safety_watch,
                    &mut display,
                )
                .await;
            }
            _ => (),
        }
    }
}
pub async fn enter_main_screen<'a, DI>(
    rpm_raw_watch: &mut Receiver<'a, CriticalSectionRawMutex, u16, 100>,
    i_watch: &mut Receiver<'a, CriticalSectionRawMutex, f32, 5>,
    safety_watch: &mut Receiver<'a, CriticalSectionRawMutex, ControllerCond, 5>,
    display: &mut Ssd1306Async<DI, DisplaySize128x32, BufferedGraphicsModeAsync<DisplaySize128x32>>,
) {
    let rpm_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(BinaryColor::On)
        .build();

    let small_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();

    let mut rpm_buf: String<16> = String::new();
    let _ = write(&mut rpm_buf, format_args!("{:10} RPM", telemetry.rpm));

    let mut curr_buf: String<24> = String::new();
    let _ = write(
        &mut curr_buf,
        format_args!("I: {:15} A", telemetry.current_a),
    );

    Text::with_baseline(&rpm_buf, Point::new(0, 0), rpm_style, Baseline::Top)
        .draw(display)
        .unwrap();

    Text::with_baseline(&curr_buf, Point::new(0, 22), small_style, Baseline::Top)
        .draw(display)
        .unwrap();
}

pub async fn enter_menu<'a, DI>(
    rpm_raw_watch: &mut Receiver<'a, CriticalSectionRawMutex, u16, 100>,
    i_watch: &mut Receiver<'a, CriticalSectionRawMutex, f32, 5>,
    safety_watch: &mut Receiver<'a, CriticalSectionRawMutex, ControllerCond, 5>,
    display: &mut Ssd1306Async<DI, DisplaySize128x32, BufferedGraphicsModeAsync<DisplaySize128x32>>,
) where
    DI: display_interface::AsyncWriteOnlyDataCommand,
{
    //Build the menu
    let mut menu = Menu::build("Mode")
        .add_item("Manual Tune", MenuOptions::ManualTune, |val| val)
        .add_item("Autotune", MenuOptions::Autotune, |val| val)
        .add_item("Set RPM Range", MenuOptions::SetRpmRange, |val| val)
        .add_item("Set Manual or Auto", MenuOptions::SetManualAuto, |val| val)
        .add_item("Go Back", MenuOptions::SetRpmRange, |val| val)
        .build();

    //keep track of what's been pressed
    let mut selected_value: MenuOptions = MenuOptions::None;

    loop {
        //refresh display
        if menu.update(display) {
            menu.draw(display).unwrap();
        }

        //check for inputs
        match BUTTON_SIGNAL.wait().await {
            ButtonSignal::SingleClick => {
                menu.interact(Interaction::Action(Action::Select)).unwrap();
            }
            ButtonSignal::GoRight => {
                menu.interact(Interaction::Navigation(Navigation::Next))
                    .unwrap();
            }
            ButtonSignal::GoLeft => {
                menu.interact(Interaction::Navigation(Navigation::Previous))
                    .unwrap();
            }
            _ => (),
        }

        let selected = menu.selected_value();
        if selected != selected_value {
            defmt::info!("Selected value: {}", selected);
            selected_value = selected;
            match selected_value {
                MenuOptions::None => (),
                MenuOptions::GoBack => {
                    return;
                }
                _ => (),
            }
        }
    }
}
