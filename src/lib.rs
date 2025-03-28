use std::{sync::Arc, time::Duration, collections::HashMap};

use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedSender},
    time::sleep,
};
use tracing::{debug, error};

use tokio_util::sync::CancellationToken;

pub mod actuator;
pub mod client;
pub mod config; 
pub mod dynamic_tracking;
pub mod player;
pub mod pattern;
pub mod speed;
pub mod filter;
mod util;

use config::*;
use speed::Speed;
use actuator::Actuator;

use player::worker::{ButtplugWorker, WorkerResult, WorkerTask};
use player::PatternPlayer;

#[derive(Debug)]
pub struct ButtplugScheduler {
    worker_task_sender: UnboundedSender<WorkerTask>,
    settings: PlayerSettings,
    control_handles: HashMap<i32, Vec<ControlHandle>>,
    last_handle: i32,
}

#[derive(Debug)]
struct ControlHandle {
    cancellation_token: CancellationToken,
    update_sender: UnboundedSender<Speed>,
}

#[derive(Debug)]
pub struct PlayerSettings {
    pub scalar_resolution_ms: i32,
}

impl ButtplugScheduler {
    pub fn create(settings: PlayerSettings) -> (ButtplugScheduler, ButtplugWorker) {
        let (worker_task_sender, task_receiver) = unbounded_channel::<WorkerTask>();
        (
            ButtplugScheduler {
                worker_task_sender,
                settings,
                control_handles: HashMap::new(),
                last_handle: 0,
            },
            ButtplugWorker { task_receiver },
        )
    }

    pub fn create_player(&mut self, actuators: Vec<Arc<Actuator>>, existing_handle: i32) -> PatternPlayer {
        let (update_sender, update_receiver) = unbounded_channel::<Speed>();
        let cancellation_token = CancellationToken::new();
        let mut handle = existing_handle;

        if existing_handle > 0 {
            if let Some(ref mut control_handles) = self.control_handles.get_mut(&existing_handle) {
                control_handles.push(ControlHandle {
                    cancellation_token: cancellation_token.clone(),
                    update_sender,
                })
            }
        } else {
            handle = self.get_next_handle();
            self.control_handles.insert(
                handle,
                vec![ControlHandle {
                    cancellation_token: cancellation_token.clone(),
                    update_sender,
                }],
            );
        }
        let (result_sender, result_receiver) =
            unbounded_channel::<WorkerResult>();
        PatternPlayer::new(
            handle,
            actuators,
            result_sender,
            result_receiver,
            update_receiver,
            cancellation_token,
            self.worker_task_sender.clone(),
            self.settings.scalar_resolution_ms,
        )
    }

    pub fn update_task(&mut self, handle: i32, speed: Speed) -> bool {
        if self.control_handles.contains_key(&handle) {
            debug!(handle, "updating handle");
            let handles = self
                .control_handles
                .get(&handle)
                .unwrap();
            for handle in handles {
                let _ = handle.update_sender.send(speed);
            }
            true
        } else {
            error!(handle, "unkown handle");
            false
        }
    }

    pub fn stop_task(&mut self, handle: i32) {
        if self.control_handles.contains_key(&handle) {
            let handles = self.control_handles
                .remove(&handle)
                .unwrap();
            debug!(handle, ?handles, "stop handle");

            for handle in handles {
                handle.cancellation_token.cancel();
            }
        } else {
            error!(handle, "Unknown handle");
        } 
    }

    pub fn stop_all(&mut self) {
        let queue_full_err = "Event sender full";
        self.worker_task_sender
            .send(WorkerTask::StopAll)
            .unwrap_or_else(|_| error!(queue_full_err));
        for entry in self.control_handles.drain() {
            debug!("stop-all - stopping handle {:?}", entry.0);
            for handle in entry.1 {
                handle.cancellation_token.cancel()
            }
        }
        self.control_handles.clear();
    }

