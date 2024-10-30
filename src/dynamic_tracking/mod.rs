use std::sync::{atomic::AtomicI64, Arc};

use derive_new::new;
use serde::{Deserialize, Serialize};
use tokio::{sync::mpsc::UnboundedReceiver, time::Instant};

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
pub struct DynamicSettings {
    pub move_at_start: bool,
    pub min_resolution_ms: u32,
    pub stroke_min_ms: u32,
    pub stroke_max_ms: u32,
    pub sample_ms: u64,
    pub initial_timeout_ms: u64,
    pub stroke_default_ms: u32,
    pub stroke_default_in: f64,
    pub stroke_default_out: f64
}

impl Default for DynamicSettings {
    fn default() -> Self {
        DynamicSettings {
            move_at_start: true,
            min_resolution_ms: 80,
            stroke_min_ms: 200,
            stroke_max_ms: 2_000,
            sample_ms: 50,
            stroke_default_ms: 400,
            stroke_default_in: 0.0,
            stroke_default_out: 1.0,
            initial_timeout_ms: 800,
        }
    }
}

pub struct DynamicTracking {
    pub settings: DynamicSettings,
    pub signals: UnboundedReceiver<TrackingSignal>,
    pub actuators: Vec<Arc<Actuator>>,
    pub cur_avg_ms: Arc<AtomicI64>,
    pub cur_depth: Arc<AtomicI64>
}
