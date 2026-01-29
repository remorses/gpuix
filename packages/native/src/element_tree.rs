use napi_derive::napi;
use serde::{Deserialize, Serialize};

use crate::style::StyleDesc;

/// Element description serialized from JS
/// Note: This is only used for JSON deserialization, not direct napi binding
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementDesc {
    /// Element type: "div", "text", "img", "svg", etc.
    #[serde(rename = "elementType", alias = "type")]
    pub element_type: String,

    /// Unique element ID for event handling
    pub id: Option<String>,

    /// Style properties
    pub style: Option<StyleDesc>,

    /// Text content (for text elements)
    pub content: Option<String>,

    /// Image source (for img elements)
    pub src: Option<String>,

    /// SVG path (for svg elements)
    pub path: Option<String>,

    /// Events this element listens to
    pub events: Option<Vec<String>>,

    /// Focus properties
    pub tab_index: Option<i32>,
    pub tab_stop: Option<bool>,
    pub auto_focus: Option<bool>,

    /// Children elements
    pub children: Option<Vec<ElementDesc>>,
}

/// Event payload sent back to JS
#[derive(Debug, Clone)]
#[napi(object)]
pub struct EventPayload {
    pub element_id: String,
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
