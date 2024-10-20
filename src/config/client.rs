use std::{
    fmt::{self, Display}, fs::{self}, path::PathBuf, vec
};
use buttplug::core::message::LogLevel;
use serde::{Deserialize, Serialize};
use tracing::{error, event, info, Level};

use crate::actuators::BpSettings;

use super::connection::ConnectionType;

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct InProcessFeatures {
    pub bluetooth: bool,
    pub serial: bool,
    pub xinput: bool
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientSettings {
    pub version: u32,
    pub log_level: LogLevel,
    pub connection: ConnectionType,
    pub in_process_features: InProcessFeatures,
    pub device_settings: BpSettings,
    #[serde(skip)]
    pub pattern_path: String,
    #[serde(skip)]
    pub action_path: String,
}

impl ClientSettings {
    pub fn new() -> Self {
        ClientSettings {
            version: 3,
            log_level: LogLevel::Debug,
            connection: ConnectionType::InProcess,
            device_settings: BpSettings {
                devices: vec![]
            },
            pattern_path: "".into(),
            action_path: "".into(),
            in_process_features: InProcessFeatures {
                bluetooth: true,
                serial: true,
                xinput: true,
            },
        }
    }

    pub fn try_read_or_default(settings_path: &str, settings_file: &str) -> Self {
        Self::try_read_or( settings_path, settings_file, ClientSettings::default() )
    }

    pub fn try_read_or(settings_path: &str, settings_file: &str, or: ClientSettings) -> Self {
        let path: PathBuf = [settings_path, settings_file].iter().collect::<PathBuf>();
        match fs::read_to_string(path) {
            Ok(settings_json) => match serde_json::from_str::<ClientSettings>(&settings_json) {
                Ok(settings) => {
                    settings
                }
                Err(err) => {
                    error!("Settings path '{}' could not be parsed. Error: {}. Using default configuration.", settings_path, err);
                    or
                }
            },
            Err(err) => {
                info!("Settings path '{}' could not be opened. Error: {}. Using default configuration.", settings_path, err);
                or
            }
        }
    }

    pub fn try_write(&self, settings_path: &str, settings_file: &str) -> bool {
        let json = serde_json::to_string_pretty(self).expect("Always serializable");
        let _ = fs::create_dir_all(settings_path);
        let filename = [settings_path, settings_file].iter().collect::<PathBuf>();

        event!(Level::INFO, filename=?filename, settings=?self, "Storing settings");
        if let Err(err) = fs::write(filename, json) {
            error!("Writing to file failed. Error: {}.", err);
            return false;
        }
        true
    }
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for ConnectionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionType::InProcess => write!(f, "In-Process"),
            ConnectionType::WebSocket(host) => write!(f, "WebSocket {}", host),
            ConnectionType::Test => write!(f, "Test"),
        }
    }
}

#[cfg(test)]
pub(crate) mod settings_tests {
    use crate::actuators::BpActuatorSettings;

    use super::*;
    use tempfile::{tempdir, TempDir};
    use tokio_test::assert_ok;

    #[test]
    fn serialize_deserialize_works() {
        // Arrange
        let mut setting = ClientSettings::new();

        // Act
        setting.device_settings.devices.push(BpActuatorSettings::from_identifier("value"));

        let serialized = serde_json::to_string_pretty(&setting).unwrap();
        let deserialized: ClientSettings = serde_json::from_str(&serialized).unwrap();
        println!("{}", serialized);
        assert_eq!(
            deserialized.device_settings.devices[0].actuator_id,
            setting.device_settings.devices[0].actuator_id
        );
    }

    #[test]
    fn file_existing_returns_parsed_content() {
        // Arrange
        let mut setting = ClientSettings::new();
        setting.device_settings.devices.push(BpActuatorSettings::from_identifier("a"));
        setting.device_settings.devices.push(BpActuatorSettings::from_identifier("b"));
        setting.device_settings.devices.push(BpActuatorSettings::from_identifier("c"));

        let file = "test_config.json";
        let (path, tmp_dir, _tmp_handle) = create_temp_file(file, &serde_json::to_string(&setting).unwrap());

        // Act
        println!("{}", path);
        let settings = ClientSettings::try_read_or_default(&tmp_dir, file);
        assert_eq!(settings.device_settings.devices.len(), 3);
    }

    #[test]
    fn file_not_existing_returns_default() {
        let settings = ClientSettings::try_read_or_default("Path that does not exist", "some.json");
        assert_eq!(settings.device_settings.devices.len(), settings.device_settings.devices.len());
    }

