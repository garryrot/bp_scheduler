use std::{sync::Arc, time::Duration};

use buttplug::core::message::ActuatorType;
use cxx::{CxxString, CxxVector};
use tracing::{debug, error};

use crate::{actuator::Actuator, devices::{sanitize_name_list, BpDeviceSettings}};

pub fn parse_csv(input: &str) -> Vec<String> {
    let mut list = vec![];
    for part in input.split(',') {
        if !part.is_empty() {
            list.push(part.trim().to_lowercase());
        }
    }
    list
}

pub fn get_duration_from_secs(secs: f32) -> Duration {
    if secs > 0.0 {
        Duration::from_millis((secs * 1000.0) as u64)
    } else {
        Duration::MAX
    }
}

pub fn read_input_string(list: &CxxVector<CxxString>) -> Vec<String> {
    // automatically discards any empty strings to account for papyrus
    // inability to do dynamic array sizes
    list.iter()
        .filter(|d| !d.is_empty())
        .map(|d| d.to_string_lossy().into_owned())
        .collect()
}

pub fn read_scalar_actuator(actuator: &str) -> ActuatorType {
    let lower = actuator.to_ascii_lowercase();
    match lower.as_str() {
        "constrict" => ActuatorType::Constrict,
        "inflate" => ActuatorType::Inflate,
        "oscillate" => ActuatorType::Oscillate,
        "vibrate" => ActuatorType::Vibrate,
        _ => {
            error!("unknown actuator {:?}", lower);
            ActuatorType::Vibrate
        }
    }
}

pub struct TkParams {}

impl TkParams {
    pub fn get_enabled_and_selected_devices(
        actuators: &[Arc<Actuator>],
        input_body_parts: &[String],
        actuator_types: &[ActuatorType],
        device_settings: &[BpDeviceSettings]
        ) -> Vec<Arc<Actuator>> {
        let body_parts = sanitize_name_list(input_body_parts);
        let selected_settings = device_settings.iter().filter( |setting| { 
            if ! setting.enabled {
                return false;
            }
            if body_parts.is_empty() {
                return true;
            }
            setting.events.iter().any( |y| body_parts.contains(y) )
        }).cloned().collect::<Vec<BpDeviceSettings>>();

        let selected = selected_settings.iter().map(|x| x.actuator_id.clone()).collect::<Vec<String>>();
        
        let used: Vec<Arc<Actuator>> = actuators
                .iter()
                .filter( |x| actuator_types.iter().any(|y| y == &x.actuator) )
                .filter( |x| selected.contains( & x.identifier().to_owned() ) )
                .cloned()
                .collect::<Vec<Arc<Actuator>>>();

        debug!("connected: {:?}", actuators.iter().map( |x| x.identifier() ).collect::<Vec<&str>>());
        debug!(?used);
        used
    }

}
