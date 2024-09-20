use buttplug::client::ButtplugClientDevice;
use buttplug::core::message::ActuatorType;
use std::{
    fmt::{self, Display},
    sync::Arc,
};

#[derive(Clone)]
pub struct Actuator {
    pub device: Arc<ButtplugClientDevice>,
    pub actuator: ActuatorType,
    pub index_in_device: u32,
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
