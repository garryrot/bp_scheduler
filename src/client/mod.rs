use std::time::Duration;
use std::{
    fmt::{self},
    time::Instant,
};

use actuator::ActuatorConfigLoader;
use anyhow::anyhow;
use anyhow::Error;

use connection::ConnectionType;
use rand::Rng;

use futures::Future;
use tracing::{debug, error, info};

use tokio::runtime::Runtime;

use buttplug::client::{ButtplugClient, ButtplugClientError};
use buttplug::server::device::hardware::communication::serialport::SerialPortCommunicationManagerBuilder;
use buttplug::server::device::hardware::communication::xinput::XInputDeviceCommunicationManagerBuilder;
use buttplug::{
    core::{connector::*, message::*},
    server::{
        device::hardware::communication::btleplug::BtlePlugCommunicationManagerBuilder,
        ButtplugServerBuilder,
    },
};
use util::trim_lower_str_list;

use crate::filter::Filter;
use crate::*;

use config::client::*;
use read::read_config;
use pattern::read_pattern;
use actions::*;
use config::linear::*;

#[cfg(feature = "testing")]
use bp_fakes::FakeDeviceConnector;

#[cfg(feature = "testing")]
pub fn get_test_connection(settings: ClientSettings) -> Result<BpClient, Error> {
    BpClient::connect_with(
        || async move { FakeDeviceConnector::device_demo().0 },
        Some(options),
        ConnectionType::Test,
    )
}

#[cfg(not(feature = "testing"))]
pub fn get_test_connection(_: ClientSettings) -> Result<BpClient, Error> {
    Err(anyhow!("Compiled without testing support"))
}

pub struct BpClient {
    pub settings: ClientSettings,
    pub actions: Actions,
    pub buttplug: ButtplugClient,
    pub runtime: Runtime,
    pub connection_result: Result<(), ButtplugClientError>,
    pub scheduler: ButtplugScheduler,
}

impl BpClient {
    pub fn connect_with<T, Fn, Fut>(
        connect_action: Fn,
        connection_settings: Option<ClientSettings>,
    ) -> Result<BpClient, anyhow::Error>
    where
        Fn: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send,
        T: ButtplugConnector<ButtplugCurrentSpecClientMessage, ButtplugCurrentSpecServerMessage>
            + 'static,
    {
        let settings = connection_settings.unwrap_or_default();
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
            runtime,
            settings: settings.clone(),
            scheduler,
            actions: Actions(vec![]),
            buttplug,
            connection_result,
        };
        client.runtime.spawn(async move {
            debug!("starting worker thread");
            worker.run_worker_thread().await;
            debug!("worked thread stopped");
        });
        Ok(client)
    }
}

fn in_process_connector(
    features: InProcessFeatures,
) -> impl ButtplugConnector<ButtplugCurrentSpecClientMessage, ButtplugCurrentSpecServerMessage> {
    info!(?features, "connecting in process");
    let mut builder = ButtplugServerBuilder::default();
    if features.bluetooth {
        builder.comm_manager(BtlePlugCommunicationManagerBuilder::default());
    }
    if features.serial {
        builder.comm_manager(SerialPortCommunicationManagerBuilder::default());
    }
    if features.xinput {
        builder.comm_manager(XInputDeviceCommunicationManagerBuilder::default());
    }
    let server = builder
        .finish()
        .expect("Could not create in-process-server.");
    ButtplugInProcessClientConnectorBuilder::default()
        .server(server)
        .finish()
}

impl BpClient {
    pub fn connect(settings: ClientSettings) -> Result<BpClient, Error> {
        let settings_clone = settings.clone();
        match settings.connection {
            ConnectionType::WebSocket(endpoint) => {
                let uri = format!("ws://{}", endpoint);
                BpClient::connect_with(
                    || async move { new_json_ws_client_connector(&uri) },
                    Some(settings_clone),
                )
            }
            ConnectionType::InProcess => BpClient::connect_with(
                move || async move { in_process_connector(settings.in_process_features) },
                Some(settings),
            ),
            ConnectionType::Test => get_test_connection(settings),
        }
    }

