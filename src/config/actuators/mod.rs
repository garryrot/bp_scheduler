use itertools::Itertools;
use linear::{LinearRange, LinearSpeedScaling};
use scalar::ScalarRange;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, instrument};

use buttplug::core::message::ActuatorType;

use crate::{actuator::Actuator, util::trim_lower_str_list};

pub mod linear;
pub mod scalar;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ActuatorSettings(pub Vec<ActuatorConfig>);

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ActuatorConfig {
    pub actuator_config_id: String,
    pub enabled: bool,
    pub body_parts: Vec<String>,
    #[serde(default = "ActuatorLimits::default")]
    pub limits: ActuatorLimits,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub enum ActuatorLimits {
    #[default]
    None,
    Scalar(ScalarRange),
    Linear(LinearRange),
}

impl ActuatorSettings {
    pub fn get_enabled_devices(&self) -> Vec<ActuatorConfig> {
        self.0.iter().filter(|d| d.enabled).cloned().collect()
    }

    pub fn get_or_create(&mut self, actuator_config_id: &str) -> ActuatorConfig {
        let device = self.get_config(actuator_config_id);
        match device {
            Some(setting) => setting,
            None => {
                let mut device = ActuatorConfig::from_identifier(actuator_config_id);
                device.body_parts = vec![ // TODO: Needs to be defined somewhere else
                    "anal".to_owned(),
                    "clitoral".to_owned(),
                    "nipple".to_owned(),
                    "oral".to_owned(),
                    "penis".to_owned(),
                    "vaginal".to_owned()
                ];
                self.update_device(device.clone());
                device
            },
        }
    }

    // unused
    pub fn try_get_limits(&mut self, actuator_config_id: &str) -> ActuatorLimits {
        if let Some(setting) = self.get_config(actuator_config_id) {
            return setting.limits;
        }
        ActuatorLimits::None
    }

    // unused
    pub fn get_or_create_linear(&mut self, actuator_config_id: &str) -> (ActuatorConfig, LinearRange) {
        let mut device = self.get_or_create(actuator_config_id);
        if let ActuatorLimits::Scalar(ref scalar) = device.limits {
            error!("actuator {:?} is scalar but assumed linear... dropping all {:?}", actuator_config_id, scalar)
        }
        if let ActuatorLimits::Linear(ref linear) = device.limits {
            return (device.clone(), linear.clone());
        }
        let default = LinearRange { scaling: LinearSpeedScaling::Parabolic(2), ..Default::default() };
        device.limits = ActuatorLimits::Linear(default.clone());
        self.update_device(device.clone());
        (device, default)
    }

    // unused
    pub fn get_or_create_scalar(&mut self, actuator_id: &str) -> (ActuatorConfig, ScalarRange) {
        let mut device = self.get_or_create(actuator_id);
        if let ActuatorLimits::Linear(ref linear) = device.limits {
            error!("actuator {:?} is linear but assumed scalar... dropping all {:?}", actuator_id, linear)
        }
        if let ActuatorLimits::Scalar(ref scalar) = device.limits {
            return (device.clone(), scalar.clone());
        }
        let default = ScalarRange::default();
        device.limits = ActuatorLimits::Scalar(default.clone());
        self.update_device(device.clone());
        (device, default)
    }

    // unused
    pub fn update_linear<F, R>(&mut self, actuator_config_id: &str, accessor: F) -> R
        where F: FnOnce(&mut LinearRange) -> R
    {
        let (mut settings, mut linear) = self.get_or_create_linear(actuator_config_id);
        let result = accessor(&mut linear);
        settings.limits = ActuatorLimits::Linear(linear);
        self.update_device(settings);
        result
    }

    // unused
    pub fn update_scalar<F, R>(&mut self, actuator_config_id: &str, accessor: F) -> R
        where F: FnOnce(&mut ScalarRange) -> R
    {
        let (mut settings, mut scalar) = self.get_or_create_scalar(actuator_config_id);
        let result = accessor(&mut scalar);
        settings.limits = ActuatorLimits::Scalar(scalar);
        self.update_device(settings);

        result
    }
    
    pub fn update_device(&mut self, setting: ActuatorConfig)
    {
        let insert_pos = self.0.iter().find_position(|x| x.actuator_config_id == setting.actuator_config_id);
        if let Some((pos, _)) = insert_pos {
            self.0[ pos ] = setting;
        } else {
            self.0.push(setting);
        }
    }

    pub fn get_config(&self, actuator_config_id: &str) -> Option<ActuatorConfig> {
         self.0
                .iter()
                .find(|d| d.actuator_config_id == actuator_config_id)
                .cloned()
    }

    #[instrument]
    pub fn set_enabled(&mut self, actuator_config_id: &str, enabled: bool) {
        debug!("set_enabled");
        let mut device =  self.get_or_create(actuator_config_id);
        device.enabled = enabled;
        self.update_device(device)
    }

    #[instrument]
    pub fn set_body_parts(&mut self, actuator_config_id: &str, events: &[&str]) {
        debug!("set_body_parts");
        let mut device = self.get_or_create(actuator_config_id);
        device.body_parts = trim_lower_str_list(events);
        self.update_device(device);
    }

    pub fn get_events(&mut self, actuator_config_id: &str) -> Vec<String> {
        self.get_or_create(actuator_config_id).body_parts
    }

    pub fn get_enabled(&mut self, actuator_config_id: &str) -> bool {
        self.get_or_create(actuator_config_id).enabled
    }
}

impl ActuatorConfig {
    pub fn from_identifier(actuator_id: &str) -> ActuatorConfig {
        ActuatorConfig {
            actuator_config_id: actuator_id.into(),
            enabled: false,
            body_parts: vec![],
            limits: ActuatorLimits::None,
        }
    }
    pub fn from_actuator(actuator: &Actuator) -> ActuatorConfig {
        ActuatorConfig {
            actuator_config_id: actuator.identifier().into(),
            enabled: false,
            body_parts: vec![],
            limits: match actuator.actuator {
                ActuatorType::Vibrate
                | ActuatorType::Rotate
                | ActuatorType::Oscillate
                | ActuatorType::Constrict
                | ActuatorType::Inflate => ActuatorLimits::Scalar(ScalarRange::default()),
                ActuatorType::Position => ActuatorLimits::Linear(LinearRange::default()),
                _ => ActuatorLimits::None,
            },
        }
    }
}