    pub fn clean_finished_tasks(&mut self) {
        self.control_handles
            .retain(|_, handles| {
                ! handles.first().and_then(|x| Some(x.cancellation_token.is_cancelled()) ).unwrap_or(false)
            }  )
    }

    fn get_next_handle(&mut self) -> i32 {
        self.last_handle += 1;
        self.last_handle
    }

}


async fn cancellable_wait(duration: Duration, cancel: &CancellationToken) -> bool {
    tokio::select! {
        _ = cancel.cancelled() => {
            false
        }
        _ = sleep(duration) => {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    use actuators::linear::{LinearRange, LinearSpeedScaling};
    use actuators::{ActuatorConfig, ActuatorLimits, ActuatorSettings};
    use funscript::{FSPoint, FScript};
    use futures::future::join_all;

    use buttplug::client::ButtplugClientDevice;
    use buttplug::core::message::ActuatorType;

    use tokio::runtime::Handle;
    use tokio::task::JoinHandle;
    use tokio::time::timeout;

    use crate::actuator::{ActuatorConfigLoader, Actuators};
    use crate::player::PatternPlayer;
    use crate::config::*;
    use crate::speed::Speed;
    
    use bp_fakes::*;

    use super::{Actuator, ButtplugScheduler, PlayerSettings};

    struct PlayerTest {
        pub scheduler: ButtplugScheduler,
        pub handles: Vec<JoinHandle<()>>,
        pub actuators: Vec<Arc<Actuator>>,
    }

    impl PlayerTest {
        fn setup_no_settings(devices: &Vec<Arc<ButtplugClientDevice>>) -> Self {
            PlayerTest::setup_with_settings(
                devices.flatten_actuators().clone(),
                PlayerSettings {
                    scalar_resolution_ms: 1,
                },
            )
        }

        fn setup(actuators: Vec<Arc<Actuator>>) -> Self {
            PlayerTest::setup_with_settings(
                actuators,
                PlayerSettings {
                    scalar_resolution_ms: 1,
                },
            )
        }

        fn setup_with_settings(
            actuators: Vec<Arc<Actuator>>,
            settings: PlayerSettings,
        ) -> Self {
            let (scheduler, mut worker) = ButtplugScheduler::create(settings);
            Handle::current().spawn(async move {
                worker.run_worker_thread().await;
            });
            PlayerTest {
                scheduler,
                handles: vec![],
                actuators,
            }
        }

        async fn play_scalar_pattern(
            &mut self,
            duration: Duration,
            fscript: FScript,
            speed: Speed
        ) {
            let player: super::PatternPlayer = self.scheduler.create_player(self.actuators.clone(), -1);
            player
                .play_scalar_pattern(duration, fscript, speed)
                .await
                .unwrap();
        }

        fn play_scalar(
            &mut self,
            duration: Duration,
            speed: Speed
        ) {
            let player = self.scheduler.create_player(self.actuators.clone(), -1);
            self.handles.push(Handle::current().spawn(async move {
                let _ = player.play_scalar(duration, speed).await;
            }));
        }

        fn get_player(&mut self) -> PatternPlayer {
            self.scheduler
                .create_player(self.actuators.clone(), -1 )
        }

        fn get_player_with_settings(&mut self, handle: i32) -> PatternPlayer {
            self.scheduler.create_player(self.actuators.clone(), handle)
        }

        async fn play_linear(&mut self, funscript: FScript, duration: Duration) {
            let player = self
                .scheduler
                .create_player(self.actuators.clone(), -1);
            player
                .play_linear(duration, funscript)
                .await
                .unwrap();
        }

        async fn await_last(&mut self) {
            let _ = self.handles.pop().unwrap().await;
        }

        async fn await_all(self) {
            join_all(self.handles).await;
        }
    }

    /// Linear
    #[tokio::test]
    async fn test_no_devices_does_not_block() {
        // arrange
        let client = get_test_client(vec![]).await;
        let mut player = PlayerTest::setup(client.created_devices.flatten_actuators().clone());

        let mut fs: FScript = FScript::default();
        fs.actions.push(FSPoint { pos: 1, at: 10 });
        fs.actions.push(FSPoint { pos: 2, at: 20 });

        // act & assert
        player.play_scalar(Duration::from_millis(50), Speed::max());
        assert!(
            timeout(Duration::from_secs(1), player.await_last(),)
                .await
                .is_ok(),
            "Scalar finishes within timeout"
        );
        assert!(
            timeout(
                Duration::from_secs(1),
                player.play_linear(fs, Duration::from_millis(50)),
            )
            .await
            .is_ok(),
            "Linear finishes within timeout"
        );
    }

    #[tokio::test]
    async fn test_stroke_linear_1() {
        let (client, _) = test_stroke(
            Speed::new(100),
            LinearRange{ min_pos: 0.0, max_pos: 1.0, min_ms: 50, max_ms: 400, invert: false, scaling: LinearSpeedScaling::Linear },
        )
        .await;

        let calls = client.get_device_calls(1);
        calls[0].assert_duration(50);
        calls[1].assert_duration(50);
        calls[2].assert_duration(50);
    }

    #[tokio::test]
    async fn test_stroke_linear_2() {
        let (client, _) = test_stroke(
            Speed::new(0),
            LinearRange{ min_pos: 1.0, max_pos: 0.0, min_ms: 10, max_ms: 100, invert: false, scaling: LinearSpeedScaling::Linear }
        )
        .await;

        let calls = client.get_device_calls(1);
        calls[0].assert_duration(100).assert_pos(0.0);
        calls[1].assert_duration(100).assert_pos(1.0);
        calls[2].assert_duration(100).assert_pos(0.0);
    }

    #[tokio::test]
    async fn test_stroke_linear_3() {
        let (client, _) = test_stroke(
            Speed::new(75),
            LinearRange{ min_pos: 0.2, max_pos: 0.7, min_ms: 100, max_ms: 200, invert: false, scaling: LinearSpeedScaling::Linear }
        )
        .await;

        let calls = client.get_device_calls(1);
        calls[0].assert_duration(125).assert_pos(0.7);
        calls[1].assert_duration(125).assert_pos(0.2);
        calls[2].assert_duration(125).assert_pos(0.7);
    }

    #[tokio::test]
    async fn test_stroke_linear_invert() {
        let (client, _) = test_stroke(
            Speed::new(100),
            LinearRange{ min_pos: 0.2, max_pos: 0.7, min_ms: 50, max_ms: 50, invert: true, scaling: LinearSpeedScaling::Linear }
        )
        .await;

        let calls = client.get_device_calls(1);
        calls[0].assert_pos(0.3);
        calls[1].assert_pos(0.8);
        calls[2].assert_pos(0.3);
    }

    #[tokio::test]
    async fn test_stroke_update() {
        let client: ButtplugTestClient = get_test_client(vec![linear(1, "lin1")]).await;
        let mut test = PlayerTest::setup(client.created_devices.flatten_actuators().clone());

        // act
        let start = Instant::now();
        let player = test.get_player();
        let join = Handle::current().spawn(async move {
            let _ = player
                .play_linear_stroke(
                    Duration::from_millis(250), 
                    Speed::new(100), 
                    LinearRange {
                        min_pos: 0.0, 
                        max_pos: 1.0, 
                        min_ms: 10, 
                        max_ms: 100, 
                        invert: true, 
                        scaling: LinearSpeedScaling::Linear
                    })
                .await;
        });

        test.scheduler.update_task(1, Speed::new(0));
        let _ = join.await;

        client.print_device_calls(start);
        let calls = client.get_device_calls(1);
        calls[0].assert_duration(100);
        calls[1].assert_duration(100);
        calls[2].assert_duration(100);
    }

    async fn test_stroke(speed: Speed, range: LinearRange) -> (ButtplugTestClient, Instant) {
        let client = get_test_client(vec![linear(1, "lin1")]).await;

        let mut config = ActuatorSettings::default();
        config.update_device(ActuatorConfig { actuator_config_id: "lin1 (Position)".into(), enabled: true, body_parts: vec![], limits: ActuatorLimits::Linear(range.clone()) } );

        let actuators = client.created_devices.flatten_actuators().load_config(&mut config).clone();
        let mut test = PlayerTest::setup(actuators);

        // act
        let start = Instant::now();
        let duration_ms = range.max_ms as f64 * 2.5;
        let player = test.get_player_with_settings(-1);
        let _ = player
            .play_linear_stroke(Duration::from_millis(duration_ms as u64), speed, LinearRange::max())
            .await;

        client.print_device_calls(start);
        (client, start)
    }

    #[tokio::test]
    async fn test_linear_empty_pattern_finishes_and_does_not_panic() {
        let client = get_test_client(vec![linear(1, "lin1")]).await;
        let mut player = PlayerTest::setup(client.created_devices.flatten_actuators().clone());

        // act & assert
        player
            .play_linear(FScript::default(), Duration::from_millis(1))
            .await;

        let mut fs = FScript::default();
        fs.actions.push(FSPoint { pos: 0, at: 0 });
        fs.actions.push(FSPoint { pos: 0, at: 0 });
        player
            .play_linear(FScript::default(), Duration::from_millis(1))
            .await;
    }

    #[tokio::test]
    async fn test_linear_funscript() {
        // arrange
        let client = get_test_client(vec![linear(1, "lin1")]).await;
        let mut player = PlayerTest::setup(client.created_devices.flatten_actuators().clone());

        let mut fscript = FScript::default();
        fscript.actions.push(FSPoint { pos: 50, at: 0 }); // zero_action_is_ignored
        fscript.actions.push(FSPoint { pos: 0, at: 200 });
        fscript.actions.push(FSPoint { pos: 100, at: 400 });

        // act
        let start = Instant::now();
        let duration = get_duration_ms(&fscript);
        player.play_linear(fscript, duration).await;

        // assert
        client.print_device_calls(start);
        client.get_device_calls(1)[0]
            .assert_pos(0.0)
            .assert_duration(200)
            .assert_time(0, start);
        client.get_device_calls(1)[1]
            .assert_pos(1.0)
            .assert_duration(200)
            .assert_time(200, start);
    }

    #[tokio::test]
    async fn test_linear_timing_remains_synced_with_clock() {
        // arrange
        let n = 40;
        let client = get_test_client(vec![linear(1, "lin1")]).await;
        let mut player = PlayerTest::setup(client.created_devices.flatten_actuators().clone());
        let fscript = get_repeated_pattern(n);

        // act
        let start = Instant::now();
        player
            .play_linear(
                get_repeated_pattern(n),
                get_duration_ms(&fscript)
            )
            .await;

        // assert
        client.print_device_calls(start);
        check_timing(client.get_device_calls(1), n, start);
    }

    #[tokio::test]
    async fn test_linear_repeats_until_duration_ends() {
        // arrange
        let client = get_test_client(vec![linear(1, "lin1")]).await;
        let mut player = PlayerTest::setup(client.created_devices.flatten_actuators().clone());

        let mut fscript = FScript::default();
        fscript.actions.push(FSPoint { pos: 100, at: 200 });
        fscript.actions.push(FSPoint { pos: 0, at: 400 });

        // act
        let start = Instant::now();
        let duration = Duration::from_millis(800);
        player.play_linear(fscript, duration).await;

        // assert
        client.print_device_calls(start);

        let calls = client.get_device_calls(1);
        calls[0].assert_pos(1.0).assert_time(0, start);
        calls[1].assert_pos(0.0).assert_time(200, start);
        calls[2].assert_pos(1.0).assert_time(400, start);
        calls[3].assert_pos(0.0).assert_time(600, start);
    }

    #[tokio::test]
    async fn test_linear_cancels_after_duration() {
        // arrange
        let client = get_test_client(vec![linear(1, "lin1")]).await;
        let mut player = PlayerTest::setup(client.created_devices.flatten_actuators().clone());

        let mut fscript = FScript::default();
        fscript.actions.push(FSPoint { pos: 0, at: 400 });
        fscript.actions.push(FSPoint { pos: 0, at: 800 });

        // act
        let start = Instant::now();
        let duration = Duration::from_millis(400);
        player.play_linear(fscript, duration).await;

        // assert
        client.print_device_calls(start);
        client.get_device_calls(1)[0]
            .assert_pos(0.0)
            .assert_duration(400);
        assert!(
            start.elapsed().as_millis() < 425,
            "Stops after duration ends"
        );
    }

    /// Scalar
    #[tokio::test]
    async fn test_scalar_empty_pattern_finishes_and_does_not_panic() {
        // arrange
        let client = get_test_client(vec![scalar(1, "vib1", ActuatorType::Vibrate)]).await;
        let mut player = PlayerTest::setup(client.created_devices.flatten_actuators().clone());

        // act & assert
        let duration = Duration::from_millis(1);
        let fscript = FScript::default();
        player
            .play_scalar_pattern(duration, fscript, Speed::max())
            .await;

        let mut fscript = FScript::default();
        fscript.actions.push(FSPoint { pos: 0, at: 0 });
        fscript.actions.push(FSPoint { pos: 0, at: 0 });
        player
            .play_scalar_pattern(Duration::from_millis(200), fscript, Speed::max())
            .await;
    }

    #[tokio::test]
    async fn test_scalar_pattern_actuator_selection() {
        // arrange
        let client = get_test_client(vec![scalars(1, "vib1", ActuatorType::Vibrate, 2)]).await;
        let actuators = client.created_devices.clone().flatten_actuators();
       
        // act
        let start = Instant::now();

        let mut player = PlayerTest::setup(vec![actuators[1].clone()]);
        let mut fs1 = FScript::default();
        fs1.actions.push(FSPoint { pos: 10, at: 0 });
        fs1.actions.push(FSPoint { pos: 20, at: 100 });
        player
            .play_scalar_pattern(
                Duration::from_millis(125),
                fs1,
                Speed::max(),
            )
            .await;

        let mut player2 = PlayerTest::setup(vec![actuators[0].clone()]);
        let mut fs2 = FScript::default();
        fs2.actions.push(FSPoint { pos: 30, at: 0 });
        fs2.actions.push(FSPoint { pos: 40, at: 100 });
        player2
            .play_scalar_pattern(
                Duration::from_millis(125),
                fs2,
                Speed::max()
            )
            .await;

        // assert
        client.print_device_calls(start);
        let calls = client.get_device_calls(1);
        calls[0].assert_strengths(vec![(1, 0.1)]);
        calls[1].assert_strengths(vec![(1, 0.2)]);
        calls[4].assert_strengths(vec![(0, 0.3)]);
        calls[5].assert_strengths(vec![(0, 0.4)]);
    }

    #[tokio::test]
    async fn test_scalar_pattern_repeats_until_duration_ends() {
        // arrange
        let client = get_test_client(vec![scalar(1, "vib1", ActuatorType::Vibrate)]).await;
        let mut player = PlayerTest::setup_no_settings(&client.created_devices);

        // act
        let mut fs = FScript::default();
        fs.actions.push(FSPoint { pos: 100, at: 0 });
        fs.actions.push(FSPoint { pos: 50, at: 50 });
        fs.actions.push(FSPoint { pos: 70, at: 100 });

        let start = Instant::now();
        player
            .play_scalar_pattern(Duration::from_millis(125), fs, Speed::max())
            .await;

        // assert
        client.print_device_calls(start);
        let calls = client.get_device_calls(1);
        calls[0].assert_strenth(1.0);
        calls[1].assert_strenth(0.5);
        calls[2].assert_strenth(0.7);
        calls[3].assert_strenth(1.0);
        calls[4].assert_strenth(0.0).assert_time(125, start);
        assert_eq!(calls.len(), 5)
    }

    #[tokio::test]
    async fn test_scalar_timing_remains_synced_with_clock() {
        // arrange
        let n = 40;
        let client = get_test_client(vec![scalar(1, "vib1", ActuatorType::Vibrate)]).await;
        let mut player = PlayerTest::setup_no_settings(&client.created_devices);
        let fscript = get_repeated_pattern(n);

        // act
        let start = Instant::now();
        player
            .play_scalar_pattern(get_duration_ms(&fscript), fscript, Speed::max())
            .await;

        // assert
        client.print_device_calls(start);
        check_timing(client.get_device_calls(1), n, start);
    }

    #[tokio::test]
    async fn test_scalar_points_below_min_resolution() {
        // arrange
        let client = get_test_client(vec![scalar(1, "vib1", ActuatorType::Vibrate)]).await;
        let mut player = PlayerTest::setup_with_settings(
            client.created_devices.flatten_actuators().clone(),
            PlayerSettings {
                scalar_resolution_ms: 100,
            },
        );

        let mut fs = FScript::default();
        fs.actions.push(FSPoint { pos: 42, at: 0 });
        fs.actions.push(FSPoint { pos: 1, at: 1 });
        fs.actions.push(FSPoint { pos: 1, at: 99 });
        fs.actions.push(FSPoint { pos: 42, at: 100 });

        // act
        let start = Instant::now();
        player
            .play_scalar_pattern(Duration::from_millis(150), fs, Speed::max())
            .await;

        // assert
        client.print_device_calls(start);
        let calls = client.get_device_calls(1);
        calls[0].assert_strenth(0.42).assert_time(0, start);
        calls[1].assert_strenth(0.42).assert_time(100, start);
    }

    #[tokio::test]
    async fn test_scalar_pattern_control() {
        // arrange
        let client = get_test_client(vec![scalar(1, "vib1", ActuatorType::Vibrate)]).await;
        let mut player = PlayerTest::setup_no_settings(&client.created_devices);

        let mut fs = FScript::default();
        fs.actions.push(FSPoint { pos: 100, at: 0 });
        fs.actions.push(FSPoint { pos: 70, at: 25 });
        fs.actions.push(FSPoint { pos: 0, at: 50 });

        // act
        let start = Instant::now();
        player
            .play_scalar_pattern(Duration::from_millis(50), fs, Speed::new(10))
            .await;

        // assert
        client.print_device_calls(start);
        let calls = client.get_device_calls(1);
        calls[0].assert_strenth(0.1);
        calls[1].assert_strenth(0.07);
        calls[2].assert_strenth(0.0);
    }

    #[tokio::test]
    async fn test_scalar_constant_control() {
        // arrange
        let client = get_test_client(vec![scalar(1, "vib1", ActuatorType::Vibrate)]).await;
        let mut player = PlayerTest::setup_no_settings(&client.created_devices);

        // act
        let start = Instant::now();
        player.play_scalar(Duration::from_millis(300), Speed::new(100));
        wait_ms(100).await;
        player.scheduler.update_task(1, Speed::new(50));
        wait_ms(100).await;
        player.scheduler.update_task(1, Speed::new(10));
        player.await_all().await;

        client.print_device_calls(start);
        client.get_device_calls(1)[0]
            .assert_strenth(1.0)
            .assert_time(0, start);
        client.get_device_calls(1)[1]
            .assert_strenth(0.5)
            .assert_time(100, start);
        client.get_device_calls(1)[2]
            .assert_strenth(0.1)
            .assert_time(200, start);
        client.get_device_calls(1)[3]
            .assert_strenth(0.0)
            .assert_time(300, start);
    }

    #[tokio::test]
    async fn test_clean_finished_tasks() {
        // arrange
        let start = Instant::now();
        let client = get_test_client(vec![scalar(1, "vib1", ActuatorType::Vibrate)]).await;

        let mut player = PlayerTest::setup_no_settings(&client.created_devices);
        player.play_scalar(Duration::from_millis(100), Speed::max());
        for _ in 0..2 {
            player.play_scalar(Duration::from_millis(1), Speed::max());
            player.await_last().await;
        }

        // act
        player.scheduler.clean_finished_tasks();

        // assert
        client.print_device_calls(start);
        assert_eq!(player.scheduler.control_handles.len(), 1);
    }

    // Concurrency Tests

    #[tokio::test]
    async fn test_concurrent_linear_access_2_threads() {
        // call1  |111111111111111111111-->|
        // call2         |2222->|
        // result |111111122222211111111-->|

        // arrange
        let client = get_test_client(vec![scalar(1, "vib1", ActuatorType::Vibrate)]).await;
        let mut player = PlayerTest::setup_no_settings(&client.created_devices);

        // act
        let start = Instant::now();

        player.play_scalar(Duration::from_millis(500), Speed::new(50));
        wait_ms(100).await;
        player.play_scalar(Duration::from_millis(100), Speed::new(100));
        player.await_all().await;

        // assert
        client.print_device_calls(start);
        client.get_device_calls(1)[0].assert_strenth(0.5);
        client.get_device_calls(1)[1].assert_strenth(1.0);
        client.get_device_calls(1)[2].assert_strenth(0.5);
        client.get_device_calls(1)[3].assert_strenth(0.0);
        assert_eq!(client.call_registry.get_device(1).len(), 4);
    }

    #[tokio::test]
    async fn test_concurrent_linear_access_3_threads() {
        // call1  |111111111111111111111111111-->|
        // call2       |22222222222222->|
        // call3            |333->|
        // result |111122222333332222222111111-->|

        // arrange
        let client = get_test_client(vec![scalar(1, "vib1", ActuatorType::Vibrate)]).await;
        let mut player = PlayerTest::setup_no_settings(&client.created_devices);

        // act
        let start = Instant::now();
        player.play_scalar(Duration::from_secs(3), Speed::new(20));
        wait_ms(250).await;

        player.play_scalar(Duration::from_secs(2), Speed::new(40));
        wait_ms(250).await;

        player.play_scalar(Duration::from_secs(1), Speed::new(80));
        player.await_all().await;

        // assert
        client.print_device_calls(start);

        client.get_device_calls(1)[0].assert_strenth(0.2);
        client.get_device_calls(1)[1].assert_strenth(0.4);
        client.get_device_calls(1)[2].assert_strenth(0.8);
        client.get_device_calls(1)[3].assert_strenth(0.4);
        client.get_device_calls(1)[4].assert_strenth(0.2);
        client.get_device_calls(1)[5].assert_strenth(0.0);
        assert_eq!(client.call_registry.get_device(1).len(), 6);
    }

    #[tokio::test]
    async fn test_concurrent_linear_access_3_threads_2() {
        // call1  |111111111111111111111111111-->|
        // call2       |22222222222->|
        // call3            |333333333-->|
        // result |111122222222222233333331111-->|

        // arrange
        let client = get_test_client(vec![scalar(1, "vib1", ActuatorType::Vibrate)]).await;
        let mut player = PlayerTest::setup_no_settings(&client.created_devices);

        // act
        let start = Instant::now();
        player.play_scalar(Duration::from_secs(3), Speed::new(20));
        wait_ms(250).await;

        player.play_scalar(Duration::from_secs(1), Speed::new(40));
        wait_ms(250).await;

        player.play_scalar(Duration::from_secs(1), Speed::new(80));
        player.await_last().await;
        thread::sleep(Duration::from_secs(2));
        player.await_all().await;

        // assert
        client.print_device_calls(start);
        client.get_device_calls(1)[0].assert_strenth(0.2);
        client.get_device_calls(1)[1].assert_strenth(0.4);
        client.get_device_calls(1)[2].assert_strenth(0.8);
        client.get_device_calls(1)[3].assert_strenth(0.8);
        client.get_device_calls(1)[4].assert_strenth(0.2);
        client.get_device_calls(1)[5].assert_strenth(0.0);
        assert_eq!(client.call_registry.get_device(1).len(), 6);
    }

    #[tokio::test]
    async fn test_concurrency_linear_and_pattern() {
        // lin1   |11111111111111111-->|
        // pat1       |23452345234523452345234-->|
        // result |1111111111111111111123452345234-->|

        // arrange
        let client = get_test_client(vec![scalar(1, "vib1", ActuatorType::Vibrate)]).await;
        let mut player = PlayerTest::setup_no_settings(&client.created_devices);

        // act
        let mut fscript = FScript::default();
        for i in 0..10 {
            fscript.actions.push(FSPoint {
                pos: 10 * i,
                at: 100 * i,
            });
        }

        let start = Instant::now();
        player.play_scalar(Duration::from_secs(1), Speed::new(99));
        wait_ms(250).await;
        player
            .play_scalar_pattern(Duration::from_secs(3), fscript, Speed::max())
            .await;

        // assert
        client.print_device_calls(start);
        assert!(client.call_registry.get_device(1).len() > 3);
    }

    #[tokio::test]
    async fn test_concurrency_two_devices_simulatenously_both_are_started_and_stopped() {
        let client = get_test_client(vec![
            scalar(1, "vib1", ActuatorType::Vibrate),
            scalar(2, "vib2", ActuatorType::Vibrate),
        ])
        .await;

        // act
        let start = Instant::now();
        let mut player = PlayerTest::setup(vec![client.get_device(1)].flatten_actuators());
        player.play_scalar(
            Duration::from_millis(300),
            Speed::new(99)
        );
        
        let mut player2 = PlayerTest::setup(vec![client.get_device(2)].flatten_actuators());
        player2.play_scalar(
            Duration::from_millis(200),
            Speed::new(88)
        );

        player.await_all().await;

        // assert
        client.print_device_calls(start);
        client.get_device_calls(1)[0].assert_strenth(0.99);
        client.get_device_calls(1)[1].assert_strenth(0.0);
        client.get_device_calls(2)[0].assert_strenth(0.88);
        client.get_device_calls(2)[1].assert_strenth(0.0);
    }

    async fn wait_ms(ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }

    fn get_duration_ms(fs: &FScript) -> Duration {
        Duration::from_millis(fs.actions.last().unwrap().at as u64)
    }

    fn check_timing(device_calls: Vec<FakeMessage>, n: usize, start: Instant) {
        for i in 0..n - 1 {
            device_calls[i].assert_time((i * 100) as i32, start);
        }
    }

    fn get_repeated_pattern(n: usize) -> FScript {
        let mut fscript = FScript::default();
        for i in 0..n {
            fscript.actions.push(FSPoint {
                pos: (i % 100) as i32,
                at: (i * 100) as i32,
            });
        }
        fscript
    }

    // Utils

    #[test]
    fn speed_correct_conversion() {
        assert_eq!(Speed::new(-1000).as_float(), 0.0);
        assert_eq!(Speed::new(0).as_float(), 0.0);
        assert_eq!(Speed::new(9).as_float(), 0.09);
        assert_eq!(Speed::new(100).as_float(), 1.0);
        assert_eq!(Speed::new(1000).as_float(), 1.0);
    }
}
