use std::{
    fmt::{self, Display},
    sync::Arc,
};
use crossbeam_channel::Receiver;
use itertools::Itertools;
use tracing::debug;

use buttplug::{client::ButtplugClientDevice, core::message::ActuatorType};

use crate::actuator::Actuator;

use super::{connection::TkConnectionEvent, settings::TkSettings};

/// Its actually device status but this makes it easier to housekeep
#[derive(Clone, Debug)]
pub struct ActuatorStatus {
    pub actuator: Arc<Actuator>,
    pub connection_status: TkConnectionStatus
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TkConnectionStatus {
    NotConnected,
    Connected,
    Failed(String),
}

pub struct Status {
    status_events: Receiver<TkConnectionEvent>,
    known_actuators: Vec<String>
}

impl Status {
    pub fn new(receiver: Receiver<TkConnectionEvent>, settings: &TkSettings) -> Self {
        Status {
            status_events: receiver,
            known_actuators: settings
                .device_settings
                .devices
                .iter()
                .map(|x| x.actuator_id.clone())
                .collect(),
        }
    }


    pub fn get_actuator(&mut self, actuator_id: &str, devices: Vec<Arc<ButtplugClientDevice>>) -> Option<Arc<Actuator>> {
        get_actuators(devices)
            .iter()
            .find(|x| x.identifier() == actuator_id)
            .cloned()
    }

    pub fn get_known_actuator_ids(&mut self, devices: Vec<Arc<ButtplugClientDevice>>) -> Vec<String> {
        let known_ids = self.known_actuators.clone();
        get_actuators(devices)
            .iter()
            .map(|x| String::from(x.identifier()))
            .chain(known_ids)
            .unique()
            .collect()
    }
}

impl Display for TkConnectionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            TkConnectionStatus::Failed(err) => write!(f, "{}", err),
            TkConnectionStatus::NotConnected => write!(f, "Not Connected"),
            TkConnectionStatus::Connected => write!(f, "Connected"),
        }
    }
}

pub fn get_actuators(devices: Vec<Arc<ButtplugClientDevice>>) -> Vec<Arc<Actuator>> {
    let mut actuators = vec![];
    for device in devices {
        if let Some(scalar_cmd) = device.message_attributes().scalar_cmd() {
            for (idx, scalar_cmd) in scalar_cmd.iter().enumerate() {
                actuators.push(Actuator::new(&device, *scalar_cmd.actuator_type(), idx))
            }
        }
        if let Some(linear_cmd) = device.message_attributes().linear_cmd() {
            for (idx, _) in linear_cmd.iter().enumerate() {
                actuators.push(Actuator::new(&device, ActuatorType::Position, idx));
            }
        }
        if let Some(rotate_cmd) = device.message_attributes().rotate_cmd() {
            for (idx, _) in rotate_cmd.iter().enumerate() {
                actuators.push(Actuator::new(&device, ActuatorType::Rotate, idx))
            }
        }
    }
    actuators.into_iter().map(Arc::new).collect()
}
