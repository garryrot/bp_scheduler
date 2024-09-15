
use anyhow::anyhow;
use buttplug::client::ButtplugClientError;
use rand::Rng;
use anyhow::Error;
use read::read_config;

use std::time::Duration;
use std::{
    fmt::{self},
    time::Instant,
};

use futures::Future;
use tracing::{debug, error, info};

use tokio::runtime::Runtime;

use buttplug::{
    client::ButtplugClient,
    core::{
        connector::{
            new_json_ws_client_connector, ButtplugConnector,
            ButtplugInProcessClientConnectorBuilder,
        },
        message::{ButtplugCurrentSpecClientMessage, ButtplugCurrentSpecServerMessage},
    },
    server::{
        device::hardware::communication::btleplug::BtlePlugCommunicationManagerBuilder,
        ButtplugServerBuilder,
    },
};

use client::input::*;
use client::pattern::*;
use config::linear::*;
use actions::*;

#[cfg(feature = "testing")]
use bp_fakes::FakeDeviceConnector;

use crate::*;

use super::connection::*;
use super::settings::*;
use super::status::*;

pub static ERROR_HANDLE: i32 = -1;

pub struct BpClient {
    pub settings: TkSettings,
    pub connection_events: crossbeam_channel::Receiver<TkConnectionEvent>,
    pub status: Status,
    pub actions: Actions,
    pub buttplug: ButtplugClient,
    connection_result: Result<(), ButtplugClientError>,
    runtime: Runtime,
    scheduler: ButtplugScheduler,
    client_event_sender: crossbeam_channel::Sender<TkConnectionEvent>,
    status_event_sender: crossbeam_channel::Sender<TkConnectionEvent>,
}

impl BpClient {
    pub fn connect_with<T, Fn, Fut>(
        connect_action: Fn,
        provided_settings: Option<TkSettings>,
        type_name: TkConnectionType,
    ) -> Result<BpClient, anyhow::Error>
    where
        Fn: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send,
        T: ButtplugConnector<ButtplugCurrentSpecClientMessage, ButtplugCurrentSpecServerMessage>
            + 'static,
    {
        let settings = provided_settings.unwrap_or_default();
        let (event_sender_client, connection_events) = crossbeam_channel::unbounded();
        let (event_sender_internal, event_receiver_internal) = crossbeam_channel::unbounded();
        let (scheduler, mut worker) = ButtplugScheduler::create(PlayerSettings {
            scalar_resolution_ms: 100,
        });

        let runtime = Runtime::new()?;
        let (buttplug, connection_result) = runtime.block_on(async move {
            info!("connecting");
            let buttplug = ButtplugClient::new("BpClient");
            let result = buttplug.connect(connect_action().await).await;
            (buttplug, result)
        });
        if let Err(err) = connection_result.as_ref() {
            error!("connection error: {:?}", err)
        }
        let client = BpClient {
            connection_events,
            runtime,
            settings: settings.clone(),
            scheduler,
            client_event_sender: event_sender_client.clone(),
            status_event_sender: event_sender_internal.clone(),
            status: Status::new(event_receiver_internal, &settings),
            actions: Actions(vec![]),
            buttplug,
            connection_result
        };
        let event_stream = client.buttplug.event_stream();
        client.runtime.spawn(async move {
            debug!("event thread");
            handle_connection(event_sender_client, event_sender_internal, event_stream).await;
            debug!("event stopped");
        });
        client.runtime.spawn(async move {
            debug!("starting worker thread");
            worker.run_worker_thread().await;
            debug!("worked thread stopped");
        });

        Ok(client)
    }
}

#[cfg(feature = "testing")]
pub fn get_test_connection(settings: TkSettings) -> Result<BpClient, Error> {
    BpClient::connect_with(
        || async move { FakeDeviceConnector::device_demo().0 },
        Some(options),
        TkConnectionType::Test,
    )
}

#[cfg(not(feature = "testing"))]
pub fn get_test_connection(_: TkSettings) -> Result<BpClient, Error> {
    Err(anyhow!("Compiled without testing support"))
}

