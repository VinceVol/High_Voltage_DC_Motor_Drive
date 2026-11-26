//Unfortunately the rp_2040 only really has one adc peripheral that it shares amongst multiple pins
// this means that all of the checking needs to happen in one task

use defmt::info;
use embassy_futures::select::{select3, Either3};
use embassy_rp::{
    adc::{Adc, Channel, Config},
    bind_interrupts,
    peripherals::{ADC, PIN_27, PIN_28},
    Peri,
};
use embassy_time::{Duration, Ticker};
use signal_filters::{MedianFilter5, Pt2Filterf32, SignalFilter};

use crate::{
    AUTOTUNE_OVERRIDE, CHANGE_THRESHOLD, I_CORR_NUM, I_CORR_OFF, I_INTR, I_REF_RATE_US, I_WATCH,
    MAX_SPINDLE_SPEED, MESA_MON_DEL_MS, MOTOR_MODE, RPM_REQ_WATCH, V_CORR_NUM, V_CORR_OFF,
    V_REF_RATE_US, V_WATCH,
};

#[embassy_executor::task]
pub async fn adc_runner(
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
    let mut voltage_in = Channel::new_pin(vol_sens_pin, embassy_rp::gpio::Pull::None);
    let mut spindle_cmd = Channel::new_pin(mesa_com_pin, embassy_rp::gpio::Pull::None);

    //set up tickers for each based on chosen delays
    let mut vol_ticker = Ticker::every(Duration::from_micros(V_REF_RATE_US));
    let mut mesa_ticker = Ticker::every(Duration::from_millis(MESA_MON_DEL_MS));

    //need to keep track of autotune to ignore the mesa inputs
    let mut autotune_enabled: bool = false;
    let mut autotune_watch = AUTOTUNE_OVERRIDE.receiver().unwrap();

    //Current sense requires filtering
    let mut i_spike_filter = MedianFilter5::<f32>::new();
    let mut i_lowpass_filter = Pt2Filterf32::new();
    i_lowpass_filter.set_cutoff_frequency(70.0, (I_REF_RATE_US as f32) / 1000000f32);

    //voltage sense requires filtering
    let mut v_spike_filter = MedianFilter5::<f32>::new();
    let mut v_lowpass_filter = Pt2Filterf32::new();
    v_lowpass_filter.set_cutoff_frequency(70.0, (V_REF_RATE_US as f32) / 1000000f32);

    //mesa input requires filtering
    let mut m_spike_filter = MedianFilter5::<f32>::new();
    let mut m_lowpass_filter = Pt2Filterf32::new();
    m_lowpass_filter.set_cutoff_frequency(70.0, (MESA_MON_DEL_MS as f32) / 1000000f32);

    //Check the adc at MESA_MON_DEL_MS frequency and if the voltage has changed past a given
    // threshold then send a updated speed to the REQ signal line
    let mut last_set_pnt = 0;
    loop {
        //poll the tickers to act on the next one
        match select3(I_INTR.wait(), vol_ticker.next(), mesa_ticker.next()).await {
            Either3::First(good_v) => {
                //sending the converted 4095 max to voltage max
                // grab raw data
                let raw_current = (I_CORR_NUM * good_v as f32) / 4095f32 + *I_CORR_OFF.lock().await;

                //remove spikes and outliers
                let despooled = i_spike_filter.update(raw_current);

                //go through lowpass for additional filtering
                let filtered_current = i_lowpass_filter.update(despooled);

                I_WATCH.sender().send(filtered_current);
                // info!(",Current Sense,  {}", filtered_current);
            }
            Either3::Second(_) => {
                match adc_v_in.read(&mut voltage_in).await {
                    Ok(good_v) => {
                        let raw_voltage = ((V_CORR_NUM * good_v as f32) / 4095f32) + V_CORR_OFF;

                        //remove spikes and outliers
                        let despooled = v_spike_filter.update(raw_voltage);

                        //go through lowpass for additional filtering
                        let filtered_voltage = v_lowpass_filter.update(despooled);
                        V_WATCH.sender().send(filtered_voltage as i64);
                        // info!("voltage sense = {}", (&filtered_voltage));
                    }
                    Err(_adc_error) => {
                        info!("Error reading adc pin -- req voltage");
                    }
                }
            }
            Either3::Third(_) => {
                //If you;re in manual mode then skip reading/ sending this adc info
                if !MOTOR_MODE.load(core::sync::atomic::Ordering::Relaxed) {
                    continue;
                }
                //check if autotune has been updated
                if let Some(new_tune) = autotune_watch.try_changed() {
                    autotune_enabled = new_tune;
                }
                //skip RPM_REQ piece if the autotune is running
                if autotune_enabled {
                    continue;
                }
                match adc_v_in.read(&mut spindle_cmd).await {
                    Ok(good_v) => {
                        //remove spikes and outliers
                        let despooled = m_spike_filter.update(good_v.into());

                        //go through lowpass for additional filtering
                        let filtered_voltage = m_lowpass_filter.update(despooled);
                        //sending the converted 4095 max to rpm max
                        if (good_v as i16 - last_set_pnt as i16).abs() > CHANGE_THRESHOLD {
                            last_set_pnt = good_v;
                            RPM_REQ_WATCH.sender().send(
                                ((MAX_SPINDLE_SPEED as u64 * filtered_voltage as u64) / 4095u64)
                                    as u16,
                            );
                        }
                        // info!("raw_req_v: {}, filt v: {}", good_v, filtered_voltage);
                    }
                    Err(_adc_error) => {
                        info!("Error reading adc pin -- req rpm");
                    }
                }
            }
        }
    }
}
