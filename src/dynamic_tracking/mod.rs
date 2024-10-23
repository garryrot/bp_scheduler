use std::sync::Arc;

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
    pub min_duration_ms: u32,
    pub default_stroke_ms: u32,
    pub default_stroke_in: f64,
    pub default_stroke_out: f64,
    pub stroke_window_ms: u32
}

impl Default for DynamicSettings {
    fn default() -> Self {
        DynamicSettings {
            move_at_start: true,
            min_resolution_ms: 50,
            min_duration_ms: 200,
            default_stroke_ms: 400,
            default_stroke_in: 0.0,
            default_stroke_out: 1.0,
            stroke_window_ms: 3_000
        }
    }
}

pub struct DynamicTracking {
    pub settings: DynamicSettings,
    pub signals: UnboundedReceiver<TrackingSignal>,
    pub actuators: Vec<Arc<Actuator>>,
}
