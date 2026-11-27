//This project is using the SSD1306 oled and this section is dedicated to running that screen

use core::fmt::write;

use defmt::Format;
use embassy_futures::select::{select3, Either3};
use embassy_rp::rom_data::reboot;
use embassy_rp::{
    flash::{Async, Flash},
    peripherals::FLASH,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, watch::Receiver};
use embedded_graphics::{
    geometry::Point,
    mono_font::{
        ascii::{FONT_10X20, FONT_8X13_ITALIC},
        iso_8859_1::{FONT_4X6, FONT_6X12},
        MonoTextStyleBuilder,
    },
    pixelcolor::BinaryColor,
    text::{Baseline, Text},
    Drawable,
};
use embedded_menu::{
    interaction::{Action, Interaction, Navigation},
    Menu, MenuStyle, SelectValue,
};
use heapless::String;
use ssd1306::{
    mode::{BufferedGraphicsModeAsync, DisplayConfigAsync},
    rotation::DisplayRotation,
    size::DisplaySize128x32,
    I2CDisplayInterface, Ssd1306Async,
};

use crate::MAX_CURRENT_MA;
use crate::{
    bus_init::TwiDevice, button::ButtonSignal, safety::ControllerCond,
    saved_param::ControllerConfig, BUTTON_SIGNAL, FLASH_MUT_SIZE, I_WATCH, MAX_SPINDLE_SPEED,
    MOTOR_MODE, RPM_RAW_WATCH, RPM_REQ_WATCH, SAFETY_WATCH,
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
    SetIRange,
    SetManualAuto,
    GoBack,
    None,
}

