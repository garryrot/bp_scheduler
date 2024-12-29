use std::{sync::atomic::Ordering, time::Duration};

use buttplug::client::LinearCommand;
use tokio::time::Instant;
use tracing::{debug, error, info};

use crate::dynamic_tracking::{movements::*, util::*, DynamicTracking, TrackingSignal};

impl DynamicTracking {
    /// mirrors the movement range of the last range for an estimated duration
    pub async fn track_mirror(&mut self) {
        let penetrating = |pen_time: &Option<Instant>| match pen_time {
            Some(time) => {
                (Instant::now() - *time) < Duration::from_millis(self.settings.stroke_max_ms.into())
            }
            None => false,
        };

        self.set_var_pen_depth(0.0);
        self.set_var_pen_speed(self.settings.stroke_max_ms);

        if self.settings.move_at_start {
            self.move_devices(
                self.settings.stroke_default_ms,
                self.settings.starting_position,
            )
            .await;
        }

        let mut last_pen = None;
        let mut meas = Movements::new(self.settings.stroke_default_ms, self.settings.stroke_max_ms);

        // this might crash somewhere because there is no guaranetee that Instant
        // can go 20 seconds into the past but I'm just gonna gamble that 
        // the cpu has at least 20s worth of cycles whenever this is called
        let mut last_turn = Instant::now() - Duration::from_secs(20);
        let mut last_pos = 0.0;
        let mut moving_inward = true;

        let mut stop = false;
        while !stop {
            match self.signals.recv().await {
                Some(signal) => match signal {
                    TrackingSignal::Penetration(instant) => last_pen = Some(instant),
                    TrackingSignal::OuterTurn(instant, margins) => {
                        if moving_inward {
                            error!("not moving outward");
                        } else if !self.below_min_resolution(last_turn, instant) {
                            debug!("moving inward");
                            last_turn = instant;
                            moving_inward = true;
                            if penetrating(&last_pen) {
                                meas.measure(instant);

                                let estimated_dur = meas.get_avg_ms();
                                let target_pos: f64 = limit_speed(
                                    last_pos,
                                     margins.most_in,
                                    estimated_dur,
                                    self.settings.min_ms_for_full_stroke,
                                );

                                self.set_var_pen_pos(target_pos);
                                self.set_var_pen_speed(estimated_dur);
                                self.set_var_pen_depth(target_pos - last_pos);
                                self.move_devices(estimated_dur, target_pos).await;
                                last_pos = target_pos;
                            }
                        }
                    }
                    TrackingSignal::InnerTurn(instant, margins) => {
                        if !moving_inward {
                            error!("not moving inward");
                        } else if !self.below_min_resolution(last_turn, instant) {
                            debug!("moving outward");
                            last_turn = instant;
                            moving_inward = false;
                            if penetrating(&last_pen) {
                                meas.measure(instant);

                                let estimated_dur = meas.get_avg_ms();
                                let target_pos = limit_speed(
                                    last_pos,
                                    margins.most_out,
                                    estimated_dur,
                                    self.settings.min_ms_for_full_stroke,
                                );

                                self.set_var_pen_pos(target_pos);
                                self.set_var_pen_depth(target_pos - last_pos);
                                self.set_var_pen_speed(estimated_dur);
                                self.move_devices(estimated_dur, target_pos).await;
                                last_pos = target_pos;
                            }
                        }
                    }
                    TrackingSignal::Stop => {
                        self.set_var_pen_depth(0.0);
                        self.set_var_pen_speed(self.settings.stroke_max_ms);
                        stop = true;
                    } 
                },
                None => {
                    error!("signals stopped");
                    stop = true
                }
            }
        }
    }

    fn set_var_pen_pos(&self, depth: f64) {
        debug!(depth, "setting var current pos");
        self.status.cur_pos.store(f64::abs((1.0 - depth) * 100.0) as i64, Ordering::Relaxed);
    }

    fn set_var_pen_depth(&self, depth: f64) {
        let dept = f64::abs(depth) * 100.0;
        self.status.cur_avg_depth
                    .store(f64::abs(dept) as i64, Ordering::Relaxed);
    }

    fn set_var_pen_speed(&self, estimated_dur: u32) {
        let val = if estimated_dur < self.settings.min_ms_for_full_stroke {
            1.0
        } else {
            let ms_to_min = (estimated_dur - self.settings.min_ms_for_full_stroke) as f32;
            let max_ms = (self.settings.stroke_max_ms - self.settings.min_ms_for_full_stroke) as f32;
            let x = 1.0 - (ms_to_min / max_ms);
            (x * x) * 100.0
        };
        self.status.cur_avg_ms.store(val as i64, Ordering::Relaxed);
    }

    async fn move_devices(&self, estimated_dur: u32, last_pos: f64) {
        for actuator in &self.actuators {
            info!(
                "moving {} to {} over {}ms...",
                actuator.identifier(),
                last_pos,
                estimated_dur
            );
            actuator
                .device
                .linear(&LinearCommand::Linear(estimated_dur, last_pos))
                .await
                .unwrap();
            info!("done!");
        }
    }

