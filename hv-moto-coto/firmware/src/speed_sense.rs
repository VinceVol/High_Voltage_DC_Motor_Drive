//Interface asyc with the hall sensor (magnet passes in front and pulls GPIO6 low).

use core_logic::hall_math::{extract_rpm, HallError};
use defmt::info;
use embassy_rp::{
    gpio::{AnyPin, Input},
    Peri,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Instant, Timer};

pub static SPEED_CHANNEL: Channel<CriticalSectionRawMutex, u16, 100> = Channel::new();

struct HallSense {
    pin: Input<'static>,
}

#[embassy_executor::task]
pub async fn run_hall(hall_pin: Peri<'static, AnyPin>) {
    let mut hall_sensor = HallSense::init(hall_pin);
    loop {
        let data: Result<[u64; 15], HallError> = hall_sensor.populate().await;
        info!("the data extracted: \n {}", data);
        match data {
            Ok(hall_data) => match extract_rpm(hall_data) {
                Ok(rpm) => {
                    SPEED_CHANNEL.send(rpm).await;
                    info!("rpm: {}", rpm);
                    Timer::after(Duration::from_millis(50)).await;
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
