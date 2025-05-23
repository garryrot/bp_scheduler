use buttplug::client::{ButtplugClientError, ScalarCommand};
use std::collections::HashMap;

use std::sync::Arc;
use tracing::{error, trace, instrument};

use crate::{actuator::Actuator, speed::Speed};

/// Stores information about concurrent accesses to a buttplug actuator
/// to calculate the actual vibration speed or linear movement
pub struct DeviceEntry {
    /// The amount of tasks that currently access this device,
    pub task_count: usize,
    /// Priority calculation works like a stack with the top of the stack
    /// task being the used vibration speed
    pub linear_tasks: Vec<(i32, Speed)>,
}

#[derive(Default, Debug, PartialEq, Eq, Hash)]
struct ActuatorIndex {
    device_index: u32,
    actuator_index: u32
}

#[derive(Default)]
pub struct DeviceAccess {
    device_actions: HashMap<ActuatorIndex, DeviceEntry>,
}

impl DeviceAccess {
    pub async fn start_scalar(
        &mut self,
        actuator: Arc<Actuator>,
        speed: Speed,
        is_pattern: bool,
        handle: i32,
    ) {
        trace!( handle, ?speed, "start scalar");
        self.device_actions
            .entry(actuator.clone().into())
            .and_modify(|entry| {
                entry.task_count += 1;
                if ! is_pattern {
                    entry.linear_tasks.push((handle, speed))
                }
            })
            .or_insert_with(|| DeviceEntry {
                task_count: 1,
                linear_tasks: if is_pattern {
                    vec![]
                } else {
                    vec![(handle, speed)]
                },
            });
        let _ = self.set_scalar(actuator, speed).await;
    }

    #[instrument(skip(self))]
    pub async fn stop_scalar(
        &mut self,
        actuator: Arc<Actuator>,
        is_pattern: bool,
        handle: i32,
    ) -> Result<(), ButtplugClientError> {
        trace!("stop scalar");
        if let Some(mut entry) = self.device_actions.remove(&actuator.clone().into()) {
            if ! is_pattern {
                entry.linear_tasks.retain(|t| t.0 != handle);
            }
            let mut count = entry.task_count;
            count = count.saturating_sub(1);
            entry.task_count = count;
            self.device_actions.insert(actuator.clone().into(), entry);
            if count == 0 {
                // nothing else is controlling the device, stop it
                return self.set_scalar(actuator, Speed::min()).await;
            } else if let Some(last_speed) = self.calculate_speed(actuator.clone()) {
                let _ = self.set_scalar(actuator, last_speed).await;
            }
        }
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn update_scalar(&mut self, actuator: Arc<Actuator>, new_speed: Speed, is_pattern: bool, handle: i32) {
        trace!(handle, ?new_speed, "update scalar");
        if ! is_pattern {
            self.device_actions.entry(actuator.clone().into()).and_modify(|entry| {
                entry.linear_tasks = entry.linear_tasks.iter().map(|t| {
                    if t.0 == handle {
                        return (handle, new_speed);
                    }
                    *t
                }).collect()
            });
        }
        let speed = self.calculate_speed(actuator.clone()).unwrap_or(new_speed);
        trace!("updating {} speed to {}", actuator, speed);
        let _ = self.set_scalar(actuator, speed).await;
    }

    #[instrument(skip(self))]
    async fn set_scalar(
        &self,
        actuator: Arc<Actuator>,
        speed: Speed,
    ) -> Result<(), ButtplugClientError> {
        let cmd = ScalarCommand::ScalarMap(HashMap::from([(
            actuator.index_in_device,
            (speed.as_float(), actuator.actuator),
        )]));

        if let Err(err) = actuator.device.scalar(&cmd).await {
            error!("failed to set scalar speed {:?}", err);
            return Err(err);
        }
        Ok(())
    }

    fn calculate_speed(&self, actuator: Arc<Actuator>) -> Option<Speed> {
        // concurrency-strategy: always use the highest existing value
        if let Some(entry) = self.device_actions.get(&actuator.into()) {
            // let mut sorted: Vec<(i32, Speed)> = entry.linear_tasks.clone();
            if let Some(percentage) = entry.linear_tasks.iter().map(|x| x.1.value).max() {
                return Some(Speed::new(percentage.into()));
            }
        }
        None
    }

    pub fn clear_all(&mut self) {
        self.device_actions.clear();
    }
}

impl From<Arc<Actuator>> for ActuatorIndex {
    fn from(value: Arc<Actuator>) -> Self {
        ActuatorIndex {
            device_index: value.device.index(),
            actuator_index: value.index_in_device,
        } 
    }
}