impl BpClient {
    pub fn connect(settings: TkSettings) -> Result<BpClient, Error> {
        let settings_clone = settings.clone();
        match settings.connection {
            TkConnectionType::WebSocket(endpoint) => {
                let uri = format!("ws://{}", endpoint);
                BpClient::connect_with(
                    || async move { new_json_ws_client_connector(&uri) },
                    Some(settings_clone),
                    TkConnectionType::WebSocket(endpoint),
                )
            }
            TkConnectionType::InProcess => BpClient::connect_with(
                || async move { in_process_connector() },
                Some(settings),
                TkConnectionType::InProcess,
            ),
            TkConnectionType::Test => get_test_connection(settings),
        }
    }

    pub fn read_actions(&mut self) {
        self.actions = Actions( read_config(self.settings.action_path.clone() ));

        info!("read {} actions...", self.actions.0.len());
        for action in self.actions.0.iter() {
            debug!("{:?}", action);
        }
    }

    pub fn scan_for_devices(&self) -> bool {
        info!("start scan");
        let result = self.runtime.block_on(async move {
            self.buttplug.start_scanning().await
        });
        if let Err(err) = result {
            error!("Failed to start scan {:?}", err);
            return false;
        }
        true     
    }

    pub fn stop_scan(&self) -> bool {
        info!("stop scan");
        let result = self.runtime.block_on(async move {
            self.buttplug.stop_scanning().await
        });
        if let Err(err) = result {
            error!("Failed to stop scan {:?}", err);
            return false;
        }
        true
    }

    pub fn stop_all(&mut self) -> bool {
        info!("stop all devices");

        self.scheduler.stop_all();
        let buttplug = &self.buttplug;
        let result = self.runtime.block_on(async move {
            buttplug.stop_all_devices().await
        });

        if let Err(err) = result {
            error!("Failed to queue stop_all {:?}", err);
            return false;
        }
        true
    }

    pub fn disconnect(&mut self) {
        info!("disconnect");
        let buttplug = &self.buttplug;
        let result = self.runtime.block_on(async move {
            buttplug.disconnect().await
        });
        if let Err(err) = result {
            error!("Failed to send disconnect {:?}", err);
        }
    }

    pub fn update(&mut self, handle: i32, speed: Speed) -> bool {
        info!("update");
        self.scheduler.clean_finished_tasks();
        self.scheduler.update_task(handle, speed)
    }

    pub fn stop(&mut self, handle: i32) -> bool {
        info!("stop");
        self.scheduler.stop_task(handle);
        true
    }

    pub fn dispatch_name(
        &mut self,
        actions_name: Vec<String>,
        body_parts: Vec<String>,
        speed: Speed,
        duration: Duration) 
        -> i32 {

        let mut handle = -1;
        for action_name in actions_name {   
            if let Some(action) = self.actions.clone().0.iter().find(|x| x.name == action_name) {
                handle = self.dispatch( action, body_parts.clone(), speed, duration, handle );
            }
        }
        handle
    }

    pub fn dispatch(
        &mut self,
        action: &Action,
        body_parts: Vec<String>,
        speed: Speed,
        duration: Duration,
        existing_handle: i32) -> i32 {
        let mut handle = existing_handle;
        for control in action.control.clone() {
            let filter_parts = match control.get_selector() {
                Selector::All => body_parts.clone(),
                Selector::BodyParts(filter) => filter,
            };
            handle = self._dispatch_control(&action.clone(), control, filter_parts, speed, duration, handle);
        }
        handle
    }

