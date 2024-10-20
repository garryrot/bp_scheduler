use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, instrument};

use buttplug::core::message::ActuatorType;

use crate::{actuator::Actuator, util::trim_lower_str_list};

use super::{
    linear::{LinearRange, LinearSpeedScaling}, 
    scalar::ScalarRange, ActuatorLimits
};

/// actuator sepcific settings
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ActuatorSettings {
    pub devices: Vec<ActuatorConfig>
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ActuatorConfig {
    pub actuator_id: String,
    pub enabled: bool,
    pub body_parts: Vec<String>,
    #[serde(default = "ActuatorLimits::default")]
    pub limits: ActuatorLimits,
}

impl ActuatorSettings {
    pub fn get_enabled_devices(&self) -> Vec<ActuatorConfig> {
        self.devices.iter().filter(|d| d.enabled).cloned().collect()
    }

    pub fn get_or_create(&mut self, actuator_id: &str) -> ActuatorConfig {
        let device = self.get_device(actuator_id);
        match device {
            Some(setting) => setting,
            None => {
                let device = ActuatorConfig::from_identifier(actuator_id);
                self.update_device(device.clone());
                device
            },
        }
    }

    pub fn try_get_limits(&mut self, actuator_id: &str) -> ActuatorLimits {
        if let Some(setting) = self.get_device(actuator_id) {
            return setting.limits;
        }
        ActuatorLimits::None
    }

    pub fn get_or_create_linear(&mut self, actuator_id: &str) -> (ActuatorConfig, LinearRange) {
        let mut device = self.get_or_create(actuator_id);
        if let ActuatorLimits::Scalar(ref scalar) = device.limits {
            error!("actuator {:?} is scalar but assumed linear... dropping all {:?}", actuator_id, scalar)
        }
        if let ActuatorLimits::Linear(ref linear) = device.limits {
            return (device.clone(), linear.clone());
        }
        let default = LinearRange { scaling: LinearSpeedScaling::Parabolic(2), ..Default::default() };
        device.limits = ActuatorLimits::Linear(default.clone());
        self.update_device(device.clone());
        (device, default)
    }

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

    pub fn update_linear<F, R>(&mut self, actuator_id: &str, accessor: F) -> R
        where F: FnOnce(&mut LinearRange) -> R
    {
        let (mut settings, mut linear) = self.get_or_create_linear(actuator_id);
        let result = accessor(&mut linear);
        settings.limits = ActuatorLimits::Linear(linear);
        self.update_device(settings);
        result
    }

    pub fn update_scalar<F, R>(&mut self, actuator_id: &str, accessor: F) -> R
        where F: FnOnce(&mut ScalarRange) -> R
    {
        let (mut settings, mut scalar) = self.get_or_create_scalar(actuator_id);
        let result = accessor(&mut scalar);
        settings.limits = ActuatorLimits::Scalar(scalar);
        self.update_device(settings);

        result
    }
   
    pub fn update_device(&mut self, setting: ActuatorConfig)
    {
        let insert_pos = self.devices.iter().find_position(|x| x.actuator_id == setting.actuator_id);
        if let Some((pos, _)) = insert_pos {
            self.devices[ pos ] = setting;
        } else {
            self.devices.push(setting);
        }
    }

    pub fn get_device(&self, actuator_id: &str) -> Option<ActuatorConfig> {
         self.devices
                .iter()
                .find(|d| d.actuator_id == actuator_id)
                .cloned()
    }

    #[instrument]
    pub fn set_enabled(&mut self, actuator_id: &str, enabled: bool) {
        debug!("set_enabled");

        let mut device =  self.get_or_create(actuator_id);
        device.enabled = enabled;
        self.update_device(device)
    }

    #[instrument]
    pub fn set_events(&mut self, actuator_id: &str, events: &[String]) {
        debug!("set_events");

        let mut device = self.get_or_create(actuator_id);
        device.body_parts = trim_lower_str_list(events);
        self.update_device(device);
    }

    pub fn get_events(&mut self, actuator_id: &str) -> Vec<String> {
        self.get_or_create(actuator_id).body_parts
    }

    pub fn get_enabled(&mut self, actuator_id: &str) -> bool {
        self.get_or_create(actuator_id).enabled
    }
}


impl ActuatorConfig {
    pub fn from_identifier(actuator_id: &str) -> ActuatorConfig {
        ActuatorConfig {
            actuator_id: actuator_id.into(),
            enabled: false,
            body_parts: vec![],
            limits: ActuatorLimits::None,
        }
    }
    pub fn from_actuator(actuator: &Actuator) -> ActuatorConfig {
        ActuatorConfig {
            actuator_id: actuator.identifier().into(),
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