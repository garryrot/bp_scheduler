use std::fmt::{self, Display};
use buttplug::core::message::LogLevel;
use serde::{Deserialize, Serialize};

use super::connection::ConnectionType;

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct InProcessFeatures {
    pub bluetooth: bool,
    pub serial: bool,
    pub xinput: bool
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoggingSettings {
    pub log_level: LogLevel,
}

impl Default for LoggingSettings {
    fn default() -> Self {
        Self { log_level: LogLevel::Debug }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientSettings {
    pub connection: ConnectionType,
    pub in_process_features: InProcessFeatures,
    #[serde(skip)]
    pub pattern_path: String
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            connection: ConnectionType::InProcess,
            pattern_path: "".into(),
            in_process_features: InProcessFeatures {
                bluetooth: true,
                serial: true,
                xinput: true,
            },
        }
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
    use std::fs;

    use crate::{actuators::{ActuatorConfig, ActuatorSettings}, read::read_or_default};

    use super::*;
    use tempfile::{tempdir, TempDir};
    use tokio_test::assert_ok;

    #[test]
    fn file_existing_returns_parsed_content() {
        // Arrange
        let mut setting = ActuatorSettings::default();
        setting.0.push(ActuatorConfig::from_identifier("a"));
        setting.0.push(ActuatorConfig::from_identifier("b"));
        setting.0.push(ActuatorConfig::from_identifier("c"));

        let file = "test_config.json";
        let (path, tmp_dir, _tmp_handle) = create_temp_file(file, &serde_json::to_string(&setting).unwrap());

        // Act
        println!("{}", path);
        
        let settings = read_or_default::<ActuatorSettings>(&tmp_dir, file);
        assert_eq!(settings.0.len(), 3);
    }

    #[test]
    fn file_not_existing_returns_default() {
        let settings = read_or_default::<ActuatorSettings>("Path that does not exist", "some.json");
        assert_eq!(settings.0.len(), settings.0.len());
    }

    #[test]
    fn file_unreadable_returns_default() {
        // File
        let (_, tmp_dir, _) = create_temp_file("bogus.json", "Some stuff that is not valid json");

        // Act
        let settings =
            read_or_default::<ActuatorSettings>(&tmp_dir, "bogus.json");

        // Assert
        assert_eq!(settings.0.len(), settings.0.len());
    }

    #[test]
    fn adds_every_device_only_once() {
        let mut settings = ActuatorSettings::default();
        settings.get_or_create("a");
        settings.get_or_create("a");
        assert_eq!(settings.0.len(), 1);
    }

    #[test]
    fn enable_and_disable_devices() {
        let mut settings = ActuatorSettings::default();
        settings.get_or_create("a");
        settings.get_or_create("b");
        settings.set_enabled("a", true);
        let enabled_devices = settings.get_enabled_devices();
        assert_eq!(enabled_devices.len(), 1);
        assert_eq!(enabled_devices[0].actuator_config_id, "a");

        settings.set_enabled("a", false);
        assert_eq!(settings.get_enabled_devices().len(), 0);
    }

    #[test]
    fn enable_multiple_devices() {
        let mut settings = ActuatorSettings::default();
        settings.get_or_create("a");
        settings.get_or_create("b");
        settings.set_enabled("a", true);
        settings.set_enabled("b", true);
        assert_eq!(settings.get_enabled_devices().len(), 2);
    }

    #[test]
    fn enable_unknown_device() {
        let mut settings = ActuatorSettings::default();
        settings.set_enabled("foobar", true);
        assert_eq!(settings.get_enabled_devices()[0].actuator_config_id, "foobar");
    }

    #[test]
    fn is_enabled_false() {
        let mut settings = ActuatorSettings::default();
        settings.get_or_create("a");
        assert!(!settings.get_enabled("a"));
    }

    #[test]
    fn is_enabled_true() {
        let mut settings = ActuatorSettings::default();
        settings.get_or_create("a");
        settings.set_enabled("a", true);
        assert!(settings.get_enabled("a"));
    }

    #[test]
    fn set_valid_websocket_endpoint() {
        let mut settings = ClientSettings::default();
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
        let mut settings = ClientSettings::default();
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
