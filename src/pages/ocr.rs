use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
pub struct OcrObservation {
    pub text: String,
    pub confidence: f32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
pub struct OcrSnapshot {
    pub pid: i32,
    pub window_id: u32,
    pub window_x: f64,
    pub window_y: f64,
    pub window_width: f64,
    pub window_height: f64,
    pub width: i64,
    pub height: i64,
    pub observations: Vec<OcrObservation>,
    pub warnings: Vec<String>,
}
