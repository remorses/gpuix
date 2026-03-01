/// Input custom element — a focusable text input rendered with GPUI primitives.
///
/// Demonstrates the CustomElement trait with:
/// - Props from React (value, placeholder, readOnly)
/// - Event handling (keyDown, keyUp, click, focus, blur)
/// - Focus management via GPUI FocusHandle
///
/// This is a controlled component: React owns the text state via the `value`
/// prop. The element renders whatever `value` is set to. Key events flow
/// back to React which updates `value` — completing the round-trip.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use crate::renderer::emit_event_full;

// ── Factory ──────────────────────────────────────────────────────────

pub struct InputFactory;

impl CustomElementFactory for InputFactory {
    fn element_type(&self) -> &str {
        "input"
    }

    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(InputElement {
            value: String::new(),
            placeholder: String::new(),
            read_only: false,
        })
    }
}

// ── Element ──────────────────────────────────────────────────────────

pub struct InputElement {
    value: String,
    placeholder: String,
    read_only: bool,
}

impl CustomElement for InputElement {
    fn render(
        &mut self,
        ctx: &CustomRenderContext,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        let is_empty = self.value.is_empty();
        let display_text = if is_empty {
            self.placeholder.clone()
        } else {
            self.value.clone()
        };

        let element_id_str = format!("__gpuix_input_{}", ctx.id);
        let mut el = gpui::div()
            .id(gpui::SharedString::from(element_id_str))
            .flex()
            .items_center()
            .overflow_hidden()
            .px(gpui::px(8.0))
            .py(gpui::px(4.0))
            .min_h(gpui::px(28.0))
            .border(gpui::px(1.0))
            .border_color(gpui::rgba(0x555555ff))
            .bg(gpui::rgba(0x1e1e2eff))
            .rounded(gpui::px(4.0))
            .text_color(if is_empty {
                gpui::rgba(0x888888ff)
            } else {
                gpui::rgba(0xe0e0e0ff)
            })
            .child(display_text);

        // Apply React style prop on top of defaults for custom element parity.
        if let Some(style) = ctx.style {
            el = crate::renderer::apply_styles(el, style);
        }

        // Attach focus handle if one exists (created by sync_focus_handles
        // when the element has keyDown/keyUp/focus/blur listeners).
        if let Some(handle) = ctx.focus_handle {
            el = el.track_focus(handle);
        }

        // Wire events — same pattern as build_div but scoped to this element.
        for event_type in ctx.events {
            let id = ctx.id;
            let callback = ctx.event_callback.clone();
            match event_type.as_str() {
                "keyDown" if !self.read_only => {
                    el = el.on_key_down(move |key_event, _window, _cx| {
                        emit_event_full(&callback, id, "keyDown", |p| {
                            p.key = Some(key_event.keystroke.key.clone());
                            p.key_char = key_event.keystroke.key_char.clone();
                            p.is_held = Some(key_event.is_held);
                            p.modifiers = Some(key_event.keystroke.modifiers.into());
                        });
                    });
                }
                "keyUp" if !self.read_only => {
                    let callback = callback.clone();
                    el = el.on_key_up(move |key_event, _window, _cx| {
                        emit_event_full(&callback, id, "keyUp", |p| {
                            p.key = Some(key_event.keystroke.key.clone());
                            p.key_char = key_event.keystroke.key_char.clone();
                            p.modifiers = Some(key_event.keystroke.modifiers.into());
                        });
                    });
                }
                "click" => {
                    let callback = callback.clone();
                    el = el.on_click(move |click_event, _window, _cx| {
                        emit_event_full(&callback, id, "click", |p| {
                            let (x, y) = crate::renderer::point_to_xy(click_event.position());
                            p.x = Some(x);
                            p.y = Some(y);
                            p.modifiers = Some(click_event.modifiers().into());
                        });
                    });
                }
                // Focus/blur handled by FocusHandle subscriptions in sync_focus_handles.
                "focus" | "blur" => {}
                _ => {}
            }
        }

        el.into_any_element()
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "value" => self.value = value.as_str().unwrap_or("").to_string(),
            "placeholder" => self.placeholder = value.as_str().unwrap_or("").to_string(),
            "readOnly" => self.read_only = value.as_bool().unwrap_or(false),
            _ => {}
        }
    }

    fn supported_props(&self) -> &[&str] {
        &["value", "placeholder", "readOnly"]
    }

    fn get_prop(&self, key: &str) -> Option<serde_json::Value> {
        match key {
            "value" => Some(serde_json::Value::String(self.value.clone())),
            "placeholder" => Some(serde_json::Value::String(self.placeholder.clone())),
            "readOnly" => Some(serde_json::Value::Bool(self.read_only)),
            _ => None,
        }
    }

    fn supported_events(&self) -> &[&str] {
        &["keyDown", "keyUp", "click", "focus", "blur"]
    }

    fn destroy(&mut self) {}
}