    #[test]
    fn file_unreadable_returns_default() {
        // File
        let (_, tmp_dir, _) = create_temp_file("bogus.json", "Some stuff that is not valid json");

        // Act
        let settings =
            ClientSettings::try_read_or_default(&tmp_dir, "bogus.json");

        // Assert
        assert_eq!(settings.device_settings.devices.len(), settings.device_settings.devices.len());
    }

    #[test]
    fn adds_every_device_only_once() {
        let mut settings = ClientSettings::new();
        settings.device_settings.get_or_create("a");
        settings.device_settings.get_or_create("a");
        assert_eq!(settings.device_settings.devices.len(), 1);
    }

    #[test]
    fn enable_and_disable_devices() {
        let mut settings = ClientSettings::new();
        settings.device_settings.get_or_create("a");
        settings.device_settings.get_or_create("b");
        settings.device_settings.set_enabled("a", true);
        let enabled_devices = settings.device_settings.get_enabled_devices();
        assert_eq!(enabled_devices.len(), 1);
        assert_eq!(enabled_devices[0].actuator_id, "a");

        settings.device_settings.set_enabled("a", false);
        assert_eq!(settings.device_settings.get_enabled_devices().len(), 0);
    }

    #[test]
    fn enable_multiple_devices() {
        let mut settings = ClientSettings::new();
        settings.device_settings.get_or_create("a");
        settings.device_settings.get_or_create("b");
        settings.device_settings.set_enabled("a", true);
        settings.device_settings.set_enabled("b", true);
        assert_eq!(settings.device_settings.get_enabled_devices().len(), 2);
    }

    #[test]
    fn enable_unknown_device() {
        let mut settings = ClientSettings::new();
        settings.device_settings.set_enabled("foobar", true);
        assert_eq!(settings.device_settings.get_enabled_devices()[0].actuator_id, "foobar");
    }

    #[test]
    fn is_enabled_false() {
        let mut settings = ClientSettings::new();
        settings.device_settings.get_or_create("a");
        assert!(!settings.device_settings.get_enabled("a"));
    }

    #[test]
    fn is_enabled_true() {
        let mut settings = ClientSettings::new();
        settings.device_settings.get_or_create("a");
        settings.device_settings.set_enabled("a", true);
        assert!(settings.device_settings.get_enabled("a"));
    }

    #[test]
    fn write_to_temp_file() {
        let mut settings = ClientSettings::new();
        settings.device_settings.get_or_create("foobar");

        // act
        let target_file = "some_target_file.json";
        let (_, tmpdir, tmp_handle) = create_temp_file(target_file, "");
        settings.try_write(&tmpdir, target_file);

        // assert
        let settings2 =
            ClientSettings::try_read_or_default(&tmpdir, target_file);
        assert_eq!(settings2.device_settings.devices[0].actuator_id, "foobar");
        assert_ok!(tmp_handle.close());
    }

    #[test]
    fn set_valid_websocket_endpoint() {
        let mut settings = ClientSettings::new();
        let endpoint = String::from("3.44.33.6:12345");
        settings.connection = ConnectionType::WebSocket(endpoint);
        if let ConnectionType::WebSocket(endpoint) = settings.connection {
            assert_eq!(endpoint, "3.44.33.6:12345")
        } else {
            panic!()
        }
    }

    #[test]
    fn set_valid_websocket_endpoint_hostname() {
        let mut settings = ClientSettings::new();
        let endpoint = String::from("localhost:12345");
        settings.connection = ConnectionType::WebSocket(endpoint);
        if let ConnectionType::WebSocket(endpoint) = settings.connection {
            assert_eq!(endpoint, "localhost:12345")
        } else {
            panic!()
        }
    }

    pub fn create_temp_file(name: &str, content: &str) -> (String, String, TempDir) {
        let tmp_path = tempdir().unwrap();
        assert_ok!(fs::create_dir_all(tmp_path.path().to_str().unwrap()));

        let file_path = tmp_path.path().join(name);
        let path = file_path.to_str().unwrap();
        assert_ok!(fs::write(file_path.clone(), content));

        (path.into(), tmp_path.path().to_str().unwrap().into(), tmp_path)
    }

    pub fn add_temp_file(name: &str, content: &str, tmp_path: &TempDir) {
        assert_ok!(fs::write(tmp_path.path().join(name).clone(), content));
    }
}
