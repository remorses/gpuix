use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;
use std::sync::{Arc, Mutex};

use crate::element_tree::{ElementDesc, EventModifiers, EventPayload};
use crate::style::{parse_color, parse_color_hex};

/// The main GPUI renderer exposed to Node.js
///
/// This struct manages the GPUI application lifecycle and provides
/// methods to render element trees from JavaScript.
#[napi]
pub struct GpuixRenderer {
    /// Callback to send events back to JS
    event_callback: Option<ThreadsafeFunction<EventPayload>>,
    /// Current element tree
    current_tree: Arc<Mutex<Option<ElementDesc>>>,
    /// Whether the renderer is running
    running: Arc<Mutex<bool>>,
}

#[napi]
impl GpuixRenderer {
    /// Create a new GPUI renderer
    ///
    /// The event_callback will be called whenever a GPUI event fires
    /// that was registered by a React element.
    #[napi(constructor)]
    pub fn new(event_callback: Option<ThreadsafeFunction<EventPayload>>) -> Self {
        Self {
            event_callback,
            current_tree: Arc::new(Mutex::new(None)),
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Render an element tree
    ///
    /// This method receives a JSON-serialized element tree from React
    /// and triggers a GPUI re-render.
    #[napi]
    pub fn render(&self, tree_json: String) -> Result<()> {
        let tree: ElementDesc = serde_json::from_str(&tree_json)
            .map_err(|e| Error::from_reason(format!("Failed to parse element tree: {}", e)))?;

        let mut current = self.current_tree.lock().unwrap();
        *current = Some(tree);

        // TODO: Trigger GPUI re-render via cx.notify()
        // This requires storing an Entity handle

        Ok(())
    }

    /// Emit an event back to JavaScript
    pub fn emit_event(&self, payload: EventPayload) {
        if let Some(ref callback) = self.event_callback {
            callback.call(payload, ThreadsafeFunctionCallMode::NonBlocking);
        }
    }

    /// Start the GPUI application
    ///
    /// This blocks the current thread and runs the GPUI event loop.
    /// The renderer will process render requests from the JS side.
    #[napi]
    pub fn run(&self) -> Result<()> {
        {
            let mut running = self.running.lock().unwrap();
            if *running {
                return Err(Error::from_reason("Renderer is already running"));
            }
            *running = true;
        }

        let tree = self.current_tree.clone();
        let callback = self.event_callback.clone();

        // Run GPUI on the main thread
        gpui::Application::new().run(move |cx: &mut gpui::App| {
            let bounds = gpui::Bounds::centered(
                None,
                gpui::size(gpui::px(800.), gpui::px(600.)),
                cx,
            );

            cx.open_window(
                gpui::WindowOptions {
                    window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_window, cx| {
                    cx.new(|_| GpuixView {
                        tree: tree.clone(),
                        event_callback: callback.clone(),
                    })
                },
            )
            .unwrap();

            cx.activate(true);
        });

        Ok(())
    }

    /// Stop the GPUI application
    #[napi]
    pub fn stop(&self) -> Result<()> {
        let mut running = self.running.lock().unwrap();
        if !*running {
            return Err(Error::from_reason("Renderer is not running"));
        }
        *running = false;

        // TODO: Signal GPUI to quit

        Ok(())
    }

    /// Check if the renderer is running
    #[napi]
    pub fn is_running(&self) -> bool {
        *self.running.lock().unwrap()
    }

    /// Get window dimensions
    #[napi]
    pub fn get_window_size(&self) -> Result<WindowSize> {
        // TODO: Get actual window size from GPUI
        Ok(WindowSize {
            width: 800.0,
            height: 600.0,
        })
    }

    /// Set window title
    #[napi]
    pub fn set_window_title(&self, _title: String) -> Result<()> {
        // TODO: Set actual window title
        Ok(())
    }

    /// Focus an element by ID
    #[napi]
    pub fn focus_element(&self, _element_id: String) -> Result<()> {
        // TODO: Focus the element in GPUI
        Ok(())
    }

    /// Blur the currently focused element
    #[napi]
    pub fn blur(&self) -> Result<()> {
        // TODO: Blur in GPUI
        Ok(())
    }
}

/// The GPUI view that renders from the element tree
struct GpuixView {
    tree: Arc<Mutex<Option<ElementDesc>>>,
    event_callback: Option<ThreadsafeFunction<EventPayload>>,
}

impl gpui::Render for GpuixView {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        let tree = self.tree.lock().unwrap();

        match tree.as_ref() {
            Some(desc) => build_element(desc, &self.event_callback),
            None => gpui::div().into_any_element(),
        }
    }
}

/// Build a GPUI element from an ElementDesc
fn build_element(
    desc: &ElementDesc,
    event_callback: &Option<ThreadsafeFunction<EventPayload>>,
) -> gpui::AnyElement {
    match desc.element_type.as_str() {
        "div" => build_div(desc, event_callback),
        "text" => build_text(desc),
        _ => gpui::div().into_any_element(),
    }
}

/// Build a div element with styles and children
fn build_div(
    desc: &ElementDesc,
    event_callback: &Option<ThreadsafeFunction<EventPayload>>,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    let mut el = gpui::div();

    // Apply ID if present (needed for events)
    if let Some(ref id) = desc.id {
        el = el.id(gpui::SharedString::from(id.clone()));
    }

    // Apply styles
    if let Some(ref style) = desc.style {
        el = apply_styles(el, style);
    }

    // Wire up events
    if let Some(ref events) = desc.events {
        for event in events {
            match event.as_str() {
                "click" => {
                    if let Some(ref id) = desc.id {
                        let id = id.clone();
                        let callback = event_callback.clone();
                        el = el.on_click(move |event, _window, _cx| {
                            emit_event(&callback, &id, "click", Some(event.up.position));
                        });
                    }
                }
                "mouseEnter" => {
                    if let Some(ref id) = desc.id {
                        let id = id.clone();
                        let callback = event_callback.clone();
                        el = el.on_mouse_enter(move |event, _window, _cx| {
                            emit_event(&callback, &id, "mouseEnter", Some(event.position));
                        });
                    }
                }
                "mouseLeave" => {
                    if let Some(ref id) = desc.id {
                        let id = id.clone();
                        let callback = event_callback.clone();
                        el = el.on_mouse_leave(move |event, _window, _cx| {
                            emit_event(&callback, &id, "mouseLeave", Some(event.position));
                        });
                    }
                }
                "mouseDown" => {
                    if let Some(ref id) = desc.id {
                        let id = id.clone();
                        let callback = event_callback.clone();
                        el = el.on_mouse_down(gpui::MouseButton::Left, move |event, _window, _cx| {
                            emit_event(&callback, &id, "mouseDown", Some(event.position));
                        });
                    }
                }
                "mouseUp" => {
                    if let Some(ref id) = desc.id {
                        let id = id.clone();
                        let callback = event_callback.clone();
                        el = el.on_mouse_up(gpui::MouseButton::Left, move |event, _window, _cx| {
                            emit_event(&callback, &id, "mouseUp", Some(event.position));
                        });
                    }
                }
                _ => {}
            }
        }
    }

    // Add text content if present
    if let Some(ref content) = desc.content {
        el = el.child(content.clone());
    }

    // Add children recursively
    if let Some(ref children) = desc.children {
        for child in children {
            el = el.child(build_element(child, event_callback));
        }
    }

    el.into_any_element()
}

/// Build a text element
fn build_text(desc: &ElementDesc) -> gpui::AnyElement {
    use gpui::prelude::*;

    let content = desc.content.clone().unwrap_or_default();

    // Text with optional styling
    if let Some(ref style) = desc.style {
        let mut el = gpui::div();

        // Apply text styles
        if let Some(hex) = style.color.as_ref().and_then(|c| parse_color_hex(c)) {
            el = el.text_color(gpui::rgba(hex));
        }
        if let Some(size) = style.font_size {
            el = el.text_size(gpui::px(size as f32));
        }

        el.child(content).into_any_element()
    } else {
        content.into_any_element()
    }
}

/// Apply styles from StyleDesc to a div
fn apply_styles(
    mut el: gpui::Div,
    style: &crate::style::StyleDesc,
) -> gpui::Div {
    use gpui::prelude::*;

    // Display & flex
    if style.display.as_deref() == Some("flex") {
        el = el.flex();
    }
    if style.flex_direction.as_deref() == Some("column") {
        el = el.flex_col();
    }
    if style.flex_direction.as_deref() == Some("row") {
        el = el.flex_row();
    }

    // Flex properties
    if let Some(grow) = style.flex_grow {
        el = el.flex_grow();
        let _ = grow; // TODO: support arbitrary grow values
    }
    if let Some(shrink) = style.flex_shrink {
        el = el.flex_shrink();
        let _ = shrink;
    }

    // Alignment
    match style.align_items.as_deref() {
        Some("center") => el = el.items_center(),
        Some("start") | Some("flex-start") => el = el.items_start(),
        Some("end") | Some("flex-end") => el = el.items_end(),
        _ => {}
    }
    match style.justify_content.as_deref() {
        Some("center") => el = el.justify_center(),
        Some("start") | Some("flex-start") => el = el.justify_start(),
        Some("end") | Some("flex-end") => el = el.justify_end(),
        Some("between") | Some("space-between") => el = el.justify_between(),
        Some("around") | Some("space-around") => el = el.justify_around(),
        _ => {}
    }

    // Gap
    if let Some(gap) = style.gap {
        el = el.gap(gpui::px(gap as f32));
    }

    // Sizing
    if let Some(w) = style.width {
        el = el.w(gpui::px(w as f32));
    }
    if let Some(h) = style.height {
        el = el.h(gpui::px(h as f32));
    }
    if let Some(min_w) = style.min_width {
        el = el.min_w(gpui::px(min_w as f32));
    }
    if let Some(min_h) = style.min_height {
        el = el.min_h(gpui::px(min_h as f32));
    }
    if let Some(max_w) = style.max_width {
        el = el.max_w(gpui::px(max_w as f32));
    }
    if let Some(max_h) = style.max_height {
        el = el.max_h(gpui::px(max_h as f32));
    }

    // Padding
    if let Some(p) = style.padding {
        el = el.p(gpui::px(p as f32));
    }
    if let Some(pt) = style.padding_top {
        el = el.pt(gpui::px(pt as f32));
    }
    if let Some(pr) = style.padding_right {
        el = el.pr(gpui::px(pr as f32));
    }
    if let Some(pb) = style.padding_bottom {
        el = el.pb(gpui::px(pb as f32));
    }
    if let Some(pl) = style.padding_left {
        el = el.pl(gpui::px(pl as f32));
    }

    // Margin
    if let Some(m) = style.margin {
        el = el.m(gpui::px(m as f32));
    }
    if let Some(mt) = style.margin_top {
        el = el.mt(gpui::px(mt as f32));
    }
    if let Some(mr) = style.margin_right {
        el = el.mr(gpui::px(mr as f32));
    }
    if let Some(mb) = style.margin_bottom {
        el = el.mb(gpui::px(mb as f32));
    }
    if let Some(ml) = style.margin_left {
        el = el.ml(gpui::px(ml as f32));
    }

    // Background color
    if let Some(ref bg) = style.background_color.as_ref().or(style.background.as_ref()) {
        if let Some(hex) = parse_color_hex(bg) {
            el = el.bg(gpui::rgba(hex));
        }
    }

    // Text color
    if let Some(ref color) = style.color {
        if let Some(hex) = parse_color_hex(color) {
            el = el.text_color(gpui::rgba(hex));
        }
    }

    // Border radius
    if let Some(radius) = style.border_radius {
        el = el.rounded(gpui::px(radius as f32));
    }

    // Border
    if let Some(width) = style.border_width {
        if width > 0.0 {
            el = el.border(gpui::px(width as f32));
        }
    }
    if let Some(ref color) = style.border_color {
        if let Some(hex) = parse_color_hex(color) {
            el = el.border_color(gpui::rgba(hex));
        }
    }

    // Opacity
    if let Some(opacity) = style.opacity {
        el = el.opacity(opacity as f32);
    }

    // Cursor
    match style.cursor.as_deref() {
        Some("pointer") => el = el.cursor_pointer(),
        Some("default") => el = el.cursor_default(),
        _ => {}
    }

    // Overflow
    match style.overflow.as_deref() {
        Some("hidden") => el = el.overflow_hidden(),
        Some("scroll") => el = el.overflow_scroll(),
        _ => {}
    }

    el
}

/// Helper to emit an event back to JS
fn emit_event(
    callback: &Option<ThreadsafeFunction<EventPayload>>,
    element_id: &str,
    event_type: &str,
    position: Option<gpui::Point<gpui::Pixels>>,
) {
    if let Some(ref cb) = callback {
        let payload = EventPayload {
            element_id: element_id.to_string(),
            event_type: event_type.to_string(),
            x: position.map(|p| p.x.0 as f64),
            y: position.map(|p| p.y.0 as f64),
            key: None,
            modifiers: Some(EventModifiers::default()),
        };
        cb.call(payload, ThreadsafeFunctionCallMode::NonBlocking);
    }
}

#[derive(Debug, Clone)]
#[napi(object)]
pub struct WindowSize {
    pub width: f64,
    pub height: f64,
}

/// Configuration for window creation
#[derive(Debug, Clone)]
#[napi(object)]
pub struct WindowOptions {
    pub title: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub min_width: Option<f64>,
    pub min_height: Option<f64>,
    pub resizable: Option<bool>,
    pub fullscreen: Option<bool>,
    pub transparent: Option<bool>,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: Some("GPUIX".to_string()),
            width: Some(800.0),
            height: Some(600.0),
            min_width: None,
            min_height: None,
            resizable: Some(true),
            fullscreen: Some(false),
            transparent: Some(false),
        }
    }
}
