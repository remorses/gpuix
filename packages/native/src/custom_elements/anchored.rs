/// Anchored custom element — positions children at window coordinates and can
/// optionally render in a deferred overlay layer for popovers/tooltips.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};

pub struct AnchoredFactory;

impl CustomElementFactory for AnchoredFactory {
    fn element_type(&self) -> &str {
        "anchored"
    }

    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(AnchoredElement::default())
    }
}

#[derive(Debug, Clone, Copy, Default)]
enum AnchorCorner {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl AnchorCorner {
    fn from_str(value: &str) -> Self {
        match value {
            "topRight" => Self::TopRight,
            "bottomLeft" => Self::BottomLeft,
            "bottomRight" => Self::BottomRight,
            _ => Self::TopLeft,
        }
    }

    fn as_gpui(self) -> gpui::Corner {
        match self {
            Self::TopLeft => gpui::Corner::TopLeft,
            Self::TopRight => gpui::Corner::TopRight,
            Self::BottomLeft => gpui::Corner::BottomLeft,
            Self::BottomRight => gpui::Corner::BottomRight,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::TopLeft => "topLeft",
            Self::TopRight => "topRight",
            Self::BottomLeft => "bottomLeft",
            Self::BottomRight => "bottomRight",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnchoredElement {
    x: f32,
    y: f32,
    anchor: AnchorCorner,
    snap_to_window: bool,
    snap_margin: f32,
    deferred: bool,
    priority: usize,
}

impl Default for AnchoredElement {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            anchor: AnchorCorner::TopLeft,
            snap_to_window: true,
            snap_margin: 8.0,
            deferred: true,
            priority: 1,
        }
    }
}

impl CustomElement for AnchoredElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        let mut content = gpui::div().flex_col();
        if let Some(style) = ctx.style {
            content = crate::renderer::apply_styles(content, style);
        }

        for child in ctx.children {
            content = content.child(child);
        }

        let mut anchored = gpui::anchored()
            .position(gpui::point(gpui::px(self.x), gpui::px(self.y)))
            .anchor(self.anchor.as_gpui());

        if self.snap_to_window {
            anchored = anchored.snap_to_window_with_margin(gpui::px(self.snap_margin));
        }

        let anchored = anchored.child(content);

        if self.deferred {
            gpui::deferred(anchored)
                .with_priority(self.priority)
                .into_any_element()
        } else {
            anchored.into_any_element()
        }
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "x" => self.x = value.as_f64().unwrap_or(0.0) as f32,
            "y" => self.y = value.as_f64().unwrap_or(0.0) as f32,
            "position" => {
                if let Some(obj) = value.as_object() {
                    if let Some(x) = obj.get("x").and_then(|v| v.as_f64()) {
                        self.x = x as f32;
                    }
                    if let Some(y) = obj.get("y").and_then(|v| v.as_f64()) {
                        self.y = y as f32;
                    }
                }
            }
            "anchor" => {
                self.anchor = value
                    .as_str()
                    .map(AnchorCorner::from_str)
                    .unwrap_or(AnchorCorner::TopLeft)
            }
            "snapToWindow" => self.snap_to_window = value.as_bool().unwrap_or(true),
            "snapMargin" => self.snap_margin = value.as_f64().unwrap_or(8.0) as f32,
            "deferred" => self.deferred = value.as_bool().unwrap_or(true),
            "priority" => {
                let n = value.as_u64().unwrap_or(1);
                self.priority = usize::try_from(n).unwrap_or(1);
            }
            _ => {}
        }
    }

    fn supported_props(&self) -> &[&str] {
        &[
            "x",
            "y",
            "position",
            "anchor",
            "snapToWindow",
            "snapMargin",
            "deferred",
            "priority",
        ]
    }

    fn get_prop(&self, key: &str) -> Option<serde_json::Value> {
        match key {
            "x" => Some(serde_json::Value::from(self.x)),
            "y" => Some(serde_json::Value::from(self.y)),
            "position" => Some(serde_json::json!({ "x": self.x, "y": self.y })),
            "anchor" => Some(serde_json::Value::String(self.anchor.as_str().to_string())),
            "snapToWindow" => Some(serde_json::Value::Bool(self.snap_to_window)),
            "snapMargin" => Some(serde_json::Value::from(self.snap_margin)),
            "deferred" => Some(serde_json::Value::Bool(self.deferred)),
            "priority" => Some(serde_json::Value::from(self.priority as u64)),
            _ => None,
        }
    }

    fn supported_events(&self) -> &[&str] {
        &[]
    }

    fn destroy(&mut self) {}
}
