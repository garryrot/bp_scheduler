use derive_new::new;
use funscript::FScript;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use worker::{WorkerResult, WorkerTask};

use std::{
    fmt,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    time::{sleep, Instant},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace};

use crate::{
    actuator::Actuator, actuators::{linear::{LinearRange, LinearSpeedScaling}, ActuatorLimits}, cancellable_wait, speed::Speed
};

pub mod access;
pub mod worker;

#[derive(Debug)]
pub enum Perc {
    Constant(Speed),
    Global(Arc<AtomicI64>),
}

/// Pattern executor that can be passed from the schedulers main-thread to a sub-thread
#[derive(new)]
pub struct PatternPlayer {
    pub handle: i32,
    pub actuators: Vec<Arc<Actuator>>,
    result_sender: UnboundedSender<WorkerResult>,
    result_receiver: UnboundedReceiver<WorkerResult>,
    update_receiver: UnboundedReceiver<Speed>,
    cancellation_token: CancellationToken,
    worker_task_sender: UnboundedSender<WorkerTask>,
    scalar_resolution_ms: i32,
}

impl PatternPlayer {
    pub async fn play_linear_stroke(
        mut self,
        duration: Duration,
        speed: Speed,
        settings: LinearRange,
    ) -> WorkerResult {
        info!(?duration, "playing linear stroke");
        let waiter = self.stop_after(duration);
        let mut result = Ok(());
        let mut current_speed = speed;
        while !self.external_cancel() {
            self.try_update(&mut current_speed);
            result = self.do_stroke(true, current_speed, &settings).await;
            if self.external_cancel() {
                break;
            }
            self.try_update(&mut current_speed);
            result = self.do_stroke(false, current_speed, &settings).await;
        }
        waiter.abort();
        result
    }

    /// Executes the linear 'fscript' for 'duration' and consumes the player
    pub async fn play_linear(mut self, duration: Duration, fscript: FScript) -> WorkerResult {
        info!(?duration, "playing linear");
        let mut last_result = Ok(());
        if fscript.actions.is_empty() || fscript.actions.iter().all(|x| x.at == 0) {
            return last_result;
        }
        let waiter = self.stop_after(duration);
        while !self.external_cancel() {
            let started = Instant::now();
            for point in fscript.actions.iter() {
                let point_as_float = Speed::from_fs(point).as_float();
                if let Some(waiting_time) =
                    Duration::from_millis(point.at as u64).checked_sub(started.elapsed())
                {
                    let token = &self.cancellation_token.clone();
                    if let Some(result) = tokio::select! {
                        _ = token.cancelled() => { None }
                        result = async {
                            self.do_linear(point_as_float, waiting_time.as_millis() as u32).await
                        } => {
                            Some(result)
                        }
                    } {
                        last_result = result;
                    } else {
                        break;
                    }
                }
            }
        }
        waiter.abort();
        last_result
    }

    /// Executes the scalar 'fscript' for 'duration' and consumes the player
    pub async fn play_scalar_pattern(
        mut self,
        duration: Duration,
        fscript: FScript,
        speed: Speed,
    ) -> WorkerResult {
        if fscript.actions.is_empty() || fscript.actions.iter().all(|x| x.at == 0) {
            return Ok(());
        }
        info!(?duration, ?speed, "playing scalar pattern");
        let waiter = self.stop_after(duration);
        let action_len = fscript.actions.len();
        let mut started = false;
        let mut loop_started = Instant::now();
        let mut i: usize = 0;
        let mut current_speed = speed;
        loop {
            let mut j = 1;
            while j + i < action_len - 1
                && (fscript.actions[i + j].at - fscript.actions[i].at) < self.scalar_resolution_ms
            {
                j += 1;
            }
            let current = &fscript.actions[i % action_len];
            let next = &fscript.actions[(i + j) % action_len];
            if let Ok(update) = self.update_receiver.try_recv() {
                current_speed = update;
            }

            let speed = Speed::from_fs(current).multiply(&current_speed);
            if !started {
                self.do_scalar(speed, true);
                started = true;
            } else {
                self.do_update(speed, true);
            }
            if let Some(waiting_time) =
                Duration::from_millis(next.at as u64).checked_sub(loop_started.elapsed())
            {
                debug!(?speed, ?waiting_time, "vibrating");
                if !(cancellable_wait(waiting_time, &self.cancellation_token).await) {
                    debug!("scalar pattern cancelled");
                    break;
                }
            }
            i += j;
            if (i % action_len) == 0 {
                loop_started = Instant::now();
            }
        }
        waiter.abort();
        let result = self.do_stop(true).await;
        result
    }

