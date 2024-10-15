use std::time::Duration;

use buttplug::client::LinearCommand;
use tokio::time::Instant;
use tracing::{debug, error, info};

use crate::worker::WorkerResult;
use crate::dynamic_tracking::{movements::*, util::*, DynamicTracking, TrackingSignal};

impl DynamicTracking {
    /// mirrors the movement range of the last range for an estimated duration
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

        let mut last_turn  = Instant::now() - Duration::from_secs(99999);
        let mut last_pos = 0.0;
        let mut moving_inward = true;

        let mut stop = false;
        while !stop {
            match self.signals.recv().await {
                Some(signal) => match signal {
                    TrackingSignal::Penetration(instant) => last_pen = Some(instant),
                    TrackingSignal::OutwardCompleted(instant, max_inward, _max_outward) => {
                        if moving_inward {
                            error!("not moving outward");
                        } else if ! self.below_min_resolution(last_turn, instant) {
                            last_turn = instant;
                            moving_inward = true;
                            if penetrating(&last_pen) {
                                movements.measure(instant);

                                let estimated_dur = movements.get_avg_ms();
                                let target_pos= limit_speed(last_pos, max_inward, estimated_dur, self.settings.min_duration_ms);
                                
                                info!("OutwardCompleted moving from {} to {} over {}ms", last_pos, target_pos, estimated_dur);
                                last_pos = target_pos;
                                for device in &self.devices {
                                    info!("moving {}...", device.name());
                                    device
                                        .linear(&LinearCommand::Linear(estimated_dur, last_pos))
                                        .await
                                        .unwrap();
                                    info!("done!");
                                }
                            }
                        }
                    }
                    TrackingSignal::InwardCompleted(instant, _max_inward, max_outward) => {
                        if !moving_inward {
                            error!("not moving inward");
                        } else if !self.below_min_resolution(last_turn, instant) {
                            last_turn = instant;
                            moving_inward = false;
                            if penetrating(&last_pen) {
                                movements.measure(instant);

                                let estimated_dur = movements.get_avg_ms();
                                let target_pos = limit_speed(last_pos, max_outward, estimated_dur, self.settings.min_duration_ms);
                                info!("InwardCompleted moving from {} to {} over {}ms", last_pos, target_pos, estimated_dur);
                                last_pos = target_pos;
                                for device in &self.devices {
                                    info!("moving {}...", device.name());
                                    device
                                        .linear(&LinearCommand::Linear(
                                            estimated_dur,
                                            last_pos,
                                        ))
                                        .await
                                        .unwrap();
                                    info!("done!");
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

    fn below_min_resolution(&self, last_instant: Instant, instant: Instant) -> bool {
        let elapsed = (instant - last_instant).as_millis() as f64;
        if elapsed < self.settings.min_resolution_ms as f64 {
            println!("skipping {}ms below min resolution {}", elapsed, self.settings.min_resolution_ms);
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

    use crate::dynamic_tracking::*;

    async fn setup() -> (
        ButtplugTestClient,
        UnboundedSender<TrackingSignal>,
        DynamicTracking,
    ) {
        let test_client = get_test_client(vec![linear(1, "lin1")]).await;
        let devices = test_client.created_devices.clone();
        let (sender, receiver) = unbounded_channel::<TrackingSignal>();
        let tracking = DynamicTracking {
            settings: DynamicSettings {
                boundaries: LinearRange::max(),
                min_resolution_ms: 50,
                min_duration_ms: 200,
                default_stroke_ms: 400,
                default_stroke_in: 0.0,
                default_stroke_out: 1.0,
                time_window_ms: 3_000,
            },
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
    }

    
    #[tokio::test]
    pub async fn mirror_movements_too_fast_shortened() {
        let (test_client, sender, tracking) = setup().await;

        sender
            .send(TrackingSignal::Penetration(Instant::now()))
            .unwrap();
        sender
            .send(TrackingSignal::InwardCompleted(Instant::now() + Duration::from_millis(100), 0.0, 1.0))
            .unwrap();
        sender
            .send(TrackingSignal::OutwardCompleted(Instant::now() + Duration::from_millis(200), 0.0, 0.0))
            .unwrap();
        sender
            .send(TrackingSignal::InwardCompleted(Instant::now() + Duration::from_millis(300), 0.0, 1.0))
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
    }

    #[tokio::test]
    pub async fn movements_below_min_resolutions_only_first_one_registered() {
        let (test_client, sender, tracking) = setup().await;

        let instant = Instant::now();
        sender.send(TrackingSignal::Penetration(Instant::now())).unwrap();
      
        sender.send(TrackingSignal::InwardCompleted(instant + Duration::from_millis(10), 0.0, 1.0)).unwrap();
        sender.send(TrackingSignal::OutwardCompleted(instant + Duration::from_millis(15), 0.0, 0.0)).unwrap();

        sender.send(TrackingSignal::OutwardCompleted(instant + Duration::from_millis(220), 0.0, 0.0)).unwrap();

        sender.send(TrackingSignal::Stop).unwrap();
        tracking.track_mirror().await.unwrap();

        let msgs = test_client.call_registry.get_device(1);
        msgs[0].assert_duration(400).assert_pos(1.0);
        msgs[1].assert_duration(200).assert_pos(0.0);
        assert_eq!(msgs.len(), 2);
    }

}
