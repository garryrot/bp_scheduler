use std::{cmp, sync::Arc, time::Duration};

use buttplug::client::{ButtplugClientDevice, LinearCommand};
use tokio::{sync::mpsc::UnboundedReceiver, time::Instant};
use tracing::error;

use crate::{linear::LinearRange, worker::WorkerResult};

pub enum TrackingSignal {
    Penetration(Instant), // starts or refreshes the movement time window for time_window_ms
    OutwardCompleted(Instant, f64 /* in range */, f64 /* out range */), // outward movement finished from pos1 to pos2
    InwardCompleted(Instant, f64 /* in range */, f64 /* out range */), // outward movement finished from pos1 to pos2
    Stop,
}

pub struct DynamicSettings {
    pub boundaries: LinearRange,
    pub max_speed_ms: u32,
    pub default_stroke_ms: u32,
    pub default_stroke_in: f64,
    pub default_stroke_out: f64,
    pub time_window_ms: u32,
}

impl DynamicSettings {
    pub fn default() -> Self {
        DynamicSettings {
            boundaries: LinearRange::max(),
            max_speed_ms: 200,
            default_stroke_ms: 400,
            default_stroke_in: 0.0,
            default_stroke_out: 1.0,
            time_window_ms: 3_000,
        }
    }
}

struct Movements {
    points: Vec<Instant>,
    default_time_ms: u32,
    meas_window_ms: u32,
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

pub enum Direction {
    Inward,
    Outward
}

pub struct DynamicTracking {
    settings: DynamicSettings,
    signals: UnboundedReceiver<TrackingSignal>,
    devices: Vec<Arc<ButtplugClientDevice>>,
}

impl DynamicTracking {

    /// repeats the range / speed of the last inward movement as 
    pub async fn track_mirror(mut self) -> WorkerResult {
        let penetrating = |pen_time: &Option<Instant>| match pen_time {
            Some(time) => {
                (Instant::now() - *time)
                    < Duration::from_millis(self.settings.time_window_ms.into())
            }
            None => false,
        };

        // TODO: Move to start

        let mut last_pen = None;
        let mut movements =
            Movements::new(self.settings.default_stroke_ms, self.settings.time_window_ms);

        let mut last_instant;
        let mut last_pos = 0.0;

        let mut moving_inward = true;

        let mut stop = false;
        while !stop {
            match self.signals.recv().await {
                Some(signal) => match signal {
                    TrackingSignal::Penetration(instant) => last_pen = Some(instant),
                    TrackingSignal::OutwardCompleted(instant, target_pos, _out) => {
                        if moving_inward {
                            error!("not moving outward");
                        } else {
                            last_instant = instant;
                            moving_inward = true;
                            if penetrating(&last_pen) {
                                movements.measure(instant);

                                let estimated_dur = movements.get_avg_ms();
                                let target_pos= get_max_distance(last_pos, target_pos, estimated_dur, self.settings.max_speed_ms);
                                
                                last_pos = target_pos;
                                for device in &self.devices {
                                    device
                                        .linear(&LinearCommand::Linear(estimated_dur, last_pos))
                                        .await
                                        .unwrap();
                                }
                            }
                        }
                    }
                    TrackingSignal::InwardCompleted(instant, _in, target_pos) => {
                        if !moving_inward {
                            error!("not moving inward");
                        } else {
                            last_instant = instant;
                            moving_inward = false;
                            if penetrating(&last_pen) {
                                movements.measure(instant);

                                let estimated_dur = movements.get_avg_ms();
                                last_pos = get_max_distance(last_pos, target_pos, estimated_dur, self.settings.max_speed_ms);

                                for device in &self.devices {
                                    device
                                        .linear(&LinearCommand::Linear(
                                            estimated_dur,
                                            last_pos,
                                        ))
                                        .await
                                        .unwrap();
                                }
                            }
                        }
                    }
                    TrackingSignal::Stop => stop = true,
                },
                None => {
                    error!("signals stopped");
                    stop = true
                },
            }
        }
        Ok(())
    }
}

fn get_max_distance(from_pos: f64, to_pos: f64, estimated_dur: u32, max_speed_ms: u32) -> f64 {
    if estimated_dur < max_speed_ms { // self.settings.max_speed_ms
        let max_dist = estimated_dur as f64 / max_speed_ms as f64;
        let mut dist = to_pos - from_pos;
        if dist < 0.0 && dist < -max_dist {
            dist = -max_dist;
        }
        if dist > 0.0 && dist > max_dist {
            dist = max_dist;
        }
        from_pos + dist
    } else {
        to_pos
    }
}

#[cfg(test)]
mod tests {
    use bp_fakes::{get_test_client, linear, ButtplugTestClient};
    use more_asserts::{assert_ge, assert_le};
    use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

    use crate::dynamic::*;

