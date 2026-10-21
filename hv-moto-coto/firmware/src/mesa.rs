// Here to send communications from mesa over channels for the main loop to react to

use embassy_rp::{
    adc::{Adc, Channel, Config},
    bind_interrupts,
    gpio::Input,
    peripherals::{ADC, PIN_0, PIN_27},
    Peri,
};
use embassy_time::{Duration, Timer};

use crate::{
    CHANGE_THRESHOLD, MAX_SPINDLE_SPEED, MESA_DIR_REQ_SIG, MESA_MON_DEL_MS, MESA_RPM_REQ_SIG,
    RPM_BFR_DIR_SWTCH,
};
use defmt::info;

#[embassy_executor::task]
pub async fn read_mesa_adc(
    spindle_set: Peri<'static, PIN_27>, //pin 27
    adc_p: Peri<'static, ADC>,
) {
    bind_interrupts!(struct Irqs {
        ADC_IRQ_FIFO => embassy_rp::adc::InterruptHandler;
    });

    let mut spindle_cmd = Channel::new_pin(spindle_set, embassy_rp::gpio::Pull::None);

    let config = Config::default();
    let mut adc_v_in = Adc::new(adc_p, Irqs, config);

    //Check the adc at MESA_MON_DEL_MS frequency and if the voltage has changed past a given
    // threshold then send a updated speed to the REQ signal line
    let mut last_set_pnt = 0;
    loop {
        Timer::after(Duration::from_millis(MESA_MON_DEL_MS)).await;
        match adc_v_in.read(&mut spindle_cmd).await {
            Ok(good_v) => {
                if (good_v as i16 - last_set_pnt as i16).abs() > CHANGE_THRESHOLD {
                    last_set_pnt = good_v;
                    //sending the converted 4095 max to rpm max
                    MESA_RPM_REQ_SIG.signal((MAX_SPINDLE_SPEED * good_v) / 4095);
                }
            }
            Err(_adc_error) => {
                info!("Error reading adc pin -- req rpm");
            }
        }
    }
}

#[embassy_executor::task]
pub async fn read_mesa_dir(dir_set: Peri<'static, PIN_0>) {
    let mut dir_in = Input::new(dir_set, embassy_rp::gpio::Pull::None);
    loop {
        dir_in.wait_for_any_edge().await;
        match dir_in.get_level() {
            embassy_rp::gpio::Level::High => {
                MESA_DIR_REQ_SIG.signal(crate::direction::Direction::Forward)
            }
            embassy_rp::gpio::Level::Low => {
                MESA_DIR_REQ_SIG.signal(crate::direction::Direction::Reverse)
            }
        }
    }
}
