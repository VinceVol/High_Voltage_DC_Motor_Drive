//Interface asyc with the hall sensor (magnet passes in front and pulls GPIO6 low).

use core::error;
use core::fmt;

use embassy_rp::{
    gpio::{AnyPin, Input, Output},
    Peri,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Channel, Sender},
};
use embassy_time::Instant;

pub static SPEED_CHANNEL: Channel<CriticalSectionRawMutex, u16, 100> = Channel::new();

struct HallSense {
    pin: Input<'static>,
    min_max: (u16, u16),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum HallError {
    /// Invalid configuration parameter (e.g., N = 0)
    InvalidConfig,
    /// Too many outlier pulses rejected during median filtering
    ExcessiveNoise,
}

// core::error::Error is stable in core since Rust 1.81
impl core::error::Error for HallError {}

impl fmt::Display for HallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HallError::InvalidConfig => write!(f, "Invalid configuration parameter N"),
            HallError::ExcessiveNoise => write!(f, "Too many outliers rejected, unreliable RPM"),
        }
    }
}

#[embassy_executor::task]
async fn run_hall() {
    todo!();
}

impl HallSense {
    fn init(pin: Input<'static>, min_max: (u16, u16)) -> Self {
        Self { pin, min_max }
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

fn extract_rpm<const N: usize>(time_hall_low: [u64; N]) -> Result<u64, HallError> {
    //Compare the each value to the median, if extraneous (~2x different) then "remove"
    time_hall_low.sort_unstable();
    let mut valid_length = N; //Keep track of how many items get removed
    let median = time_hall_low[N / 2];
    for index in 0..N {
        if time_hall_low[index] < median / 2 || time_hall_low[index] > 2 * median {
            //shift everything left
            for i in index..N - 1 {
                time_hall_low[i] = time_hall_low[i + 1];
                valid_length -= 1;
            }
        }
    }

    let threshold = N / 4; //any number of rejects greater than 25% throws error
    if N - valid_length < threshold {
        return None;
    }

    //take the average time difference in ms from the valid values
    let avg_dif_ms = time_hall_low[0..valid_length].iter().sum::<u64>() / (valid_length as u64);
    let rpm = 60000 / (avg_dif_ms);
    return Some(rpm);
}
