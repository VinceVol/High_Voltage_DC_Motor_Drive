//Interface asyc with the hall sensor (magnet passes in front and pulls GPIO6 low).
// times.rotate_left(mid);1o
use core::future::Future;

use core_logic::hall_math::{extract_rpm, HallError};
use defmt::info;
use embassy_futures::select::Either;
use embassy_rp::{
    gpio::{AnyPin, Input},
    Peri,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Receiver};
use embassy_time::{Duration, Instant, Ticker, Timer};

use crate::{CURR_DRAW_THRESH_FOR_ZERO, HALL_CNT, I_WATCH, RPM_FILT_WATCH, RPM_RAW_WATCH};

struct HallSense {
    pin: Input<'static>,
    speed_list: [u64; HALL_CNT],
}

#[derive(Clone, Copy)]
pub enum HallRpm {
    Good(u16),
    Bad(HallError),
    Stall,
}

const SLOW_RESP: u64 = 6000000; // equates to 10 rpm with 1 magnet
const STOPPED_THRESHOLD_US: u64 = 600000;

#[embassy_executor::task]
pub async fn run_hall(hall_pin: Peri<'static, AnyPin>) {
    let mut fn_timer = Instant::now();
    let mut hall_sensor = HallSense::init(hall_pin);
    loop {
        //send raw result to the raw watch -- mostly autotune functionality
        RPM_RAW_WATCH
            .sender()
            .send((60000000 / hall_sensor.speed_list[HALL_CNT - 1]) as u16);

        //For regular operation filter the data to make sure its clean
        // info!("Data: {}", data);
        RPM_FILT_WATCH
            .sender()
            .send(hall_sensor.populate(&mut fn_timer).await);
    }
}

//dumb name for a fn that returns when the rpm is ~ the same twice in a row
pub async fn double_tap<'a>(
    rpm_watch: &mut Receiver<'a, CriticalSectionRawMutex, u16, 100>,
) -> u16 {
    //going to be checking new vs old rpm to know when the motor has settled
    let mut new_rpm: i32;
    let mut old_rpm: i32 = 0;

    loop {
        new_rpm = rpm_watch.changed().await as i32;

        //Essentially fire when the rpm starts to be constant I prefer checking two in a row over a delay
        if (new_rpm as i32 - old_rpm as i32).abs() < 10 {
            return new_rpm as u16;
        }

        old_rpm = new_rpm;
    }
}

impl HallSense {
    fn init(pin_any: Peri<'static, AnyPin>) -> Self {
        let pin = Input::new(pin_any, embassy_rp::gpio::Pull::Up);
        Self {
            pin,
            speed_list: [SLOW_RESP; HALL_CNT],
        }
    }

    async fn populate(&mut self, timer: &mut Instant) -> HallRpm {
        self.speed_list.rotate_left(1); //make room for the new value
                                        //Watch current draw in case the motor shuts off
                                        // hall sensor stands no chance if the motor isn't spinning
        let mut cur_watch = I_WATCH.receiver().unwrap();
        let mut last_cur = 0f32;
        let mut ticker = Ticker::every(Duration::from_micros(STOPPED_THRESHOLD_US));

        //use debounce to avoid noise on trigger
        const DEBOUNCE_THRESHOLD_US: u64 = 1000;

        // we're also checking to make sure current draw isn't low at each step to try
        // and avoid the scenario where rpm stops returning because motor stopped spinning
        //
        // Looking at current at the same time to avoid some kind of misread as a interpretation
        // of zero rpm
        if let Some(new_cur) = cur_watch.try_get() {
            last_cur = new_cur;
        }
        loop {
            //The hall sensor fires on a falling edge
            // check to make sure the motor isn't stopped at each step
            if check_stop_and(self.pin.wait_for_falling_edge(), &mut ticker).await {
                self.speed_list = [SLOW_RESP; HALL_CNT];
                if last_cur < CURR_DRAW_THRESH_FOR_ZERO {
                    return HallRpm::Good(0);
                }
                return HallRpm::Stall;
            }

            if check_stop_and(self.pin.wait_for_low(), &mut ticker).await {
                self.speed_list = [SLOW_RESP; HALL_CNT];
                if last_cur < CURR_DRAW_THRESH_FOR_ZERO {
                    return HallRpm::Good(0);
                }
                return HallRpm::Stall;
            }

            //If the pin is still low after the DEBOUNCE THRESH then we should be good
            let mut is_it_rlly_low: [bool; 5] = [false; 5];

            for c in 0..5 {
                is_it_rlly_low[c] = if self.pin.is_low() { true } else { false };
                Timer::after_micros(DEBOUNCE_THRESHOLD_US / 5).await;
            }
            if !is_it_rlly_low.contains(&false) {
                self.speed_list[HALL_CNT - 1] = timer.elapsed().as_micros();
                *timer = Instant::now();
                match extract_rpm(self.speed_list) {
                    Ok(rpm) => return HallRpm::Good(rpm),
                    Err(e) => return HallRpm::Bad(e),
                }
            }
        }
    }
}

//should do &self .skip_if_stopped and send the future in
async fn check_stop_and<F: Future>(future: F, ticker: &mut Ticker) -> bool {
    match embassy_futures::select::select(future, ticker.next()).await {
        //If the future returns first then we didn't time out
        Either::First(_) => {
            return false;
        }
        //if the ticker returns first then we times out
        Either::Second(_) => {
            return true;
        }
    }
}
