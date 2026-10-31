//Unfortunately the rp_2040 only really has one adc peripheral that it shares amongst multiple pins
// this means that all of the checking needs to happen in one task

use defmt::info;
use embassy_futures::select::{select3, Either3};
use embassy_rp::{
    adc::{Adc, Channel, Config},
    bind_interrupts,
    peripherals::{ADC, PIN_26, PIN_27, PIN_28},
    Peri,
};
use embassy_time::{Duration, Ticker};

use crate::{
    CHANGE_THRESHOLD, I_CORR_NUM, I_DEL_MS, I_WATCH, MAX_SPINDLE_SPEED, MESA_MON_DEL_MS,
    MESA_RPM_REQ_SIG, V_CORR_NUM, V_CORR_OFF, V_DEL_MS, V_WATCH,
};

#[embassy_executor::task]
pub async fn adc_runner(
    cur_sens_pin: Peri<'static, PIN_26>,
    vol_sens_pin: Peri<'static, PIN_28>,
    mesa_com_pin: Peri<'static, PIN_27>,
    adc_p: Peri<'static, ADC>,
) {
    //ADC Peripheral setup
    bind_interrupts!(struct Irqs {
        ADC_IRQ_FIFO => embassy_rp::adc::InterruptHandler;
    });
    let config = Config::default();
    let mut adc_v_in = Adc::new(adc_p, Irqs, config);

    //Seperate pins setup
    let mut current_in = Channel::new_pin(cur_sens_pin, embassy_rp::gpio::Pull::None);
    let mut voltage_in = Channel::new_pin(vol_sens_pin, embassy_rp::gpio::Pull::None);
    let mut spindle_cmd = Channel::new_pin(mesa_com_pin, embassy_rp::gpio::Pull::None);

    //set up tickers for each based on chosen delays
    let mut cur_ticker = Ticker::every(Duration::from_millis(I_DEL_MS));
    let mut vol_ticker = Ticker::every(Duration::from_millis(V_DEL_MS));
    let mut mesa_ticker = Ticker::every(Duration::from_millis(MESA_MON_DEL_MS));
    //Check the adc at MESA_MON_DEL_MS frequency and if the voltage has changed past a given
    // threshold then send a updated speed to the REQ signal line
    let mut last_set_pnt = 0;
    loop {
        //poll the tickers to act on the next one
        match select3(cur_ticker.next(), vol_ticker.next(), mesa_ticker.next()).await {
            Either3::First(_) => {
                match adc_v_in.read(&mut current_in).await {
                    Ok(good_v) => {
                        //sending the converted 4095 max to voltage max
                        I_WATCH
                            .sender()
                            .send(((I_CORR_NUM * good_v as u64) / 4095u64) as u16);
                        // info!(
                        //     "Current sense = {}",
                        //     ((I_CORR_NUM * good_v as u64) / 4095u64)
                        // );
                    }
                    Err(_adc_error) => {
                        info!("Error reading adc pin -- req current");
                    }
                }
            }
            Either3::Second(_) => {
                match adc_v_in.read(&mut voltage_in).await {
                    Ok(good_v) => {
                        //sending the converted 4095 max to voltage max
                        let signed_voltage = ((V_CORR_NUM * good_v as i64) / 4095i64) + V_CORR_OFF;
                        V_WATCH.sender().send(signed_voltage.abs() as u16);
                        // info!("voltage sense = {}", (signed_voltage));
                    }
                    Err(_adc_error) => {
                        info!("Error reading adc pin -- req voltage");
                    }
                }
            }
            Either3::Third(_) => {
                match adc_v_in.read(&mut spindle_cmd).await {
                    Ok(good_v) => {
                        if (good_v as i16 - last_set_pnt as i16).abs() > CHANGE_THRESHOLD {
                            last_set_pnt = good_v;
                            //sending the converted 4095 max to rpm max
                            MESA_RPM_REQ_SIG.signal(
                                ((MAX_SPINDLE_SPEED as u64 * good_v as u64) / 4095u64) as u16,
                            );
                        }
                    }
                    Err(_adc_error) => {
                        info!("Error reading adc pin -- req rpm");
                    }
                }
            }
        }
    }
}
