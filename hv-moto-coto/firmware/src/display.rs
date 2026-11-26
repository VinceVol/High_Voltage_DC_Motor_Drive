//This project is using the SSD1306 oled and this section is dedicated to running that screen

use core::fmt::write;

use defmt::{info, Format};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_futures::select::{select3, Either3};
use embassy_rp::pio::program::RP2040_MAX_PROGRAM_SIZE;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Receiver,mutex::Mutex};
use embassy_time::{Duration, Timer};
use embedded_graphics::{
    geometry::Point,
    mono_font::{
        ascii::{FONT_10X20, FONT_6X10, FONT_8X13_BOLD, FONT_8X13_ITALIC},
        iso_8859_1::{FONT_4X6, FONT_6X12},
        MonoTextStyle, MonoTextStyleBuilder,
    },
    pixelcolor::BinaryColor,
    text::{Baseline, Text, TextStyle},
    Drawable,
};
use embedded_menu::{
    interaction::{Action, Interaction, Navigation},
    theme, Menu, MenuStyle, SelectValue,
};
use heapless::String;
use ssd1306::{
    mode::{BufferedGraphicsModeAsync, DisplayConfigAsync},
    rotation::DisplayRotation,
    size::DisplaySize128x32,
    I2CDisplayInterface, Ssd1306Async,
};
use embassy_rp::{
    flash::{Async, Flash},
    peripherals::FLASH,
};

use crate::{
    BUTTON_SIGNAL, FLASH_MUT_SIZE, I_WATCH, MAX_CURRENT_AMPS, MAX_SPINDLE_SPEED, MOTOR_MODE, RPM_FILT_WATCH, RPM_RAW_WATCH, RPM_REQ_WATCH, SAFETY_WATCH, bus_init::TwiDevice, button::ButtonSignal, safety::ControllerCond, saved_param::ControllerConfig,
};

#[derive(Copy, Clone, PartialEq, Eq)]
enum ScreenOptions {
    Rpm,
    Current,
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

#[embassy_executor::task]
pub async fn display_runner(i2c_p: TwiDevice,
    flash_mut: &'static Mutex<
        CriticalSectionRawMutex,
        Flash<'static, FLASH, Async, { FLASH_MUT_SIZE }>,
    >,

) {
    let interface = I2CDisplayInterface::new(i2c_p);
    let mut display = Ssd1306Async::new(interface, DisplaySize128x32, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();

    display.init().await.expect("SSD1306 init failed");

    //keep track on incoming information
    let mut safety_watch = SAFETY_WATCH.receiver().unwrap();
    let mut i_watch = I_WATCH.receiver().unwrap();
    let mut rpm_watch = RPM_RAW_WATCH.receiver().unwrap();

    //Loop and check based on new knob info/ new rpm info or a new safety trip
    loop {
        enter_main_screen(
            &mut rpm_watch,
            &mut i_watch,
            &mut safety_watch,
            &mut display,
        )
        .await;
    }
}
pub async fn enter_main_screen<'a, DI>(
    rpm_raw_watch: &mut Receiver<'a, CriticalSectionRawMutex, u16, 100>,
    i_watch: &mut Receiver<'a, CriticalSectionRawMutex, f32, 5>,
    safety_watch: &mut Receiver<'a, CriticalSectionRawMutex, ControllerCond, 5>,
    display: &mut Ssd1306Async<DI, DisplaySize128x32, BufferedGraphicsModeAsync<DisplaySize128x32>>,
) where
    DI: display_interface::AsyncWriteOnlyDataCommand,
{
    //Big RPM and AMPS stlye and the buf we update with this info
    let main_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(BinaryColor::On)
        .build();
    let mut main_buf: String<24> = String::new();

    //Sub text marking the rpm setpoint and current max
    let mini_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X12)
        .text_color(BinaryColor::On)
        .build();
    let mut mini_buf: String<24> = String::new();

    //Use selected screen to know what to upload to the buf
    let mut selected_screen = ScreenOptions::Rpm;

    let mut last_rpm = 0;
    let mut last_rpm_stpt = 0;
    let mut last_current = 0f32;
    let mut last_cond = ControllerCond::Ok;

