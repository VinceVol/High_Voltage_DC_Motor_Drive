//Current and Voltage sense tasks
// basically read the adc pins related to each and pump that information into a signal

use defmt::info;
use embassy_rp::{
    adc::{Adc, Channel, Config},
    bind_interrupts,
    peripherals::{ADC, PIN_26, PIN_28},
    Peri,
};
use embassy_time::{Duration, Timer};

use crate::{I_CORR_NUM, I_DEL_MS, I_WATCH, V_CORR_NUM, V_DEL_MS, V_WATCH};

#[embassy_executor::task]
pub async fn current_sense(cur_sens_pin: Peri<'static, PIN_26>, adc_p: Peri<'static, ADC>) {
    bind_interrupts!(struct Irqs {
        ADC_IRQ_FIFO => embassy_rp::adc::InterruptHandler;
    });

    let mut current_in = Channel::new_pin(cur_sens_pin, embassy_rp::gpio::Pull::None);

    let config = Config::default();
    let mut adc_v_in = Adc::new(adc_p, Irqs, config);
    loop {
        Timer::after(Duration::from_millis(I_DEL_MS)).await;
        match adc_v_in.read(&mut current_in).await {
            Ok(good_v) => {
                //sending the converted 4095 max to voltage max
                I_WATCH.sender().send((I_CORR_NUM * good_v) / 4095);
            }
            Err(_adc_error) => {
                info!("Error reading adc pin -- req current");
            }
        }
    }
}

#[embassy_executor::task]
pub async fn voltage_sense(vol_set_pin: Peri<'static, PIN_28>, adc_p: Peri<'static, ADC>) {
    bind_interrupts!(struct Irqs {
        ADC_IRQ_FIFO => embassy_rp::adc::InterruptHandler;
    });

    let mut voltage_in = Channel::new_pin(vol_set_pin, embassy_rp::gpio::Pull::None);

    let config = Config::default();
    let mut adc_v_in = Adc::new(adc_p, Irqs, config);
    loop {
        Timer::after(Duration::from_millis(V_DEL_MS)).await;
        match adc_v_in.read(&mut voltage_in).await {
            Ok(good_v) => {
                //sending the converted 4095 max to voltage max
                V_WATCH.sender().send((V_CORR_NUM * good_v) / 4095);
            }
            Err(_adc_error) => {
                info!("Error reading adc pin -- req voltage");
            }
        }
    }
}
