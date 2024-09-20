use std::sync::Arc;
use itertools::Itertools;

use buttplug::{client::ButtplugClientDevice, core::message::ActuatorType};

use crate::actuator::Actuator;

use super::settings::TkSettings;

pub fn get_known_actuator_ids(devices: Vec<Arc<ButtplugClientDevice>>, settings: &TkSettings) -> Vec<String> {
    let known_actuators : Vec<String> = settings
            .device_settings
            .devices
            .iter()
            .map(|x| x.actuator_id.clone())
            .collect();

    let known_ids = known_actuators.clone();
    get_actuators(devices)
        .iter()
        .map(|x| String::from(x.identifier()))
        .chain(known_ids)
        .unique()
        .collect()
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