    fn _dispatch_control(
        &mut self,
        action: &Action,
        control: Control,
        body_parts: Vec<String>,
        speed: Speed,
        duration: Duration,
        handle: i32
    ) -> i32 {
        self.scheduler.clean_finished_tasks();
        let action_clone = action.clone();
        let actuators = self.status.connected_actuators();
        let actuator_types = control.get_actuators();
        let pattern_path = self.settings.pattern_path.clone();
        let devices = TkParams::get_enabled_and_selected_devices(
            &actuators,
            &body_parts,
            &actuator_types,
            &self.settings.device_settings.devices,
        );

        let settings = devices
            .iter()
            .map(|x| {
                self.settings
                    .device_settings
                    .get_or_create(x.identifier())
                    .actuator_settings
            })
            .collect();

        let player = self
            .scheduler
            .create_player_with_settings(devices, settings, handle);
        let handle = player.handle;

        info!(handle, "dispatching {:?}", action);
        let client_sender_clone = self.client_event_sender.clone();
        let status_sender_clone = self.status_event_sender.clone();

        self.runtime.spawn(async move {
            let now = Instant::now();
            client_sender_clone
                .send(TkConnectionEvent::ActionStarted(
                    action_clone.clone(),
                    player.actuators.clone(),
                    body_parts,
                    player.handle,
                ))
                .expect("never full");
            let result = match control {
                Control::Scalar(_, Strength::Constant(_), _) => player.play_scalar(duration, speed).await,
                Control::Scalar(_, strength, _) => {
                    let pattern = match strength {
                        Strength::Constant(_) => panic!(),
                        Strength::Funscript(_, pattern) => pattern.clone(),
                        Strength::RandomFunscript(_, patterns) => patterns.get(rand::thread_rng().gen_range(0..patterns.len()-1)).unwrap().clone()
                    };
                    match read_pattern(&pattern_path, &pattern, true) {
                        Some(fscript) => player.play_scalar_pattern(duration, fscript, speed).await,
                        None => panic!("fscript not found"), // todo differnet
                    }
                }
                Control::Stroke(_, _, range) => player.play_linear_stroke(duration, speed, LinearRange {
                    min_ms: range.min_ms,
                    max_ms: range.max_ms,
                    min_pos: range.min_pos,
                    max_pos: range.max_pos,
                    invert: false,
                    scaling: LinearSpeedScaling::Linear,
                }).await,
                Control::StrokePattern(_, _, pattern) => {
                    match read_pattern(&pattern_path, &pattern, false) {
                        Some(_) => player.play_scalar(duration, speed).await,
                        None => panic!("fscript not found"), // todo different
                    }
                },
            };
            info!(handle, "done");
            let event = match result {
                Ok(()) => TkConnectionEvent::ActionDone(action_clone, now.elapsed(), handle),
                Err(err) => TkConnectionEvent::ActionError(err.actuator, err.bp_error.to_string()),
            };
            client_sender_clone.send(event.clone()).expect("never full");
            status_sender_clone.send(event.clone()).expect("never full");
        });

        handle
    }
}

pub fn in_process_connector(
) -> impl ButtplugConnector<ButtplugCurrentSpecClientMessage, ButtplugCurrentSpecServerMessage> {
    ButtplugInProcessClientConnectorBuilder::default()
        .server(
            ButtplugServerBuilder::default()
                .comm_manager(BtlePlugCommunicationManagerBuilder::default())
                .finish()
                .expect("Could not create in-process-server."),
        )
        .finish()
}

