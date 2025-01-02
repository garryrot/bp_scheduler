use std::sync::Arc;

use buttplug::{client::ButtplugClientDevice, core::message::ActuatorType};
use tracing::{debug, error};

use crate::{actions::Selector, actuator::{Actuator, ActuatorConfigLoader, Actuators}};

use super::actuators::ActuatorSettings;

pub struct Filter {
    settings: ActuatorSettings,
    actuators: Vec<Arc<Actuator>>
}

impl Filter {
    pub fn new(settings: ActuatorSettings, devices: &[Arc<ButtplugClientDevice>]) -> Self {
        let actuators = devices
            .iter()
            .filter(|x| x.connected())
            .cloned()
            .collect::<Vec<Arc<ButtplugClientDevice>>>()
            .flatten_actuators();

        debug!(?actuators, "filtering");
        Filter {
            settings,
            actuators
        }
    }

    pub fn from_actuators(settings: ActuatorSettings, actuators: Vec<Arc<Actuator>>) -> Self {
        Filter {
            settings,
            actuators
        }
    }

    pub fn connected(mut self) -> Self {
        self.actuators.retain(|x: &Arc<Actuator>| x.device.connected());
        self
    }

    pub fn load_config(mut self, settings: &mut ActuatorSettings) -> Self {
        self.actuators = self.actuators.load_config(settings);
        self
    }

    pub fn enabled(mut self) -> Self {
        self.actuators.retain(|x| {
            self.settings.get_or_create(x.identifier())
        }.enabled);
        self
    }

    pub fn with_actuator_types(mut self, actuator_types: &[ActuatorType]) -> Self {
        self.actuators.retain(|x| actuator_types.contains(&x.actuator) );
        self
    }

    pub fn with_selector(mut self, selector: &Selector) -> Self {
        self.actuators.retain(|x| {
            if let Some(c) = &x.config {
                return selector.matches(&c.body_parts)
            }
            error!("settings not initialised");
            false
        });
        self
    }

    pub fn result(self) -> (ActuatorSettings, Vec<Arc<Actuator>>) {
        debug!(?self.actuators, "result");
        (self.settings, self.actuators)
    }
}