    loop {
        //Refresh display ---------------------------
        display.clear_buffer();
        let _ = Text::with_baseline(main_buf.as_str(), Point::zero(), main_style, Baseline::Top)
            .draw(display);
        let _ = Text::with_baseline(
            mini_buf.as_str(),
            Point::zero(),
            mini_style,
            Baseline::Bottom,
        )
        .draw(display);
        display.flush().await.unwrap();
        //Refresh display ---------------------------

        //Check for inputs or new rpm data
        match select3(
            rpm_raw_watch.changed(),
            BUTTON_SIGNAL.wait(),
            safety_watch.changed(),
        )
        .await
        {
            Either3::First(new_rpm) => {
                last_rpm = new_rpm;
                last_current = i_watch.get().await;
            }
            Either3::Second(btn) => {
                defmt::info!("Got button signal: {:?}", btn);

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
                        enter_menu(rpm_raw_watch, i_watch, safety_watch, display).await;
                    }
                    ButtonSignal::SingleClick => {
                        selected_screen = match selected_screen {
                            ScreenOptions::Rpm => ScreenOptions::Current,
                            ScreenOptions::Current => ScreenOptions::Rpm,
                            ScreenOptions::Error => ScreenOptions::Error,
                        }
                    }
                    //IF the motor is in manual mode the setpoing is set within this loop
                    ButtonSignal::GoRight => {
                        if !MOTOR_MODE.load(core::sync::atomic::Ordering::Relaxed) {
                            last_rpm_stpt += 20;
                            last_rpm_stpt = last_rpm_stpt.clamp(0, MAX_SPINDLE_SPEED);
                            RPM_REQ_WATCH.sender().send(last_rpm_stpt);
                        }
                    }
                    ButtonSignal::GoLeft => {
                        if !MOTOR_MODE.load(core::sync::atomic::Ordering::Relaxed) {
                            last_rpm_stpt -= 20;
                            last_rpm_stpt = last_rpm_stpt.clamp(0, MAX_SPINDLE_SPEED);
                            RPM_REQ_WATCH.sender().send(last_rpm_stpt);
                        }
                    }
                    _ => (),
                }
            }
            Either3::Third(cont_cond) => match cont_cond {
                ControllerCond::NG(_err) => {
                    selected_screen = ScreenOptions::Error;
                    last_cond = cont_cond;
                }
                _ => (),
            },
        }

        match selected_screen {
            ScreenOptions::Rpm => {
                main_buf.clear();
                mini_buf.clear();
                let _ = write(&mut main_buf, format_args!("RPM:{}", last_rpm));
                let _ = write(&mut mini_buf, format_args!("Setpoint:{}", last_rpm_stpt));
            }
            ScreenOptions::Current => {
                main_buf.clear();
                mini_buf.clear();
                let _ = write(&mut main_buf, format_args!("AMPS:{:.3}", last_current));
                let _ = write(
                    &mut mini_buf,
                    format_args!("Max Current:{} Amps", MAX_SPINDLE_SPEED),
                );
            }
            ScreenOptions::Error => {
                main_buf.clear();
                mini_buf.clear();
                if let ControllerCond::NG(error) = last_cond {
                    let _ = write(&mut main_buf, format_args!("ERROR: {}", error.as_msg()));
                }
            }
        }
    }
}

pub async fn enter_menu<'a, DI>(
    rpm_raw_watch: &mut Receiver<'a, CriticalSectionRawMutex, u16, 100>,
    i_watch: &mut Receiver<'a, CriticalSectionRawMutex, f32, 5>,
    safety_watch: &mut Receiver<'a, CriticalSectionRawMutex, ControllerCond, 5>,
    display: &mut Ssd1306Async<DI, DisplaySize128x32, BufferedGraphicsModeAsync<DisplaySize128x32>>,
) where
    DI: display_interface::AsyncWriteOnlyDataCommand,
{
    //We want to modify the controller configuration in this menu so we can start with the
    // current one loaded up
    let current_config = ControllerConfig::load()
    
    //Build the menu
    let style = MenuStyle::new(BinaryColor::On)
        .with_font(&FONT_8X13_ITALIC)
        .with_title_font(&FONT_4X6);

    let mut menu = Menu::with_style("Settings", style)
        .add_item("Manual Tune", "", |_| MenuOptions::ManualTune)
        .add_item("Autotune", "", |_| MenuOptions::Autotune)
        .add_item("Set Rpm Range", "", |_| MenuOptions::SetRpmRange)
        .add_item(
            "Set Mode",
            if MOTOR_MODE.load(core::sync::atomic::Ordering::Relaxed) {
                "Auto"
            } else {
                "Manual"
            },
            |_| {
                //Swap the motor mode within the menu
                let current_mode = MOTOR_MODE.load(core::sync::atomic::Ordering::Relaxed);
                if current_mode == true {
                    MOTOR_MODE.store(false, core::sync::atomic::Ordering::Relaxed);
                } else {
                    MOTOR_MODE.store(true, core::sync::atomic::Ordering::Relaxed);
                }
                MenuOptions::SetManualAuto
            },
        )
        .add_item("Exit", "", |_| MenuOptions::GoBack)
        .build();

    //keep track of what's been pressed
    let mut selected_value: MenuOptions = MenuOptions::None;
    let mut selected = selected_value;

    loop {
        //refresh display
        if menu.update(display) {
            display.clear_buffer();
            menu.draw(display).unwrap();
            display.flush().await.unwrap();
        }
        let new_btn_sig = BUTTON_SIGNAL.wait().await;
        defmt::info!("Got button signal: {:?}", new_btn_sig);
        //check for inputs
        match new_btn_sig {
            ButtonSignal::SingleClick => {
                menu.interact(Interaction::Action(Action::Select));
                selected = menu.selected_value();
            }
            ButtonSignal::GoRight => {
                menu.interact(Interaction::Navigation(Navigation::Previous));
            }
            ButtonSignal::GoLeft => {
                menu.interact(Interaction::Navigation(Navigation::Next));
            }
            _ => (),
        }

        defmt::info!("New selected value: {}", selected);
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
