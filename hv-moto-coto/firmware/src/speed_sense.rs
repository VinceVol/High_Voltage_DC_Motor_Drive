//Interface asyc with the hall sensor (magnet passes in front and pulls GPIO6 low).

use core_logic::hall_math::{extract_rpm, HallError};
use defmt::info;
use embassy_rp::{
    gpio::{AnyPin, Input},
    Peri,
};
use embassy_time::{Duration, Instant, Timer};

use crate::RPM_WATCH;

struct HallSense {
    pin: Input<'static>,
}

#[embassy_executor::task]
pub async fn run_hall(hall_pin: Peri<'static, AnyPin>) {
    let mut hall_sensor = HallSense::init(hall_pin);
    loop {
        let data: Result<[u64; 15], HallError> = hall_sensor.populate().await;
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
                    info!("RPM Hall Error: {}", hall_error);
                    Timer::after(Duration::from_millis(500)).await;
                }
            },
            Err(hall_error) => {
                info!("Populate Hall List Error: {}", hall_error);
                Timer::after(Duration::from_millis(500)).await;
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

        //use debounce to avoid noise on trigger
        const DEBOUNCE_THRESHOLD_US: u64 = 1000;

        //Create a list of time differences for N number
        let mut time_start = Instant::now();
        let mut time_finish;
        let mut cntr = 0;
        while cntr < N {
            //even with debounce still needed the next two lines to help with low speed noise
            // maybe this noise is mostly gone when machine mounting the sensor rather than hand holding
            self.pin.wait_for_low().await;
            self.pin.wait_for_high().await;
            loop {
                //The hall sensor fires on a falling edge
                self.pin.wait_for_falling_edge().await;
                time_finish = Instant::now();
                let elapsed = (time_finish - time_start).as_micros();
                if elapsed > DEBOUNCE_THRESHOLD_US {
                    time_hall_low[cntr] = elapsed;
                    time_start = time_finish;
                    cntr += 1;
                    break;
                }
            }
        }

        return Ok(time_hall_low);
    }
}