#[embassy_executor::task]
pub async fn display_runner(
    i2c_p: TwiDevice,
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
            flash_mut,
        )
        .await;
    }
}
pub async fn enter_main_screen<'a, DI>(
    rpm_raw_watch: &mut Receiver<'a, CriticalSectionRawMutex, u16, 100>,
    i_watch: &mut Receiver<'a, CriticalSectionRawMutex, f32, 5>,
    safety_watch: &mut Receiver<'a, CriticalSectionRawMutex, ControllerCond, 5>,
    display: &mut Ssd1306Async<DI, DisplaySize128x32, BufferedGraphicsModeAsync<DisplaySize128x32>>,
    flash_mut: &'static Mutex<
        CriticalSectionRawMutex,
        Flash<'static, FLASH, Async, { FLASH_MUT_SIZE }>,
    >,
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
                        enter_menu(rpm_raw_watch, i_watch, safety_watch, display, flash_mut).await;
                        //reset the chip so that all the firmware boots up with the new settings
                        reboot(0x00, 50, 0, 0);
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
                            last_rpm_stpt = last_rpm_stpt.clamp(
                                0,
                                MAX_SPINDLE_SPEED.load(core::sync::atomic::Ordering::Relaxed),
                            );
                            RPM_REQ_WATCH.sender().send(last_rpm_stpt);
                        }
                    }
                    ButtonSignal::GoLeft => {
                        if !MOTOR_MODE.load(core::sync::atomic::Ordering::Relaxed) {
                            last_rpm_stpt -= 20;
                            last_rpm_stpt = last_rpm_stpt.clamp(
                                0,
                                MAX_SPINDLE_SPEED.load(core::sync::atomic::Ordering::Relaxed),
                            );
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
                let _ = write(
                    &mut main_buf,
                    format_args!("AMPS:{:.3}", last_current / 1000f32),
                );
                let _ = write(
                    &mut mini_buf,
                    format_args!(
                        "Max Current:{} Amps",
                        MAX_SPINDLE_SPEED.load(core::sync::atomic::Ordering::Relaxed)
                    ),
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
    _rpm_raw_watch: &mut Receiver<'a, CriticalSectionRawMutex, u16, 100>,
    _i_watch: &mut Receiver<'a, CriticalSectionRawMutex, f32, 5>,
    _safety_watch: &mut Receiver<'a, CriticalSectionRawMutex, ControllerCond, 5>,
    display: &mut Ssd1306Async<DI, DisplaySize128x32, BufferedGraphicsModeAsync<DisplaySize128x32>>,
    flash_mut: &'static Mutex<
        CriticalSectionRawMutex,
        Flash<'static, FLASH, Async, { FLASH_MUT_SIZE }>,
    >,
) where
    DI: display_interface::AsyncWriteOnlyDataCommand,
{
    //Build the menu
    let style = MenuStyle::new(BinaryColor::On)
        .with_font(&FONT_8X13_ITALIC)
        .with_title_font(&FONT_4X6);

    //Basically clicking on new menu items will open a new screen to allow you to adjust parameters
    // ex: |Set Max Rpm |
    // if you click this then it opens another screen where you adust this number then when
    let mut menu = Menu::with_style("Settings", style)
        .add_item("Manual Tune", "", |_| MenuOptions::ManualTune)
        .add_item("Autotune", "", |_| MenuOptions::Autotune)
        .add_item("Set Max Rpm", "", |_| MenuOptions::SetRpmRange)
        .add_item("Set Max Current", "", |_| MenuOptions::SetIRange)
        .add_item("Set Mode", "", |_| {
            //Swap the motor mode within the menu
            let current_mode = MOTOR_MODE.load(core::sync::atomic::Ordering::Relaxed);
            if current_mode == true {
                MOTOR_MODE.store(false, core::sync::atomic::Ordering::Relaxed);
            } else {
                MOTOR_MODE.store(true, core::sync::atomic::Ordering::Relaxed);
            }
            MenuOptions::SetManualAuto
        })
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

        //Do something with the recent menu click
        if selected != selected_value {
            selected_value = selected;
            match selected_value {
                //if the set rpm range menu is selected the n pull up the adjstment screen and store
                // its returned vale into the MAX_SPINDLE_SPEED mutex
                MenuOptions::SetRpmRange => MAX_SPINDLE_SPEED.store(
                    adjust_screen(
                        "--Max RPM Set--",
                        MAX_SPINDLE_SPEED.load(core::sync::atomic::Ordering::Relaxed) as f32,
                        display,
                    )
                    .await as u16,
                    core::sync::atomic::Ordering::Relaxed,
                ),
                MenuOptions::SetIRange => MAX_CURRENT_MA.store(
                    adjust_screen(
                        "--Max mA Set--",
                        MAX_CURRENT_MA.load(core::sync::atomic::Ordering::Relaxed) as f32,
                        display,
                    )
                    .await as u16,
                    core::sync::atomic::Ordering::Relaxed,
                ),
                MenuOptions::GoBack => {
                    //our opportunity to save all the menu changes to the flash and restart everything

                    //We want to modify the controller configuration in this menu so we can start with the
                    // current one loaded up
                    let mut current_config = ControllerConfig::load(flash_mut).await.unwrap();

                    //Update PID parameters
                    // TODO

                    //Update operating ranges
                    current_config.max_rpm =
                        MAX_SPINDLE_SPEED.load(core::sync::atomic::Ordering::Relaxed);
                    current_config.max_current_ma =
                        MAX_CURRENT_MA.load(core::sync::atomic::Ordering::Relaxed);

                    //Update manual vs auto mode
                    if MOTOR_MODE.load(core::sync::atomic::Ordering::Relaxed) {
                        current_config.man_mod = 1;
                    } else {
                        current_config.man_mod = 0;
                    }

                    //Save all new parameters
                    current_config.save(flash_mut).await.unwrap();
                    return;
                }
                _ => (),
            }
        }
    }
}

//TODO Refactor this to be any single number adjustment screen, make our lives easier
// Adjust some value and return the new value screen
pub async fn adjust_screen<DI>(
    adjusting_str: &'static str,
    starting_value: f32,
    display: &mut Ssd1306Async<DI, DisplaySize128x32, BufferedGraphicsModeAsync<DisplaySize128x32>>,
) -> f32
where
    DI: display_interface::AsyncWriteOnlyDataCommand,
{
    //Big number showing the value we're adjusting
    let main_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(BinaryColor::On)
        .build();
    let mut main_buf: String<24> = String::new();

    //Sub text reminder user what's being adjusted
    let mini_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X12)
        .text_color(BinaryColor::On)
        .build();

    let mut last_value = starting_value;

    loop {
        //Refresh display ---------------------------
        display.clear_buffer();
        let _ = Text::with_baseline(main_buf.as_str(), Point::zero(), main_style, Baseline::Top)
            .draw(display);
        let _ = Text::with_baseline(adjusting_str, Point::zero(), mini_style, Baseline::Bottom)
            .draw(display);
        display.flush().await.unwrap();
        //Refresh display ---------------------------

        match BUTTON_SIGNAL.wait().await {
            //Long press sets the value and goes back to main menu
            ButtonSignal::LongPress => {
                return last_value;
            }
            //IF the motor is in manual mode the setpoing is set within this loop
            ButtonSignal::GoRight => {
                last_value += 20f32;
            }
            ButtonSignal::GoLeft => {
                last_value -= 20f32;
            }
            _ => (),
        }
        main_buf.clear();
        let _ = write(&mut main_buf, format_args!("{}", last_value));
    }
}
