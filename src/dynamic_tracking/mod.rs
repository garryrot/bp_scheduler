use std::sync::{atomic::{AtomicI64, Ordering}, Arc};

use derive_new::new;
use serde::{Deserialize, Serialize};
use tokio::{sync::mpsc::UnboundedReceiver, time::Instant};
use tokio_util::sync::CancellationToken;

use crate::actuator::Actuator;

pub mod movements;
pub mod collision;
pub mod tracking_mirror;
pub mod util;

#[derive(new)]
pub struct Margins {
    most_in: f64,
    most_out: f64
}

pub enum TrackingSignal {
    Penetration(Instant),
    OuterTurn(Instant, Margins),
    InnerTurn(Instant, Margins),
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrokerSettings {
    pub move_at_start: bool,
    pub starting_position: f64,
    pub min_resolution_ms: u32,
    pub min_ms_for_full_stroke: u32,
    pub stroke_max_ms: u32,
    pub sampling_rate_ms: u64,
    pub initial_timeout_ms: u64,
    pub stroke_default_ms: u32
}

impl Default for StrokerSettings {
    fn default() -> Self {
        StrokerSettings {
            move_at_start: true,
            starting_position: 1.0,
            min_resolution_ms: 80,
            min_ms_for_full_stroke: 200,
            stroke_max_ms: 2_000,
            sampling_rate_ms: 50,
            stroke_default_ms: 400,
            initial_timeout_ms: 800,
        }
    }
}

// TODO Rename to BoneTrackingAction
pub struct DynamicTracking {
    pub settings: StrokerSettings,
    pub signals: UnboundedReceiver<TrackingSignal>,
    pub actuators: Vec<Arc<Actuator>>,
    pub status: DynamicTrackingHandle
}

// TODO: Rename to BoneTracking
#[derive(Clone, Debug)]
pub struct DynamicTrackingHandle {
    pub cancel: Option<CancellationToken>,
    pub cur_avg_ms: Arc<AtomicI64>,
    pub cur_avg_depth: Arc<AtomicI64>,
    pub cur_pos: Arc<AtomicI64>
}

impl DynamicTrackingHandle {
    pub fn reset(&mut self) {
        self.cur_avg_ms.store(0, Ordering::Relaxed);
        self.cur_avg_depth.store(0, Ordering::Relaxed);
        self.cur_pos.store(0, Ordering::Relaxed);
    }
}

impl Default for DynamicTrackingHandle {
    fn default() -> Self {
        Self { 
            cancel: None, 
            cur_avg_ms: Arc::new(AtomicI64::new(0)), 
            cur_avg_depth: Arc::new(AtomicI64::new(0)),
            cur_pos: Arc::new(AtomicI64::new(0))
        }
    }
}