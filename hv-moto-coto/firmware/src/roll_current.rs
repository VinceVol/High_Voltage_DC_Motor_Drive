use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Receiver};
use embassy_time::Instant;
use libm::{powf, sqrtf};

pub struct RollingCurrent<const N: usize> {
    buffer: [f32; N],
}

impl<const N: usize> defmt::Format for RollingCurrent<N> {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "RollingCurrent {{ mean: {}, std_dev: {}, buffer: {} }}",
            self.mean(),
            self.std_dev(),
            self.buffer,
        );
    }
}

impl<'a, const N: usize> RollingCurrent<N> {
    pub async fn fill(i_watch: &mut Receiver<'a, CriticalSectionRawMutex, f32, 5>) -> Self {
        let mut buffer: [f32; N] = [0.0; N];
        let mut ii = 0;
        loop {
            //Fully populate the rolling current array
            buffer.rotate_left(1);
            buffer[N - 1] = i_watch.changed().await;
            if ii >= N - 1 {
                return Self { buffer };
            }
            ii += 1;
        }
    }
    pub async fn update(&mut self, i_watch: &mut Receiver<'a, CriticalSectionRawMutex, f32, 5>) {
        self.buffer.rotate_left(1);
        self.buffer[self.buffer.len() - 1] = i_watch.changed().await;
    }
    pub fn mean(&self) -> f32 {
        self.buffer.iter().sum::<f32>() / self.buffer.len() as f32
    }

    fn std_dev(&self) -> f32 {
        let xi_u = self
            .buffer
            .iter()
            .map(|xi| powf(*xi - self.mean(), 2.0))
            .sum::<f32>();

        sqrtf(xi_u / (self.buffer.len() as f32))
    }

    pub async fn wait_till_steady(
        &mut self,
        threshold: f32,
        i_watch: &mut Receiver<'a, CriticalSectionRawMutex, f32, 5>,
    ) {
        loop {
            self.update(i_watch).await;
            //if the standard deviation is less than steady state current const
            if self.std_dev() <= threshold {
                return;
            }
        }
    }

    pub async fn wait_ms(
        &mut self,
        milli: u64,
        i_watch: &mut Receiver<'a, CriticalSectionRawMutex, f32, 5>,
    ) {
        let timer = Instant::now();
        while timer.elapsed().as_millis() < milli {
            self.update(i_watch).await;
            //if the standard deviation is less than steady state current const
        }
    }
    //Need to make sure the current reading isn't the peak inrush
    pub fn slope(&self) -> f32 {
        let a = (0..self.buffer.len()).map(|x| x as f32);
        let b = self.buffer.map(|y| y - self.mean());

        let c: f32 = a.clone().zip(b).map(|(a, b)| a * b).sum();
        let d: f32 = a.map(|x| powf(x, 2.0)).sum();
        c / d
    }

    pub fn crossed(&self, y_target: f32) -> bool {
        let y1 = self.buffer[self.buffer.len() - 1];
        let y2 = self.buffer[self.buffer.len() - 2];
        (y_target - y1) * (y_target - y2) <= 0.0
    }
    pub fn last_two(&self) -> [f32; 2] {
        [
            self.buffer[self.buffer.len() - 2],
            self.buffer[self.buffer.len() - 1],
        ]
    }
}
