/// Event types for Rust → JS communication.
/// Element IDs are f64 (JS numbers) — lossless for integers up to 2^53.

use napi_derive::napi;

/// Event payload sent back to JS when a user interacts with an element.
#[derive(Debug, Clone)]
#[napi(object)]
pub struct EventPayload {
    pub element_id: f64,
    pub event_type: String,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub key: Option<String>,
    pub modifiers: Option<EventModifiers>,
}

#[derive(Debug, Clone)]
#[napi(object)]
pub struct EventModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub cmd: bool,
}

impl Default for EventModifiers {
    fn default() -> Self {
        Self {
            shift: false,
            ctrl: false,
            alt: false,
            cmd: false,
        }
    }
}
