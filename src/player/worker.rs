use buttplug::client::{LinearCommand, ButtplugClientError};
use std::{collections::HashMap, sync::Arc};

use tokio::{runtime::Handle, sync::mpsc::UnboundedReceiver};
use tracing::{error, info, trace};
use tokio::sync::mpsc::UnboundedSender;

use crate::{actuator::Actuator, speed::Speed};

use super::access::DeviceAccess;

pub type WorkerResult<T = ()> = Result<T, WorkerError>;

/// Process the queue of all device actions from all player threads
///
/// This was introduced so that that the housekeeping and the decision which
/// thread gets priority on a device is always done in the same thread and
/// its not necessary to introduce Mutex/etc to handle multithreaded access
pub struct ButtplugWorker {
    pub task_receiver: UnboundedReceiver<WorkerTask>,
}

#[derive(Clone, Debug)]
pub enum WorkerTask {
    Start(Arc<Actuator>, Speed, bool, i32),
    Update(Arc<Actuator>, Speed, bool, i32),
    End(
        Arc<Actuator>,
        bool,
        i32,
        UnboundedSender<WorkerResult>,
    ),
    Move(
        Arc<Actuator>,
        f64,
        u32,
        bool,
        UnboundedSender<WorkerResult>,
    ),
    StopAll, // global but required for resetting device state
}

impl ButtplugWorker {
    pub async fn run_worker_thread(&mut self) {
        let mut device_access = DeviceAccess::default();
        loop {
            if let Some(next_action) = self.task_receiver.recv().await {
                trace!("worker exec action {:?}", next_action);
                match next_action {
                    WorkerTask::Start(actuator, speed, is_pattern, handle) => {
                        device_access
                            .start_scalar(actuator, speed, is_pattern, handle)
                            .await;
                    }
                    WorkerTask::Update(actuator, speed, is_pattern, handle) => {
                        device_access.update_scalar(actuator, speed, is_pattern, handle).await;
                    }
                    WorkerTask::End(actuator, is_pattern, handle, result_sender) => {
                        let result = device_access
                            .stop_scalar(actuator.clone(), is_pattern, handle)
                            .await;
                        if let Err(err) = result_sender.send(get_worker_result(result, actuator)) {
                            error!("failed sending scalar result {:?}", err)
                        }
                    }
                    WorkerTask::Move(actuator, position, duration_ms, finish, result_sender) => {
                        let cmd = LinearCommand::LinearMap(HashMap::from([(
                            actuator.index_in_device,
                            (duration_ms, position),
                        )]));
                        Handle::current().spawn(async move {
                            let result = actuator.device.linear(&cmd).await;
                            if finish {
                                if let Err(err) = result_sender.send(get_worker_result(result, actuator)) {
                                    error!("failed sending linear result {:?}", err)
                                }
                            }
                        });
                    }
                    WorkerTask::StopAll => {
                        device_access.clear_all();
                        info!("stop all action");
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct WorkerError {
    pub bp_error: ButtplugClientError,
    pub actuator: Arc<Actuator>
}

fn get_worker_result<T>(bp_result: Result<T, ButtplugClientError>, actuator: Arc<Actuator>) -> Result<T, WorkerError> {
    match bp_result {
        Ok(t) => Ok(t),
        Err(err) => Err(WorkerError { 
            bp_error: err, 
            actuator: actuator.clone() 
        }),
    }
}