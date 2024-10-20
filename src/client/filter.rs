use std::sync::Arc;

use buttplug::{client::ButtplugClientDevice, core::message::ActuatorType};

use crate::{actuator::{Actuator, Actuators}, actuators::BpActuatorSettings};

use super::actuators::BpSettings;

pub struct Filter {
    settings: BpSettings,
    actuators: Vec<Arc<Actuator>>
}

impl Filter {
    pub fn new(settings: BpSettings, devices: &[Arc<ButtplugClientDevice>]) -> Self {
        Filter {
            settings,
            actuators: devices
                .iter()
                .filter(|x| x.connected())
                .cloned()
                .collect::<Vec<Arc<ButtplugClientDevice>>>()
                .flatten_actuators(),
        }
    }

    pub fn connected(mut self) -> Self {
        self.actuators.retain(|x| x.device.connected());
        self
    }

    pub fn enabled(mut self) -> Self {
        self.actuators.retain(|x| x.get_settings(&mut self.settings).enabled);
        self
    }

    pub fn with_actuator_types(mut self, actuator_types: &[ActuatorType]) -> Self {
        self.actuators.retain(|x| actuator_types.contains(&x.actuator) );
        self
    }

    pub fn with_body_parts(mut self, body_parts: &[String]) -> Self {
        if !body_parts.is_empty() {
            self.actuators.retain(|x| {
                x.get_settings(&mut self.settings).body_parts.iter().any( |x| body_parts.contains(x) )
            });
        }
        self
    }

    pub fn result(self) -> (BpSettings, Vec<Arc<Actuator>>) {
        (self.settings, self.actuators)
    }
}

impl Actuator {
    pub fn get_settings(&self, settings: &mut BpSettings) -> BpActuatorSettings {
        settings.get_or_create(self.identifier())
    }
}