    /// Executes a constant movement with 'speed' for 'duration' and consumes the player
    pub async fn play_scalar(mut self, duration: Duration, speed: Speed) -> WorkerResult {
        info!(?duration, ?speed, "playing scalar");
        let waiter = self.stop_after(duration);
        self.do_scalar(speed, false);
        loop {
            tokio::select! {
                _ = self.cancellation_token.cancelled() => {
                    break;
                }
                update = self.update_receiver.recv() => {
                    if let Some(speed) = update {
                        self.do_update(speed, false);
                    }
                }
            };
        }
        waiter.abort();
        let result = self.do_stop(false).await;
        result
    }

    /// Executes a constant movement with 'percentage' updating every 200ms
    /// for 'duration' and consumes the player
    pub async fn play_scalar_var(
        self,
        duration: Duration,
        variable: Arc<AtomicI64>,
    ) -> WorkerResult {
        info!(?duration, "play scalar variable");
        let waiter = self.stop_after(duration);
        let mut last_var = variable.load(Ordering::Relaxed);
        debug!(?last_var, self.handle, "var initialized");
        self.do_scalar(Speed::new(last_var), false);
        loop {
            tokio::select! {
                _ = self.cancellation_token.cancelled() => {
                    break;
                }
                _ = sleep(Duration::from_millis(200)) => {
                    let var = variable.load(Ordering::Relaxed);
                    if var != last_var {
                        debug!(?var, self.handle, "var updated");
                        self.do_update(Speed::new(var), false);
                        last_var = var;
                    }
                }
            };
        }
        waiter.abort();
        let result = self.do_stop(false).await;
        result
    }

    fn do_update(&self, speed: Speed, is_pattern: bool) {
        for actuator in &self.actuators {
            trace!( actuator=actuator.identifier(), ?actuator.config, "do_update {} {:?}", speed, actuator);
            self.worker_task_sender
                .send(WorkerTask::Update(
                    actuator.clone(),
                    apply_scalar_settings(speed, &actuator.get_config().limits),
                    is_pattern,
                    self.handle,
                ))
                .unwrap_or_else(|err| error!("queue err {:?}", err));
        }
    }

    fn do_scalar(&self, speed: Speed, is_pattern: bool) {
        for actuator in &self.actuators {
            trace!( actuator=actuator.identifier(), ?actuator.config, "do_scalar");
            self.worker_task_sender
                .send(WorkerTask::Start(
                    actuator.clone(),
                    apply_scalar_settings(speed, &actuator.get_config().limits),
                    is_pattern,
                    self.handle,
                ))
                .unwrap_or_else(|err| error!("queue err {:?}", err));
        }
    }

    async fn do_stop(mut self, is_pattern: bool) -> WorkerResult {
        for actuator in self.actuators.iter() {
            trace!( actuator=actuator.identifier(), ?actuator.config, "do_stop");
            self.worker_task_sender
                .send(WorkerTask::End(
                    actuator.clone(),
                    is_pattern,
                    self.handle,
                    self.result_sender.clone(),
                ))
                .unwrap_or_else(|err| error!("queue err {:?}", err));
        }
        let mut last_result = Ok(());
        for _ in self.actuators.iter() {
            last_result = self.result_receiver.recv().await.unwrap();
        }
        last_result
    }

    async fn do_linear(&mut self, mut pos: f64, duration_ms: u32) -> WorkerResult {
        for actuator in &self.actuators {
            let settings = &actuator.get_config().limits.linear_or_max();
            pos = settings.apply_pos(pos);
            trace!(?duration_ms, ?pos, ?settings, "linear");
            self.worker_task_sender
                .send(WorkerTask::Move(
                    actuator.clone(),
                    pos,
                    duration_ms,
                    true,
                    self.result_sender.clone(),
                ))
                .unwrap_or_else(|err| error!("queue err {:?}", err));
        }
        sleep(Duration::from_millis(duration_ms as u64)).await;
        self.result_receiver.recv().await.unwrap()
    }

