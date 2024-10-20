use std::sync::Arc;

use buttplug::client::ButtplugClientDevice;
use itertools::Itertools;

use super::{actuator::Actuators, config::client::ClientSettings};

pub fn get_known_actuator_ids(devices: Vec<Arc<ButtplugClientDevice>>, settings: &ClientSettings) -> Vec<String> {
    let known_actuators : Vec<String> = settings
            .device_settings
            .devices
            .iter()
            .map(|x| x.actuator_id.clone())
            .collect();

    let known_ids = known_actuators.clone();
    devices.flatten_actuators()
        .iter()
        .map(|x| String::from(x.identifier()))
        .chain(known_ids)
        .unique()
        .collect()
}
