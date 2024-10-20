use buttplug::client::ButtplugClientDevice;
use buttplug::core::message::ActuatorType;
use std::{
    fmt::{self, Display}, ops::Deref, sync::Arc
};

use crate::actuators::{ActuatorConfig, ActuatorSettings};

#[derive(Clone)]
pub struct Actuator {
    pub device: Arc<ButtplugClientDevice>,
    pub actuator: ActuatorType,
    pub index_in_device: u32,
    pub config: Option<ActuatorConfig>,
    identifier: String,
}

impl Actuator {
    pub fn new(
        device: &Arc<ButtplugClientDevice>,
        actuator: ActuatorType,
        index_in_device: usize,
    ) -> Self {
        let identifier = Actuator::get_identifier(device, actuator, index_in_device);
        Actuator {
            device: device.clone(),
            actuator,
            index_in_device: index_in_device as u32,
            identifier,
            config: None
        }
    }

    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    fn get_identifier(
        device: &Arc<ButtplugClientDevice>,
        actuator: ActuatorType,
        index_in_device: usize,
    ) -> String {
        if index_in_device > 0 {
            return format!("{} ({} #{})", device.name(), actuator, index_in_device);
        }
        format!("{} ({})", device.name(), actuator)
    }

    pub fn get_config(&self) -> ActuatorConfig {
        match &self.config {
            Some(cfg) => cfg.clone(),
            None => ActuatorConfig::default() // panic!("actuator config not loaded: {:?}", self), // TODO Return default
        }
    }

}

impl Display for Actuator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.identifier)
    }
}

impl fmt::Debug for Actuator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Actuator({})", self.identifier)
    }
}

pub trait Actuators {
    fn flatten_actuators(&self) -> Vec<Arc<Actuator>>;
}

impl Actuators for Vec<Arc<ButtplugClientDevice>> {
    fn flatten_actuators(&self) -> Vec<Arc<Actuator>> {
        self.iter().flat_map(|x| x.flatten_actuators()).collect()
    }
}

impl Actuators for &Arc<ButtplugClientDevice> {
    fn flatten_actuators(&self) -> Vec<Arc<Actuator>> {
        let mut actuators = vec![];
        if let Some(scalar_cmd) = self.message_attributes().scalar_cmd() {
            for (idx, scalar_cmd) in scalar_cmd.iter().enumerate() {
                actuators.push(Actuator::new(self, *scalar_cmd.actuator_type(), idx))
            }
        }
        if let Some(linear_cmd) = self.message_attributes().linear_cmd() {
            for (idx, _) in linear_cmd.iter().enumerate() {
                actuators.push(Actuator::new(self, ActuatorType::Position, idx));
            }
        }
        if let Some(rotate_cmd) = self.message_attributes().rotate_cmd() {
            for (idx, _) in rotate_cmd.iter().enumerate() {
                actuators.push(Actuator::new(self, ActuatorType::Rotate, idx))
            }
        }
        actuators.into_iter().map(Arc::new).collect()
    }
}

pub trait ActuatorConfigLoader {
    fn load_config(self, config: &mut ActuatorSettings) -> Vec<Arc<Actuator>>;
}

impl ActuatorConfigLoader for Vec<Arc<Actuator>> {
    fn load_config(self, config: &mut ActuatorSettings) -> Vec<Arc<Actuator>> {
        self.into_iter().map(|act| { 
            Arc::new(Actuator {
                config: Some(config.get_or_create(&act.identifier)),
                .. act.deref().clone()
            })
        } ).collect()
    }
}