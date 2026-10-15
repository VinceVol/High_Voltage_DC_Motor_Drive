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
            Ok(hall_data) => {
                let rpm = extract_rpm(hall_data);
                info!("Rpm: {}", rpm);
            }
            Err(hall_error) => {
                info!("Error: {}", hall_error);
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

        //Create a list of time differences for N number
        let mut time_start = Instant::now();
        let mut time_finish;
        let mut cntr = 0;
        while cntr < N {
            self.pin.wait_for_rising_edge().await;
            time_finish = Instant::now();
            time_hall_low[cntr] = (time_finish - time_start).as_millis();
            time_start = time_finish;
            cntr += 1;
        }

        return Ok(time_hall_low);
    }
}