    fn below_min_resolution(&self, last_instant: Instant, instant: Instant) -> bool {
        let elapsed = (instant - last_instant).as_millis() as f64;
        if elapsed < self.settings.min_resolution_ms as f64 {
            debug!(
                "skipping {}ms below min resolution {}",
                elapsed, self.settings.min_resolution_ms
            );
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bp_fakes::{get_test_client, linear, ButtplugTestClient};
    use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

    use crate::{actuator::Actuators, dynamic_tracking::*};

    async fn setup() -> (
        ButtplugTestClient,
        UnboundedSender<TrackingSignal>,
        DynamicTracking,
    ) {
        let test_client = get_test_client(vec![linear(1, "lin1")]).await;
        let actuators = test_client.created_devices.flatten_actuators().clone();
        let (sender, receiver) = unbounded_channel::<TrackingSignal>();
        let tracking = DynamicTracking {
            settings: DynamicSettings {
                move_at_start: false,
                min_resolution_ms: 50,
                min_ms_for_full_stroke: 200, // lmits speed
                stroke_default_ms: 400,
                starting_position: 0.0,
                stroke_max_ms: 3_000,
                sampling_rate_ms: 50,
                initial_timeout_ms: 1200
            },
            signals: receiver,
            actuators,
            status: DynamicTrackingHandle::default()
        };
        (test_client, sender, tracking)
    }

    #[tokio::test]
    pub async fn mirror_no_penetration_nothing_happens() {
        let test = TestFixture::new().await;
        test.signal_inner(0, 0.0, 0.0);
        test.signal_outer(200, 0.0, 0.0);
        let results = test.finish().await;
        results.call_registry.assert_unused(1);
    }

    #[tokio::test]
    pub async fn mirror_movement_after_timeout_nothing_happens() {
        let test = TestFixture::new().await;
        test.send(TrackingSignal::Penetration(
            Instant::now() - Duration::from_secs(4),
        ));
        test.signal_inner(0, 0.0, 1.0);
        test.signal_outer(200, 0.0, 0.0);
        test.signal_inner(400, 0.0, 1.0);
        let results = test.finish().await;

        results.call_registry.assert_unused(1);
    }

    #[tokio::test]
    pub async fn mirror_movements_from_last_inward_as_outward() {
        let test = TestFixture::new().await;
        test.signal_penetration();
        test.signal_inner(0, 0.8, 0.0);
        test.signal_outer(200, 0.0, 0.1);
        test.signal_inner(500, 0.9, 0.0);
        let results = test.finish().await;

        let msgs = results.call_registry.get_device(1);
        msgs[0].assert_duration(400).assert_pos(0.8); // uses default ms
        msgs[1].assert_duration(200).assert_pos(0.1); // average ms
        msgs[2].assert_duration(250).assert_pos(0.9); // average ms
    }

    #[tokio::test]
    pub async fn mirror_movements_too_fast_shortened() {
        let test = TestFixture::new().await;
        test.signal_penetration();
        test.signal_inner(100, 1.0, 0.0);
        test.signal_outer(200, 0.0, 0.0);
        test.signal_inner(300, 1.0, 0.0);
        let results = test.finish().await;

        let msgs = results.call_registry.get_device(1);
        msgs[0].assert_duration(400).assert_pos(1.0); // uses default ms
        msgs[1].assert_duration(100).assert_pos(0.5); // average ms
        msgs[2].assert_duration(100).assert_pos(1.0); // average ms
    }

    #[tokio::test]
    pub async fn movements_below_min_resolutions_only_first_one_registered() {
        let test = TestFixture::new().await;
        test.signal_penetration();
        test.signal_inner(10, 1.0, 0.0);
        test.signal_outer(15, 0.0, 0.0);
        test.signal_outer(220, 0.0, 0.0);
        let results = test.finish().await;

        let msgs = results.call_registry.get_device(1);
        msgs[0].assert_duration(400).assert_pos(1.0);
        msgs[1].assert_duration(200).assert_pos(0.0);
        assert_eq!(msgs.len(), 2);
    }

    struct TestFixture {
        instant: Instant,
        sender: UnboundedSender<TrackingSignal>,
        tracking: DynamicTracking,
        client: ButtplugTestClient,
    }

    impl TestFixture {
        pub async fn new() -> Self {
            let (client, sender, tracking) = setup().await;
            Self {
                instant: Instant::now(),
                sender,
                tracking,
                client,
            }
        }

        pub fn signal_penetration(&self) {
            self.send(TrackingSignal::Penetration(Instant::now()));
        }

        pub fn signal_inner(&self, delay_ms: u32, inner: f64, outer: f64) {
            self.send(TrackingSignal::InnerTurn(
                self.instant + Duration::from_millis(delay_ms.into()),
                Margins::new(inner, outer),
            ));
        }

        pub fn signal_outer(&self, delay_ms: u32, inner: f64, outer: f64) {
            self.send(TrackingSignal::OuterTurn(
                self.instant + Duration::from_millis(delay_ms.into()),
                Margins::new(inner, outer),
            ));
        }

        fn send(&self, signal: TrackingSignal) {
            self.sender.send(signal).unwrap()
        }

        async fn finish(mut self) -> ButtplugTestClient {
            let test_client = self.client;
            self.sender.send(TrackingSignal::Stop).unwrap();
            self.tracking.track_mirror().await;
            test_client
        }
    }
}
