use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, instrument};

use buttplug::core::message::ActuatorType;

use crate::{actuator::Actuator, util::trim_lower_str_list};

use super::{
    linear::{LinearRange, LinearSpeedScaling}, 
    scalar::ScalarRange, ActuatorSettings
};

/// actuator sepcific settings
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BpSettings {
    pub devices: Vec<BpActuatorSettings>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BpActuatorSettings {
    pub actuator_id: String,

    /// if the actuator should be used
    pub enabled: bool,

    /// body parts associated with this actuator
    pub body_parts: Vec<String>,

    #[serde(default = "ActuatorSettings::default")]
    pub actuator_settings: ActuatorSettings,
}

impl BpSettings {
    pub fn get_enabled_devices(&self) -> Vec<BpActuatorSettings> {
        self.devices.iter().filter(|d| d.enabled).cloned().collect()
    }

    pub fn get_or_create(&mut self, actuator_id: &str) -> BpActuatorSettings {
        let device = self.get_device(actuator_id);
        match device {
            Some(setting) => setting,
            None => {
                let device = BpActuatorSettings::from_identifier(actuator_id);
                self.update_device(device.clone());
                device
            },
        }
    }

    pub fn try_get_actuator_settings(&mut self, actuator_id: &str) -> ActuatorSettings {
        if let Some(setting) = self.get_device(actuator_id) {
            return setting.actuator_settings;
        }
        ActuatorSettings::None
    }

    pub fn get_or_create_linear(&mut self, actuator_id: &str) -> (BpActuatorSettings, LinearRange) {
        let mut device = self.get_or_create(actuator_id);
        if let ActuatorSettings::Scalar(ref scalar) = device.actuator_settings {
            error!("actuator {:?} is scalar but assumed linear... dropping all {:?}", actuator_id, scalar)
        }
        if let ActuatorSettings::Linear(ref linear) = device.actuator_settings {
            return (device.clone(), linear.clone());
        }
        let default = LinearRange { scaling: LinearSpeedScaling::Parabolic(2), ..Default::default() };
        device.actuator_settings = ActuatorSettings::Linear(default.clone());
        self.update_device(device.clone());
        (device, default)
    }

    pub fn get_or_create_scalar(&mut self, actuator_id: &str) -> (BpActuatorSettings, ScalarRange) {
        let mut device = self.get_or_create(actuator_id);
        if let ActuatorSettings::Linear(ref linear) = device.actuator_settings {
            error!("actuator {:?} is linear but assumed scalar... dropping all {:?}", actuator_id, linear)
        }
        if let ActuatorSettings::Scalar(ref scalar) = device.actuator_settings {
            return (device.clone(), scalar.clone());
        }
        let default = ScalarRange::default();
        device.actuator_settings = ActuatorSettings::Scalar(default.clone());
        self.update_device(device.clone());
        (device, default)
    }

    pub fn update_linear<F, R>(&mut self, actuator_id: &str, accessor: F) -> R
        where F: FnOnce(&mut LinearRange) -> R
    {
        let (mut settings, mut linear) = self.get_or_create_linear(actuator_id);
        let result = accessor(&mut linear);
        settings.actuator_settings = ActuatorSettings::Linear(linear);
        self.update_device(settings);
        result
    }

    pub fn update_scalar<F, R>(&mut self, actuator_id: &str, accessor: F) -> R
        where F: FnOnce(&mut ScalarRange) -> R
    {
        let (mut settings, mut scalar) = self.get_or_create_scalar(actuator_id);
        let result = accessor(&mut scalar);
        settings.actuator_settings = ActuatorSettings::Scalar(scalar);
        self.update_device(settings);

        result
    }
   
    pub fn update_device(&mut self, setting: BpActuatorSettings)
    {
        let insert_pos = self.devices.iter().find_position(|x| x.actuator_id == setting.actuator_id);
        if let Some((pos, _)) = insert_pos {
            self.devices[ pos ] = setting;
        } else {
            self.devices.push(setting);
        }
    }

    pub fn get_device(&self, actuator_id: &str) -> Option<BpActuatorSettings> {
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


impl BpActuatorSettings {
    pub fn from_identifier(actuator_id: &str) -> BpActuatorSettings {
        BpActuatorSettings {
            actuator_id: actuator_id.into(),
            enabled: false,
            body_parts: vec![],
            actuator_settings: ActuatorSettings::None,
        }
    }
    pub fn from_actuator(actuator: &Actuator) -> BpActuatorSettings {
        BpActuatorSettings {
            actuator_id: actuator.identifier().into(),
            enabled: false,
            body_parts: vec![],
            actuator_settings: match actuator.actuator {
                ActuatorType::Vibrate
                | ActuatorType::Rotate
                | ActuatorType::Oscillate
                | ActuatorType::Constrict
                | ActuatorType::Inflate => ActuatorSettings::Scalar(ScalarRange::default()),
                ActuatorType::Position => ActuatorSettings::Linear(LinearRange::default()),
                _ => ActuatorSettings::None,
            },
        }
    }
}