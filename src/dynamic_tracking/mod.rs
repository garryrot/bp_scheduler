use std::sync::Arc;

use buttplug::client::ButtplugClientDevice;
use tokio::{sync::mpsc::UnboundedReceiver, time::Instant};

use crate::linear::LinearRange;

pub mod movements;
pub mod collision;
pub mod tracking_mirror;
pub mod util;

pub struct Margins {
    most_inward: f64,
    most_outward: f64
}

pub enum TrackingSignal {
    Penetration(Instant), // starts or refreshes the movement time window for time_window_ms
    OutwardCompleted(Instant, f64 /* most inward */, f64 /* most outward */), // outward movement finished from pos1 to pos2
    InwardCompleted(Instant, f64 /* most inward */, f64 /* most outward */), // outward movement finished from pos1 to pos2
    Stop,
}

pub struct DynamicSettings {
    pub boundaries: LinearRange,
    pub min_resolution_ms: u32,
    pub min_duration_ms: u32,
    pub default_stroke_ms: u32,
    pub default_stroke_in: f64,
    pub default_stroke_out: f64,
    pub time_window_ms: u32,
}

impl Default for DynamicSettings {
    fn default() -> Self {
        DynamicSettings {
            boundaries: LinearRange::max(),
            min_resolution_ms: 50,
            min_duration_ms: 200,
            default_stroke_ms: 400,
            default_stroke_in: 0.0,
            default_stroke_out: 1.0,
            time_window_ms: 3_000,
        }
    }
}

pub enum Direction {
    Inward,
    Outward
}

pub struct DynamicTracking {
    pub settings: DynamicSettings,
    pub signals: UnboundedReceiver<TrackingSignal>,
    pub devices: Vec<Arc<ButtplugClientDevice>>,
}

