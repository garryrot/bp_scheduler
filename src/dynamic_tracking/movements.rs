use std::time::Duration;

use tokio::time::Instant;

pub struct Movements {
    pub points: Vec<Instant>,
    pub default_time_ms: u32,
    pub meas_window_ms: u32,
}

impl Movements {
    pub fn new(default_time_ms: u32, meas_window_ms: u32) -> Self {
        Self {
            points: vec![],
            default_time_ms,
            meas_window_ms,
        }
    }

    pub fn measure_now(&mut self) {
        self.points.push(Instant::now());
    }

    pub fn measure(&mut self, instant: Instant) {
        self.points.push(instant);
    }

    pub fn get_avg_ms(&mut self) -> u32 {
        self.points = self
            .points
            .iter()
            .filter(|t| self.in_timeframe(t))
            .cloned()
            .collect();
        let len = self.points.len();
        if len > 1 {
            let sum_us = self
                .points
                .windows(2)
                .map(|w| (w[1] - w[0]).as_micros())
                .sum::<u128>();
            (sum_us as f64 / (len - 1) as f64 / 1000.0) as u32
        } else {
            self.default_time_ms
        }
    }

    fn in_timeframe(&self, instant: &Instant) -> bool {
        instant > &(Instant::now() - Duration::from_millis(self.meas_window_ms.into()))
    }
}


#[cfg(test)]
mod tests {
    use std::time::Duration;
    use more_asserts::{assert_ge, assert_le};
    use crate::dynamic_tracking::movements::Movements;
        
    #[tokio::test]
    pub async fn measurement_returns_defaul_no_meas() {
        let mut meas = Movements::new(50, 999);
        assert_eq!(meas.get_avg_ms(), 50);
    }

    #[tokio::test]
    pub async fn measurement_returns_default_one_meas() {
        let mut meas = Movements::new(50, 999);
        meas.measure_now();
        assert_eq!(meas.get_avg_ms(), 50);
    }

    async fn measurement_test_avg(ms: u32, i: u64) {
        let mut meas = Movements::new(7878, 999);
        for _ in 0..i {
            meas.measure_now();
            tokio::time::sleep(Duration::from_millis(ms.into())).await;
        }
        meas.measure_now();
        let avg = meas.get_avg_ms();
        assert_ge!(avg, ms - 15);
        assert_le!(avg, ms + 15);
    }

    #[tokio::test]
    pub async fn measure_avg_2() {
        measurement_test_avg(100, 2).await;
    }

    #[tokio::test]
    pub async fn measure_avg_3() {
        measurement_test_avg(100, 3).await;
    }

    #[tokio::test]
    pub async fn measure_avg_5() {
        measurement_test_avg(100, 5).await;
    }

}