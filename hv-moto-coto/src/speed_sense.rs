//Interface asyc with the hall sensor (magnet passes in front and pulls GPIO6 low).

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
    channel: Sender<'static, CriticalSectionRawMutex, u16, 100>,
}

#[embassy_executor::task]
async fn run_hall() {
    todo!();
}

impl HallSense {
    fn init(
        pin: Input<'static>,
        min_max: (u16, u16),
        channel: Sender<'static, CriticalSectionRawMutex, u16, 100>,
    ) -> Self {
        Self {
            pin,
            min_max,
            channel,
        }
    }

    async fn read<const N: usize>(&mut self) -> Option<u16> {
        //N representing the amount of readings needed for an average rpm to return
        let mut time_hall_low: [u64; N] = [0; N];
        if N == 0 {
            return None;
        }

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

        None
    }
}