    async fn do_stroke(
        &mut self,
        start: bool,
        mut speed: Speed,
        settings: &LinearRange,
    ) -> WorkerResult {
        let mut wait_ms = 0;
        for actuator in &self.actuators {
            let actual_settings = settings.merge(&actuator.get_config().limits.linear_or_max());
            speed = actual_settings.scaling.apply(speed);
            wait_ms = actual_settings.get_duration_ms(speed);
            let target_pos = actual_settings.get_pos(start);
            debug!(?wait_ms, ?target_pos, ?actual_settings, "stroke");
            self.worker_task_sender
                .send(WorkerTask::Move(
                    actuator.clone(),
                    target_pos,
                    wait_ms,
                    true,
                    self.result_sender.clone(),
                ))
                .unwrap_or_else(|err| error!("queue err {:?}", err));
        }
        // breaks with multiple devices that have different settings
        sleep(Duration::from_millis(wait_ms as u64)).await;
        self.result_receiver.recv().await.unwrap()
    }

    fn stop_after(&self, duration: Duration) -> JoinHandle<()> {
        let cancellation_clone = self.cancellation_token.clone();
        Handle::current().spawn(async move {
            sleep(duration).await;
            cancellation_clone.cancel();
        })
    }

    fn try_update(&mut self, speed: &mut Speed) {
        if let Ok(update) = self.update_receiver.try_recv() {
            *speed = update;
        }
    }

    fn external_cancel(&self) -> bool {
        self.cancellation_token.is_cancelled()
    }
}

impl LinearRange {
    fn merge(&self, settings: &LinearRange) -> LinearRange {
        LinearRange {
            min_ms: if self.min_ms < settings.min_ms {
                settings.min_ms
            } else {
                self.min_ms
            },
            max_ms: if self.max_ms > settings.max_ms {
                settings.max_ms
            } else {
                self.max_ms
            },
            min_pos: if self.min_pos < settings.min_pos {
                settings.min_pos
            } else {
                self.min_pos
            },
            max_pos: if self.max_pos > settings.max_pos {
                settings.max_pos
            } else {
                self.max_pos
            },
            invert: if settings.invert {
                !self.invert
            } else {
                self.invert
            },
            scaling: match settings.scaling {
                LinearSpeedScaling::Linear => match self.scaling {
                    LinearSpeedScaling::Linear => LinearSpeedScaling::Linear,
                    LinearSpeedScaling::Parabolic(n) => LinearSpeedScaling::Parabolic(n),
                },
                LinearSpeedScaling::Parabolic(n) => LinearSpeedScaling::Parabolic(n),
            },
        }
    }
    pub fn get_pos(&self, move_up: bool) -> f64 {
        match move_up {
            true => {
                if self.invert {
                    1.0 - self.max_pos
                } else {
                    self.max_pos
                }
            }
            false => {
                if self.invert {
                    1.0 - self.min_pos
                } else {
                    self.min_pos
                }
            }
        }
    }
    pub fn apply_pos(&self, pos: f64) -> f64 {
        if self.invert {
            1.0 - pos
        } else {
            pos
        }
    }
    pub fn get_duration_ms(&self, speed: Speed) -> u32 {
        let factor = (100 - speed.value) as f64 / 100.0;
        let ms = self.min_ms as f64 + (self.max_ms - self.min_ms) as f64 * factor;
        ms as u32
    }
}

fn apply_scalar_settings(speed: Speed, settings: &ActuatorLimits) -> Speed {
    if speed.value == 0 {
        return speed;
    }
    match settings {
        ActuatorLimits::Scalar(settings) => {
            trace!("applying {settings:?}");
            let speed = Speed::from_float(speed.as_float() * settings.factor);
            if speed.value < settings.min_speed as u16 {
                Speed::new(settings.min_speed)
            } else if speed.value > settings.max_speed as u16 {
                Speed::new(settings.max_speed)
            } else {
                speed
            }
        }
        _ => speed,
    }
}

impl fmt::Debug for PatternPlayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PatternPlayer")
            .field("actuators", &self.actuators)
            .field("handle", &self.handle)
            .finish()
    }
}