    pub fn read_actions(&mut self) {
        self.actions = Actions(read_config(self.settings.action_path.clone()));

        info!("read {} actions...", self.actions.0.len());
        for action in self.actions.0.iter() {
            debug!("{:?}", action);
        }
    }

    pub fn scan_for_devices(&self) -> bool {
        info!("start scan");
        let result = self
            .runtime
            .block_on(async move { self.buttplug.start_scanning().await });
        if let Err(err) = result {
            error!("Failed to start scan {:?}", err);
            return false;
        }
        true
    }

    pub fn stop_scan(&self) -> bool {
        info!("stop scan");
        let result = self
            .runtime
            .block_on(async move { self.buttplug.stop_scanning().await });
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
        let result = self
            .runtime
            .block_on(async move { buttplug.stop_all_devices().await });

        if let Err(err) = result {
            error!("Failed to queue stop_all {:?}", err);
            return false;
        }
        true
    }

    pub fn disconnect(&mut self) {
        info!("disconnect");
        let buttplug = &self.buttplug;
        let result = self
            .runtime
            .block_on(async move { buttplug.disconnect().await });
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

    pub fn dispatch_refs(
        &mut self,
        action_refs: Vec<ActionRef>,
        body_parts: Vec<String>,
        speed: Speed,
        duration: Duration,
    ) -> i32 {
        let mut handle = -1;
        for action_ref in action_refs {
            if let Some(action) = self
                .actions
                .clone()
                .0
                .iter()
                .find(|x| x.name == action_ref.action)
            {
                let strength = action_ref.strength.multiply(&speed);
                for control in action.control.clone() {
                    let ext_selector = Selector::from(&body_parts);
                    handle = self.dispatch(
                        action.name.clone(),
                        match control {
                            Control::Scalar(selector, actuators) => {
                                Control::Scalar(selector.and(ext_selector), actuators)
                            }
                            Control::Stroke(selector, stroke_range) => {
                                Control::Stroke(selector.and(ext_selector), stroke_range)
                            }
                        },
                        strength.clone(),
                        duration,
                        handle,
                    );
                }
            }
        }
        handle
    }

    pub fn dispatch(
        &mut self,
        action_name: String,
        control: Control,
        strength: Strength,
        duration: Duration,
        handle: i32,
    ) -> i32 {
        self.scheduler.clean_finished_tasks();
        let body_parts = control.get_selector().as_vec();

        let (updated_settings, actuators) = Filter::new(self.settings.device_settings.clone(), &self.buttplug.devices())
            .connected()
            .enabled()
            .with_actuator_types(&control.get_actuators())
            .with_body_parts(&trim_lower_str_list(&body_parts))
            .result();

        self.settings.device_settings = updated_settings;
        let pattern_path = self.settings.pattern_path.clone();

        let player = self
            .scheduler
            .create_player(actuators.load_config(&mut self.settings.device_settings), handle);
        let handle = player.handle;
        info!(
            handle,
            "dispatching {:?} {:?} {:?}", action_name, strength, control
        );

        self.runtime.spawn(async move {
            let now = Instant::now();
            info!(
                "action started {:?} {:?} {:?} {:?}",
                action_name, player.actuators, body_parts, player.handle
            );
            let result = match control {
                Control::Scalar(_, _) => match strength {
                    Strength::Constant(speed) => {
                        player.play_scalar(duration, Speed::new(speed.into())).await
                    }
                    Strength::Funscript(speed, pattern) => {
                        match read_pattern(&pattern_path, &pattern, true) {
                            Some(fscript) => {
                                player
                                    .play_scalar_pattern(
                                        duration,
                                        fscript,
                                        Speed::new(speed.into()),
                                    )
                                    .await
                            }
                            None => {
                                error!("error reading pattern {}", pattern);
                                player.play_scalar(duration, Speed::new(speed.into())).await
                            }
                        }
                    }
                    Strength::RandomFunscript(speed, patterns) => {
                        let pattern = patterns
                            .get(rand::thread_rng().gen_range(0..patterns.len() - 1))
                            .unwrap()
                            .clone();
                        match read_pattern(&pattern_path, &pattern, true) {
                            Some(fscript) => {
                                player
                                    .play_scalar_pattern(
                                        duration,
                                        fscript,
                                        Speed::new(speed.into()),
                                    )
                                    .await
                            }
                            None => {
                                error!("error reading pattern {}", pattern);
                                player.play_scalar(duration, Speed::new(speed.into())).await
                            }
                        }
                    }
                },
                Control::Stroke(_, range) => match strength {
                    Strength::Constant(speed) => {
                        player
                            .play_linear_stroke(
                                duration,
                                Speed::new(speed.into()),
                                LinearRange {
                                    min_ms: range.min_ms,
                                    max_ms: range.max_ms,
                                    min_pos: range.min_pos,
                                    max_pos: range.max_pos,
                                    invert: false,
                                    scaling: LinearSpeedScaling::Linear,
                                },
                            )
                            .await
                    }
                    Strength::Funscript(speed, pattern) => {
                        match read_pattern(&pattern_path, &pattern, true) {
                            Some(fscript) => player.play_linear(duration, fscript).await,
                            None => {
                                error!("error reading pattern {}", pattern);
                                player
                                    .play_linear_stroke(
                                        duration,
                                        Speed::new(speed.into()),
                                        LinearRange::max(),
                                    )
                                    .await
                            }
                        }
                    }
                    Strength::RandomFunscript(speed, patterns) => {
                        let pattern = patterns
                            .get(rand::thread_rng().gen_range(0..patterns.len() - 1))
                            .unwrap()
                            .clone();
                        match read_pattern(&pattern_path, &pattern, false) {
                            Some(fscript) => player.play_linear(duration, fscript).await,
                            None => {
                                error!("error reading pattern {}", pattern);
                                player
                                    .play_linear_stroke(
                                        duration,
                                        Speed::new(speed.into()),
                                        LinearRange::max(),
                                    )
                                    .await
                            }
                        }
                    }
                },
            };
            info!(handle, "done");

            match result {
                Ok(()) => {
                    info!(
                        "action done {:?} {:?} {:?}",
                        action_name,
                        now.elapsed(),
                        handle
                    );
                }
                Err(err) => {
                    error!(
                        "action error {:?} {:?} {:?} {:?}",
                        err,
                        action_name,
                        now.elapsed(),
                        handle
                    )
                }
            };
        });

        handle
    }
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
    use actuator::Actuators;
    use buttplug::client::ButtplugClientDevice;
    use buttplug::core::message::{ActuatorType, DeviceAdded};
    use funscript::FScript;
    use itertools::Itertools;
    use pattern::read_pattern;
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
            assert_timeout!(self.buttplug.devices().len() >= devices, "Awaiting connect");
        }
    }

    /// Vibrate
    pub fn test_cmd(
        tk: &mut BpClient,
        strength: Strength,
        duration: Duration,
        body_parts: Vec<String>,
        _: Option<FScript>,
        actuators: &[ScalarActuator],
    ) -> i32 {
        tk.actions = Actions(vec![Action::new(
            "foobar",
            vec![Control::Scalar(
                Selector::All,
                actuators.to_vec(),
            )],
        )]);
        tk.dispatch_refs(
            vec![ActionRef {
                action: "foobar".into(),
                strength,
            }],
            body_parts,
            Speed::max(),
            duration,
        )
    }

    #[test]
    fn test_vibrate_and_stop() {
        // arrange
        let (mut tk, call_registry) =
            wait_for_connection(vec![scalar(1, "vib1", ActuatorType::Vibrate)], None);

        // act
        let handle = test_cmd(
            &mut tk,
            Strength::Constant(100),
            Duration::MAX,
            vec![],
            None,
            &[ScalarActuator::Vibrate],
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
            Strength::Constant(100),
            Duration::from_secs(1),
            vec![],
            None,
            &[ScalarActuator::Vibrate],
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
        let mut tk = BpClient::connect_with(|| async move { connector }, None).unwrap();
        tk.await_connect(count);
        for actuator_id in &get_known_actuator_ids(tk.buttplug.devices(), &tk.settings) {
            tk.settings.device_settings.set_enabled(actuator_id, true);
        }
        test_cmd(
            &mut tk,
            Strength::Constant(100),
            Duration::from_millis(1),
            vec![],
            None,
            &[ScalarActuator::Vibrate],
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
            Strength::Constant(100),
            Duration::from_millis(1),
            vec![String::from("does not exist")],
            None,
            &[ScalarActuator::Vibrate],
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
            Strength::Constant(100),
            Duration::from_millis(1),
            vec![],
            None,
            &[ScalarActuator::Vibrate],
        );
        thread::sleep(Duration::from_secs(1));

        // assert
        call_registry.get_device(1)[0].assert_strenth(1.0);
        call_registry.get_device(1)[1].assert_strenth(0.0);
        call_registry.get_device(3)[0].assert_strenth(1.0);
        call_registry.get_device(3)[1].assert_strenth(0.0);
        call_registry.assert_unused(2);
    }

    
    #[test]
    fn settings_only_move_selected_actuators() {
        // arrange
        let (mut tk, call_registry) = wait_for_connection(
            vec![
                scalar(1, "vib1", ActuatorType::Vibrate),
                scalar(2, "vib2", ActuatorType::Inflate)
            ],
            None,
        );
        tk.settings
            .device_settings
            .set_enabled("vib1 (Vibrate)", true);
        tk.settings
            .device_settings
            .set_enabled("vib2 (Inflate)", true);
        
        // act
        test_cmd(
            &mut tk,
            Strength::Constant(99),
            Duration::from_millis(1),
            vec![],
            None,
            &[ScalarActuator::Inflate],
        );
        thread::sleep(Duration::from_secs(1));

        // assert
        call_registry.get_device(2)[0].assert_strenth(0.99);
        call_registry.get_device(2)[1].assert_strenth(0.0);
        call_registry.assert_unused(0);
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
        let settings = ClientSettings::default();
        let pattern_path = "TODO/Define/Me";
        let mut tk =
            BpClient::connect_with(|| async move { in_process_connector(InProcessFeatures { bluetooth: true, serial: false, xinput: false }) }, Some(settings))
                .unwrap();
        tk.scan_for_devices();
        tk.await_connect(1);
        thread::sleep(Duration::from_secs(2));
        let known_actuator_ids = get_known_actuator_ids(tk.buttplug.devices(), &tk.settings);
        tk.settings
            .device_settings
            .set_enabled(known_actuator_ids.first().unwrap(), true);

        let fscript = read_pattern(&pattern_path, pattern_name, vibration_pattern).unwrap();
        let handle = test_cmd(
            &mut tk,
            Strength::Funscript(100, pattern_name.into()),
            duration,
            vec![],
            Some(fscript),
            &[ScalarActuator::Vibrate],
        );
        (tk, handle)
    }

    /// Intiface (E2E)

    #[test]
    #[ignore = "Requires intiface to be connected, with a connected device (vibrates it)"]
    fn intiface_test_vibration() {
        let mut settings = ClientSettings::default();
        settings.connection = ConnectionType::WebSocket(String::from("127.0.0.1:12345"));

        let mut tk = BpClient::connect(settings).unwrap();
        tk.scan_for_devices();

        thread::sleep(Duration::from_secs(5));
        assert!(tk.connection_result.is_ok());
        for actuator in tk.buttplug.devices().flatten_actuators() {
            tk.settings
                .device_settings
                .set_enabled(actuator.device.name(), true);
        }
        test_cmd(
            &mut tk,
            Strength::Constant(100),
            Duration::MAX,
            vec![],
            None,
            &[ScalarActuator::Vibrate],
        );
        thread::sleep(Duration::from_secs(5));
    }

    #[test]
    fn intiface_not_available_connection_status_error() {
        let settings = ClientSettings { connection: ConnectionType::WebSocket(String::from("bogushost:6572")), ..Default::default() };
        let tk = BpClient::connect(settings).unwrap();
        tk.scan_for_devices();
        thread::sleep(Duration::from_secs(5));
        if tk.connection_result.is_ok() {
            panic!("should not be ok");
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
            Strength::Constant(100),
            Duration::from_millis(1),
            vec![String::from("some event")],
            None,
            &[ScalarActuator::Vibrate],
        );

        thread::sleep(Duration::from_millis(500));
        call_registry.get_device(1)[0].assert_strenth(1.0);
        call_registry.get_device(1)[1].assert_strenth(0.0);
    }

    #[test]
    fn get_devices_contains_connected_devices() {
        // arrange
        let (tk, _) = wait_for_connection(
            vec![
                scalar(1, "vib1", ActuatorType::Vibrate),
                scalar(2, "vib2", ActuatorType::Inflate),
            ],
            None,
        );

        // assert
        assert_timeout!(tk.buttplug.devices().len() == 2, "Enough devices connected");
        assert!(
            get_known_actuator_ids(tk.buttplug.devices(), &tk.settings)
                .contains(&String::from("vib1 (Vibrate)")),
            "Contains name vib1"
        );
        assert!(
            get_known_actuator_ids(tk.buttplug.devices(), &tk.settings)
                .contains(&String::from("vib2 (Inflate)")),
            "Contains name vib2"
        );
    }

    #[test]
    fn get_devices_contains_devices_from_settings() {
        let mut settings = ClientSettings::default();
        settings.device_settings.set_enabled("foreign", true);

        let (tk, _) = wait_for_connection(vec![], Some(settings));
        assert!(
            get_known_actuator_ids(tk.buttplug.devices(), &tk.settings)
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
            Strength::Constant(100),
            Duration::from_millis(1),
            vec![String::from("selected_event")],
            None,
            &[ScalarActuator::Vibrate],
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
            Strength::Constant(100),
            Duration::from_millis(1),
            vec![String::from(" SoMe EvEnT    ")],
            None,
            &[ScalarActuator::Vibrate],
        );

        thread::sleep(Duration::from_millis(500));
        call_registry.get_device(1)[0].assert_strenth(1.0);
        call_registry.get_device(1)[1].assert_strenth(0.0);
    }

    fn wait_for_connection(
        devices: Vec<DeviceAdded>,
        settings: Option<ClientSettings>,
    ) -> (BpClient, FakeConnectorCallRegistry) {
        let (connector, call_registry) = FakeDeviceConnector::new(devices);
        let count = connector.devices.len();

        // act
        let mut settings = settings.unwrap_or_default();
        settings.pattern_path = String::from("../deploy/Data/SKSE/Plugins/BpClient/Patterns");
        let mut tk = BpClient::connect_with(|| async move { connector }, Some(settings)).unwrap();
        tk.await_connect(count);

        let actuators = tk.buttplug.devices().flatten_actuators();
        for actuator in actuators {
            tk.settings
                .device_settings
                .set_enabled(actuator.identifier(), true);
        }
        (tk, call_registry)
    }

    fn get_known_actuator_ids(devices: Vec<Arc<ButtplugClientDevice>>, settings: &ClientSettings) -> Vec<String> {
        let known_actuators : Vec<String> = settings
                .device_settings
                .devices
                .iter()
                .map(|x| x.actuator_id.clone())
                .collect();
    
        let known_ids = known_actuators.clone();
        devices.flatten_actuators()
            .iter()
            .map(|x| String::from(x.identifier()))
            .chain(known_ids)
            .unique()
            .collect()
    }
}
