//Interface asyc with the hall sensor (magnet passes in front and pulls GPIO6 low).

use core::future::Future;

use core_logic::hall_math::{extract_rpm, HallError};
use defmt::info;
use embassy_futures::select::Either;
use embassy_rp::{
    gpio::{AnyPin, Input},
    Peri,
};
use embassy_time::{Duration, Instant, Ticker, Timer};

use crate::{CURR_DRAW_THRESH_FOR_ZERO, HALL_CNT, I_WATCH, RPM_WATCH, SPEED_FLOOR_RPM};

struct HallSense {
    pin: Input<'static>,
}

const STOPPED_THRESHOLD_US: u64 = 60000000 / SPEED_FLOOR_RPM;

#[embassy_executor::task]
pub async fn run_hall(hall_pin: Peri<'static, AnyPin>) {
    let mut hall_sensor = HallSense::init(hall_pin);
    loop {
        let data: Result<[u64; HALL_CNT], HallError> = hall_sensor.populate().await;
        // info!("Data: {}", data);
        match data {
            Ok(hall_data) => match extract_rpm(hall_data) {
                Ok(rpm) => {
                    RPM_WATCH.sender().send(rpm);
                    info!("rpm: {}", rpm);
                    //I don't think we want to use a wait fn because the pid loop requires
                    // some fast refresh rates
                    // Timer::after(Duration::from_millis(50)).await;
                }
                Err(hall_error) => {
                    info!("RPM Hall Error: {}, from data: {}", hall_error, data);
                    Timer::after(Duration::from_millis(50)).await;
                }
            },
            Err(hall_error) => {
                info!("Populate Hall List Error: {}", hall_error);
                Timer::after(Duration::from_millis(50)).await;
            }
        }
    }
}

impl HallSense {
    fn init(pin_any: Peri<'static, AnyPin>) -> Self {
        let pin = Input::new(pin_any, embassy_rp::gpio::Pull::Up);
        Self { pin }
    }

    async fn populate<const N: usize>(&mut self) -> Result<[u64; N], HallError> {
        //N representing the amount of readings needed for an average rpm to return
        let mut time_hall_low: [u64; N] = [0; N];
        if N == 0 {
            return Err(HallError::InvalidConfig);
        }

        //Watch current draw in case the motor shuts off
        // hall sensor stands no chance if the motor isn't spinning
        let mut cur_watch = I_WATCH.receiver().unwrap();
        let mut last_cur = 0f32;
        let mut ticker = Ticker::every(Duration::from_micros(STOPPED_THRESHOLD_US));
        let slow_response: [u64; N] = [6000000u64; N];

        //use debounce to avoid noise on trigger
        const DEBOUNCE_THRESHOLD_US: u64 = 1000;

        //Create a list of time differences for N number
        let mut timer = Instant::now();
        let mut cntr = 0;
        while cntr < N {
            //even with debounce still needed the next two lines to help with low speed noise
            // maybe this noise is mostly gone when machine mounting the sensor rather than hand holding
            //
            // we're also checking to make sure current draw isn't low at each step to try
            // and avoid the scenario where rpm stops returning because motor stopped spinning
            //
            // Looking at current at the same time to avoid some kind of misread as a interpretation
            // of zero rpm
            if let Some(new_cur) = cur_watch.try_get() {
                last_cur = new_cur;
            }
            if check_stop_and(self.pin.wait_for_high(), &mut ticker, last_cur).await {
                return Ok(slow_response);
            }
            loop {
                info!("A");
                loop {
                    info!("B");
                    //The hall sensor fires on a falling edge
                    // check to make sure the motor isn't stopped at each step
                    if check_stop_and(self.pin.wait_for_falling_edge(), &mut ticker, last_cur).await
                    {
                        return Ok(slow_response);
                    }
                    if check_stop_and(self.pin.wait_for_low(), &mut ticker, last_cur).await {
                        return Ok(slow_response);
                    }

                    //Force a short wait and check the pin is still low
                    // This helps a ton with low speed noise
                    if check_stop_and(
                        Timer::after_micros(DEBOUNCE_THRESHOLD_US),
                        &mut ticker,
                        last_cur,
                    )
                    .await
                    {
                        return Ok(slow_response);
                    }

                    info!("C");
                    //If the pin is still low after the DEBOUNCE THRESH then we should be good
                    if self.pin.is_low() {
                        break;
                    }
                }
                info!("D");
                time_hall_low[cntr] = timer.elapsed().as_micros();
                timer = Instant::now();
                cntr += 1;
                break;
            }
        }

        return Ok(time_hall_low);
    }
}

//should do &self .skip_if_stopped and send the future in
async fn check_stop_and<F: Future>(
    future: F,
    ticker: &mut Ticker,
    last_current_msrmnt: f32,
) -> bool {
    match embassy_futures::select::select(future, ticker.next()).await {
        //If the future returns first then we didn't time out
        Either::First(_) => {
            return false;
        }
        //if the ticker returns first then we times out
        Either::Second(_) => {
            if last_current_msrmnt > CURR_DRAW_THRESH_FOR_ZERO {
                return false;
            } else {
                return true;
            }
        }
    }
}