    #[tokio::test]
    pub async fn max_distance_test() {
        assert_eq!(get_max_distance(0.0, 1.0, 200, 200), 1.0, "returns actual target if speed in range");
        assert_eq!(get_max_distance(1.0, 0.0, 200, 200), 0.0, "works in reverse");
        assert_eq!(get_max_distance(0.0, 1.0, 100, 200), 0.5, "moves 50% of the range if the speed is 2x to fast");
        assert_eq!(get_max_distance(0.0, 1.0, 50, 200), 0.25, "moves 25% of the range if the speed is 4x too fast ");
        assert_eq!(get_max_distance(0.75, 0.0, 100, 200), 0.25, "moves 75% of the range of the speed 25% to ");
    }


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

    async fn setup() -> (
        ButtplugTestClient,
        UnboundedSender<TrackingSignal>,
        DynamicTracking,
    ) {
        let test_client = get_test_client(vec![linear(1, "lin1")]).await;
        let devices = test_client.created_devices.clone();
        let (sender, receiver) = unbounded_channel::<TrackingSignal>();
        let tracking = DynamicTracking {
            settings: DynamicSettings::default(),
            signals: receiver,
            devices,
        };
        (test_client, sender, tracking)
    }

    #[tokio::test]
    pub async fn mirror_no_penetration_nothing_happens() {
        let (test_client, sender, tracking) = setup().await;

        sender.send(TrackingSignal::InwardCompleted(Instant::now(), 0.0, 0.0)).unwrap();
        sender.send(TrackingSignal::OutwardCompleted(Instant::now(), 0.0, 0.0)).unwrap();
        sender.send(TrackingSignal::Stop).unwrap();
        tracking.track_mirror().await.unwrap();

        test_client.call_registry.assert_unused(1);
    }

    #[tokio::test]
    pub async fn mirror_movement_after_timeout_nothing_happens() {
        let (test_client, sender, tracking) = setup().await;

        sender
            .send(TrackingSignal::Penetration(Instant::now() - Duration::from_secs(4)))
            .unwrap();
        sender.send(TrackingSignal::InwardCompleted(Instant::now(), 0.0, 0.0)).unwrap();
        sender.send(TrackingSignal::OutwardCompleted(Instant::now(), 0.0, 0.0)).unwrap();
        sender.send(TrackingSignal::Stop).unwrap();
        tracking.track_mirror().await.unwrap();

        test_client.call_registry.assert_unused(1);
    }

    #[tokio::test]
    pub async fn mirror_movements_from_last_inward_as_outward() {
        let (test_client, sender, tracking) = setup().await;

        sender
            .send(TrackingSignal::Penetration(Instant::now()))
            .unwrap();
        sender
            .send(TrackingSignal::InwardCompleted(Instant::now(), 0.0, 0.8))
            .unwrap();
        sender
            .send(TrackingSignal::OutwardCompleted(Instant::now() + Duration::from_millis(200), 0.1, 0.0))
            .unwrap();
        sender
            .send(TrackingSignal::InwardCompleted(Instant::now() + Duration::from_millis(500), 0.0, 0.9))
            .unwrap();
        sender.send(TrackingSignal::Stop).unwrap();
        tracking.track_mirror().await.unwrap();

        let msgs = test_client.call_registry.get_device(1);
        msgs[0].assert_duration(400); // uses default ms
        msgs[0].assert_pos(0.8);
        msgs[1].assert_duration(200); // average ms
        msgs[1].assert_pos(0.1);
        msgs[2].assert_duration(250); // average ms
        msgs[2].assert_pos(0.9);

        // TODO: This doesn't mirror, rather delay 
    }

    
    #[tokio::test]
    pub async fn mirror_movements_too_fast_shortened() {
        let (test_client, sender, tracking) = setup().await;

        sender
            .send(TrackingSignal::Penetration(Instant::now()))
            .unwrap();
        sender
            .send(TrackingSignal::InwardCompleted(Instant::now(), 0.0, 1.0))
            .unwrap();
        sender
            .send(TrackingSignal::OutwardCompleted(Instant::now() + Duration::from_millis(100), 0.0, 0.0))
            .unwrap();
        sender
            .send(TrackingSignal::InwardCompleted(Instant::now() + Duration::from_millis(200), 0.0, 1.0))
            .unwrap();
        sender.send(TrackingSignal::Stop).unwrap();
        tracking.track_mirror().await.unwrap();

        let msgs = test_client.call_registry.get_device(1);
        msgs[0].assert_duration(400); // uses default ms
        msgs[0].assert_pos(1.0);
        msgs[1].assert_duration(100); // average ms
        msgs[1].assert_pos(0.5);
        msgs[2].assert_duration(100); // average ms
        msgs[2].assert_pos(1.0);

        // TODO: This doesn't mirror, rather delay 
    }
}