impl fmt::Debug for BpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BpClient")
            .field("settings", &self.settings)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use buttplug::core::message::{ActuatorType, DeviceAdded};
    use funscript::FScript;
    use std::time::Instant;
    use std::{thread, time::Duration, vec};

    use super::*;
    use bp_fakes::*;

    macro_rules! assert_timeout {
        ($cond:expr, $arg:tt) => {
            // starting time
            let start: Instant = Instant::now();
            while !$cond {
                thread::sleep(Duration::from_millis(10));
                if start.elapsed().as_secs() > 20 {
                    panic!($arg);
                }
            }
        };
    }
    
    impl BpClient {
        pub fn await_connect(&mut self, devices: usize) {
            assert_timeout!(self.status.actuators().len() >= devices, "Awaiting connect");
        }
    }

    /// Vibrate
    pub fn test_cmd(
        tk: &mut BpClient,
        task: Task,
        duration: Duration,
        body_parts: Vec<String>,
        _: Option<FScript>,
        _: &[ActuatorType],
    ) -> i32 {
        let speed: Speed = match task {
            Task::Scalar(speed) => speed,
            Task::Pattern(speed, _, _) => speed,
            Task::Linear(speed, _) => speed,
            Task::LinearStroke(speed, _) => speed,
        };
        tk.actions = Actions(vec![
            Action::build(
                "foobar", 
                vec![
                    Control::Scalar( 
                        Selector::All,
                        Strength::Constant(100),
                        vec![ ScalarActuators::Vibrate ],
                    )] 
            )
        ]);
        tk.dispatch_name( vec![ "foobar".into() ], body_parts, speed, duration )
    }

    #[test]
    fn test_vibrate_and_stop() {
        // arrange
        let (mut tk, call_registry) =
            wait_for_connection(vec![scalar(1, "vib1", ActuatorType::Vibrate)], None);

        // act
        let handle = test_cmd(
            &mut tk,
            Task::Scalar(Speed::max()),
            Duration::MAX,
            vec![],
            None,
            &[ActuatorType::Vibrate],
        );
        thread::sleep(Duration::from_secs(1));
        call_registry.get_device(1)[0].assert_strenth(1.0);

        tk.stop(handle);
        thread::sleep(Duration::from_secs(1));
        call_registry.get_device(1)[1].assert_strenth(0.0);
    }

    #[test]
    fn test_vibrate_and_stop_all() {
        // arrange
        let (mut tk, call_registry) =
            wait_for_connection(vec![scalar(1, "vib1", ActuatorType::Vibrate)], None);

        // act
        thread::sleep(Duration::from_secs(1));
        test_cmd(
            &mut tk,
            Task::Scalar(Speed::max()),
            Duration::from_secs(1),
            vec![],
            None,
            &[ActuatorType::Vibrate],
        );
        thread::sleep(Duration::from_secs(2));
        call_registry.get_device(1)[0].assert_strenth(1.0);
        tk.stop_all();

        thread::sleep(Duration::from_secs(1));
        call_registry.get_device(1)[1].assert_strenth(0.0);
    }

    #[test]
    fn vibrate_all_demo_vibrators() {
        // arrange
        let (connector, call_registry) = FakeDeviceConnector::device_demo();
        let count = connector.devices.len();

        // act
        let mut tk =
            BpClient::connect_with(|| async move { connector }, None, TkConnectionType::Test)
                .unwrap();
        tk.await_connect(count);
        for actuator_id in tk.status.get_known_actuator_ids() {
            tk.settings.device_settings.set_enabled(&actuator_id, true);
        }
        test_cmd(
            &mut tk,
            Task::Scalar(Speed::new(100)),
            Duration::from_millis(1),
            vec![],
            None,
            &[ActuatorType::Vibrate],
        );

        // assert
        thread::sleep(Duration::from_millis(500));
        call_registry.get_device(1)[0].assert_strenth(1.0);
        call_registry.get_device(1)[1].assert_strenth(0.0);
        call_registry.assert_unused(4); // linear
        call_registry.assert_unused(7); // rotator
    }

    #[test]
    fn vibrate_non_existing_device() {
        // arrange
        let (mut tk, call_registry) =
            wait_for_connection(vec![scalar(1, "vib1", ActuatorType::Vibrate)], None);

        // act
        test_cmd(
            &mut tk,
            Task::Scalar(Speed::max()),
            Duration::from_millis(1),
            vec![String::from("does not exist")],
            None,
            &[ActuatorType::Vibrate],
        );
        thread::sleep(Duration::from_millis(50));

        // assert
        call_registry.assert_unused(1);
    }

    #[test]
    fn settings_only_vibrate_enabled_devices() {
        // arrange
        let (mut tk, call_registry) = wait_for_connection(
            vec![
                scalar(1, "vib1", ActuatorType::Vibrate),
                scalar(2, "vib2", ActuatorType::Vibrate),
                scalar(3, "vib3", ActuatorType::Vibrate),
            ],
            None,
        );
        tk.settings
            .device_settings
            .set_enabled("vib2 (Vibrate)", false);

        // act
        test_cmd(
            &mut tk,
            Task::Scalar(Speed::max()),
            Duration::from_millis(1),
            vec![],
            None,
            &[ActuatorType::Vibrate],
        );
        thread::sleep(Duration::from_secs(1));

        // assert
        call_registry.get_device(1)[0].assert_strenth(1.0);
        call_registry.get_device(1)[1].assert_strenth(0.0);
        call_registry.get_device(3)[0].assert_strenth(1.0);
        call_registry.get_device(3)[1].assert_strenth(0.0);
        call_registry.assert_unused(2);
    }

    /// Vibrate (E2E)

    #[test]
    #[ignore = "Requires one (1) vibrator to be connected via BTLE (vibrates it)"]
    fn vibrate_pattern() {
        let (mut tk, handle) = test_pattern("02_Cruel-Tease", Duration::from_secs(10), true);
        thread::sleep(Duration::from_secs(2)); // dont disconnect
        tk.stop(handle);
        thread::sleep(Duration::from_secs(10));
    }

    fn test_pattern(
        pattern_name: &str,
        duration: Duration,
        vibration_pattern: bool,
    ) -> (BpClient, i32) {
        let settings = TkSettings::new();
        let pattern_path = String::from("../deploy/Data/SKSE/Plugins/BpClient/Patterns");
        let mut tk = BpClient::connect_with(
            || async move { in_process_connector() },
            Some(settings),
            TkConnectionType::Test,
        )
        .unwrap();
        tk.scan_for_devices();
        tk.await_connect(1);
        thread::sleep(Duration::from_secs(2));
        let known_actuator_ids = tk.status.get_known_actuator_ids();
        tk.settings
            .device_settings
            .set_enabled(known_actuator_ids.first().unwrap(), true);

        let fscript = read_pattern(&pattern_path, pattern_name, vibration_pattern).unwrap();
        let handle = test_cmd(
            &mut tk,
            Task::Pattern(Speed::max(), ActuatorType::Vibrate, pattern_name.into()),
            duration,
            vec![],
            Some(fscript),
            &[ActuatorType::Vibrate],
        );
        (tk, handle)
    }

    /// Intiface (E2E)

    #[test]
    #[ignore = "Requires intiface to be connected, with a connected device (vibrates it)"]
    fn intiface_test_vibration() {
        let mut settings = TkSettings::new();
        settings.connection = TkConnectionType::WebSocket(String::from("127.0.0.1:12345"));

        let mut tk = BpClient::connect(settings).unwrap();
        tk.scan_for_devices();

        thread::sleep(Duration::from_secs(5));
        assert!(tk.connection_result.is_ok());
        for actuator in tk.status.actuators() {
            tk.settings
                .device_settings
                .set_enabled(actuator.device.name(), true);
        }
        test_cmd(
            &mut tk,
            Task::Scalar(Speed::max()),
            Duration::MAX,
            vec![],
            None,
            &[ActuatorType::Vibrate],
        );
        thread::sleep(Duration::from_secs(5));
    }

    #[test]
    fn intiface_not_available_connection_status_error() {
        let mut settings = TkSettings::new();
        settings.connection = TkConnectionType::WebSocket(String::from("bogushost:6572"));
        let mut tk = BpClient::connect(settings).unwrap();
        tk.scan_for_devices();
        thread::sleep(Duration::from_secs(5));
        match tk.connection_result {
            Ok(_) => panic!("should not be ok"),
            Err(err) => {}
        };
    }

    /// Settings

    #[test]
    fn settings_are_trimmed_and_lowercased() {
        let (mut tk, call_registry) =
            wait_for_connection(vec![scalar(1, "vib1", ActuatorType::Vibrate)], None);
        tk.settings
            .device_settings
            .set_enabled("vib1 (Vibrate)", true);
        tk.settings
            .device_settings
            .set_events("vib1 (Vibrate)", &[String::from(" SoMe EvEnT    ")]);
        test_cmd(
            &mut tk,
            Task::Scalar(Speed::max()),
            Duration::from_millis(1),
            vec![String::from("some event")],
            None,
            &[ActuatorType::Vibrate],
        );

        thread::sleep(Duration::from_millis(500));
        call_registry.get_device(1)[0].assert_strenth(1.0);
        call_registry.get_device(1)[1].assert_strenth(0.0);
    }

    #[test]
    fn get_devices_contains_connected_devices() {
        // arrange
        let (mut tk, _) = wait_for_connection(
            vec![
                scalar(1, "vib1", ActuatorType::Vibrate),
                scalar(2, "vib2", ActuatorType::Inflate),
            ],
            None,
        );

        // assert
        assert_timeout!(tk.status.actuators().len() == 2, "Enough devices connected");
        assert!(
            tk.status
                .get_known_actuator_ids()
                .contains(&String::from("vib1 (Vibrate)")),
            "Contains name vib1"
        );
        assert!(
            tk.status
                .get_known_actuator_ids()
                .contains(&String::from("vib2 (Inflate)")),
            "Contains name vib2"
        );
    }

    #[test]
    fn get_devices_contains_devices_from_settings() {
        let mut settings = TkSettings::new();
        settings.device_settings.set_enabled("foreign", true);

        let (mut tk, _) = wait_for_connection(vec![], Some(settings));
        assert!(
            tk.status
                .get_known_actuator_ids()
                .contains(&String::from("foreign")),
            "Contains additional device from settings"
        );
    }

    #[test]
    fn events_get() {
        let empty: Vec<String> = vec![];
        let one_event = &[String::from("evt2")];
        let two_events = &[String::from("evt2"), String::from("evt3")];

        let (mut tk, _) = wait_for_connection(
            vec![
                scalar(1, "vib1", ActuatorType::Vibrate),
                scalar(2, "vib2", ActuatorType::Vibrate),
                scalar(3, "vib3", ActuatorType::Vibrate),
            ],
            None,
        );

        tk.settings.device_settings.set_events("vib2", one_event);
        tk.settings.device_settings.set_events("vib3", two_events);

        assert_eq!(tk.settings.device_settings.get_events("vib1"), empty);
        assert_eq!(tk.settings.device_settings.get_events("vib2"), one_event);
        assert_eq!(tk.settings.device_settings.get_events("vib3"), two_events);
    }

    #[test]
    fn event_only_vibrate_selected_devices() {
        let (mut tk, call_registry) = wait_for_connection(
            vec![
                scalar(1, "vib1", ActuatorType::Vibrate),
                scalar(2, "vib2", ActuatorType::Vibrate),
            ],
            None,
        );
        tk.settings
            .device_settings
            .set_events("vib1 (Vibrate)", &[String::from("selected_event")]);
        tk.settings
            .device_settings
            .set_events("vib2 (Vibrate)", &[String::from("bogus")]);

        test_cmd(
            &mut tk,
            Task::Scalar(Speed::max()),
            Duration::from_millis(1),
            vec![String::from("selected_event")],
            None,
            &[ActuatorType::Vibrate],
        );
        thread::sleep(Duration::from_secs(1));

        call_registry.get_device(1)[0].assert_strenth(1.0);
        call_registry.get_device(1)[1].assert_strenth(0.0);
        call_registry.assert_unused(2);
    }

    #[test]
    fn event_is_trimmed_and_ignores_casing() {
        let (mut tk, call_registry) =
            wait_for_connection(vec![scalar(1, "vib1", ActuatorType::Vibrate)], None);
        tk.settings
            .device_settings
            .set_enabled("vib1 (Vibrate)", true);
        tk.settings
            .device_settings
            .set_events("vib1 (Vibrate)", &[String::from("some event")]);
        test_cmd(
            &mut tk,
            Task::Scalar(Speed::max()),
            Duration::from_millis(1),
            vec![String::from(" SoMe EvEnT    ")],
            None,
            &[ActuatorType::Vibrate],
        );

        thread::sleep(Duration::from_millis(500));
        call_registry.get_device(1)[0].assert_strenth(1.0);
        call_registry.get_device(1)[1].assert_strenth(0.0);
    }

    /// Device Status
    #[test]
    fn get_device_connected() {
        let (mut tk, _) =
            wait_for_connection(vec![scalar(1, "existing", ActuatorType::Vibrate)], None);
        assert_eq!(
            tk.status
                .get_actuator_connection_status("existing (Vibrate)"),
            TkConnectionStatus::Connected,
            "Existing device returns connected"
        );
        assert_eq!(
            tk.status
                .get_actuator_connection_status("not existing (Vibrate)"),
            TkConnectionStatus::NotConnected,
            "Non-existing device returns not connected"
        );
    }

    fn wait_for_connection(
        devices: Vec<DeviceAdded>,
        settings: Option<TkSettings>,
    ) -> (BpClient, FakeConnectorCallRegistry) {
        let (connector, call_registry) = FakeDeviceConnector::new(devices);
        let count = connector.devices.len();

        // act
        let mut settings = settings.unwrap_or(TkSettings::new());
        settings.pattern_path = String::from("../deploy/Data/SKSE/Plugins/BpClient/Patterns");
        let mut tk = BpClient::connect_with(
            || async move { connector },
            Some(settings),
            TkConnectionType::Test,
        )
        .unwrap();
        tk.await_connect(count);

        for actuator in tk.status.actuators() {
            tk.settings
                .device_settings
                .set_enabled(actuator.identifier(), true);
        }
        (tk, call_registry)
    }
}
