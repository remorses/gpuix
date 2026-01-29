use gpui::AppContext as _;
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::element_tree::{ElementDesc, EventModifiers, EventPayload};
use crate::style::parse_color_hex;

static ELEMENT_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

fn generate_element_id() -> String {
    let id = ELEMENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("__gpuix_{}", id)
}

/// The main GPUI renderer exposed to Node.js
#[napi]
pub struct GpuixRenderer {
    event_callback: Option<ThreadsafeFunction<EventPayload>>,
    current_tree: Arc<Mutex<Option<ElementDesc>>>,
    running: Arc<Mutex<bool>>,
    window_title: Arc<Mutex<Option<String>>>,
}

#[napi]
impl GpuixRenderer {
    #[napi(constructor)]
    pub fn new(event_callback: Option<ThreadsafeFunction<EventPayload>>) -> Self {
        Self {
            event_callback,
            current_tree: Arc::new(Mutex::new(None)),
            running: Arc::new(Mutex::new(false)),
            window_title: Arc::new(Mutex::new(None)),
        }
    }

    #[napi]
    pub fn render(&self, tree_json: String) -> Result<()> {
        eprintln!("[GPUIX-RUST] render() called, JSON length: {}", tree_json.len());
        eprintln!("[GPUIX-RUST] JSON preview: {}", &tree_json[..tree_json.len().min(500)]);
        
        let tree: ElementDesc = serde_json::from_str(&tree_json)
            .map_err(|e| {
                eprintln!("[GPUIX-RUST] Failed to parse: {}", e);
                Error::from_reason(format!("Failed to parse element tree: {}", e))
            })?;

        eprintln!("[GPUIX-RUST] Parsed tree type: {:?}", tree.element_type);
        
        let mut current = self.current_tree.lock().unwrap();
        *current = Some(tree);
        eprintln!("[GPUIX-RUST] Tree stored successfully");

        Ok(())
    }

    pub fn emit_event(&self, payload: EventPayload) {
        if let Some(ref callback) = self.event_callback {
            callback.call(Ok(payload), ThreadsafeFunctionCallMode::NonBlocking);
        }
    }

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
        let window_title = self.window_title.clone();

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
                        window_title: window_title.clone(),
                    })
                },
            )
            .unwrap();

            cx.activate(true);
        });

        let mut is_running = self.running.lock().unwrap();
        *is_running = false;

        Ok(())
    }

    #[napi]
    pub fn stop(&self) -> Result<()> {
        let mut running = self.running.lock().unwrap();
        if !*running {
            return Err(Error::from_reason("Renderer is not running"));
        }
        *running = false;
        Ok(())
    }

    #[napi]
    pub fn is_running(&self) -> bool {
        *self.running.lock().unwrap()
    }

    #[napi]
    pub fn get_window_size(&self) -> Result<WindowSize> {
        Ok(WindowSize {
            width: 800.0,
            height: 600.0,
        })
    }

    #[napi]
    pub fn set_window_title(&self, title: String) -> Result<()> {
        *self.window_title.lock().unwrap() = Some(title);
        Ok(())
    }

    #[napi]
    pub fn focus_element(&self, _element_id: String) -> Result<()> {
        Ok(())
    }

    #[napi]
    pub fn blur(&self) -> Result<()> {
        Ok(())
    }
}

struct GpuixView {
    tree: Arc<Mutex<Option<ElementDesc>>>,
    event_callback: Option<ThreadsafeFunction<EventPayload>>,
    window_title: Arc<Mutex<Option<String>>>,
}

impl gpui::Render for GpuixView {
    fn render(&mut self, window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        use gpui::IntoElement;

        if let Some(title) = self.window_title.lock().unwrap().as_ref() {
            window.set_window_title(title);
        }
        
        let tree = self.tree.lock().unwrap();

        match tree.as_ref() {
            Some(desc) => {
                eprintln!("[GPUIX-RUST] GpuixView::render - building tree, root type: {:?}", desc.element_type);
                build_element(desc, &self.event_callback)
            },
            None => {
                eprintln!("[GPUIX-RUST] GpuixView::render - NO TREE, returning Empty");
                gpui::Empty.into_any_element()
            },
        }
    }
}

fn build_element(
    desc: &ElementDesc,
    event_callback: &Option<ThreadsafeFunction<EventPayload>>,
) -> gpui::AnyElement {
    use gpui::IntoElement;
    eprintln!(
        "[GPUIX-RUST] build_element: type={:?} id={:?} children={} style_present={}",
        desc.element_type,
        desc.id,
        desc.children.as_ref().map(|c| c.len()).unwrap_or(0),
        desc.style.is_some()
    );

    match desc.element_type.as_str() {
        "div" => build_div(desc, event_callback),
        "text" => build_text(desc),
        _ => gpui::Empty.into_any_element(),
    }
}

fn build_div(
    desc: &ElementDesc,
    event_callback: &Option<ThreadsafeFunction<EventPayload>>,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    // Get or generate element ID
    let element_id = desc.id.clone().unwrap_or_else(generate_element_id);
    eprintln!(
        "[GPUIX-RUST] build_div: id={} children={} style_present={}",
        element_id,
        desc.children.as_ref().map(|c| c.len()).unwrap_or(0),
        desc.style.is_some()
    );
    
    // Create stateful div with ID
    let mut el = gpui::div().id(gpui::SharedString::from(element_id.clone()));

    // Apply styles
    if let Some(ref style) = desc.style {
        eprintln!("[GPUIX-RUST] build_div: applying styles for id={}", element_id);
        el = apply_styles(el, style);
    }

    // Wire up events
    if let Some(ref events) = desc.events {
        for event in events {
            match event.as_str() {
                "click" => {
                    let id = element_id.clone();
                    let callback = event_callback.clone();
                    el = el.on_click(move |click_event, _window, cx| {
                        eprintln!("[GPUIX-RUST] on_click fired for id={}", id);
                        emit_event(&callback, &id, "click", Some(click_event.position()));
                        cx.refresh_windows();
                    });
                }
                "mouseDown" => {
                    let id = element_id.clone();
                    let callback = event_callback.clone();
                    el = el.on_mouse_down(gpui::MouseButton::Left, move |mouse_event, _window, cx| {
                        eprintln!("[GPUIX-RUST] on_mouse_down fired for id={}", id);
                        emit_event(&callback, &id, "mouseDown", Some(mouse_event.position));
                        cx.refresh_windows();
                    });
                }
                "mouseUp" => {
                    let id = element_id.clone();
                    let callback = event_callback.clone();
                    el = el.on_mouse_up(gpui::MouseButton::Left, move |mouse_event, _window, cx| {
                        eprintln!("[GPUIX-RUST] on_mouse_up fired for id={}", id);
                        emit_event(&callback, &id, "mouseUp", Some(mouse_event.position));
                        cx.refresh_windows();
                    });
                }
                "mouseMove" => {
                    let id = element_id.clone();
                    let callback = event_callback.clone();
                    el = el.on_mouse_move(move |mouse_event, _window, cx| {
                        eprintln!("[GPUIX-RUST] on_mouse_move fired for id={}", id);
                        emit_event(&callback, &id, "mouseMove", Some(mouse_event.position));
                        cx.refresh_windows();
                    });
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
        eprintln!(
            "[GPUIX-RUST] build_div: rendering {} children for id={}",
            children.len(),
            element_id
        );
        for child in children {
            el = el.child(build_element(child, event_callback));
        }
    }

    el.into_any_element()
}

fn build_text(desc: &ElementDesc) -> gpui::AnyElement {
    use gpui::prelude::*;

    let content = desc.content.clone().unwrap_or_default();

    if let Some(ref style) = desc.style {
        let mut el = gpui::div();

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

// Helper functions for dimension handling
fn apply_width<E: gpui::Styled>(el: E, dim: &crate::style::DimensionValue) -> E {
    eprintln!("[GPUIX-RUST] apply_width: {:?}", dim);
    match dim {
        crate::style::DimensionValue::Pixels(v) => el.w(gpui::px(*v as f32)),
        crate::style::DimensionValue::Percentage(v) => el.w(gpui::relative(*v as f32)),
        crate::style::DimensionValue::Auto => el, // auto is default
    }
}

fn apply_height<E: gpui::Styled>(el: E, dim: &crate::style::DimensionValue) -> E {
    eprintln!("[GPUIX-RUST] apply_height: {:?}", dim);
    match dim {
        crate::style::DimensionValue::Pixels(v) => el.h(gpui::px(*v as f32)),
        crate::style::DimensionValue::Percentage(v) => el.h(gpui::relative(*v as f32)),
        crate::style::DimensionValue::Auto => el, // auto is default
    }
}

fn apply_styles<E: gpui::Styled>(mut el: E, style: &crate::style::StyleDesc) -> E {
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
    if style.flex_grow.is_some() {
        el = el.flex_grow();
    }
    if style.flex_shrink.is_some() {
        el = el.flex_shrink();
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
    if let Some(ref w) = style.width {
        el = apply_width(el, w);
    }
    if let Some(ref h) = style.height {
        el = apply_height(el, h);
    }
    if let Some(ref min_w) = style.min_width {
        match min_w {
            crate::style::DimensionValue::Pixels(v) => el = el.min_w(gpui::px(*v as f32)),
            crate::style::DimensionValue::Percentage(v) => el = el.min_w(gpui::relative(*v as f32)),
            crate::style::DimensionValue::Auto => {}
        }
    }
    if let Some(ref min_h) = style.min_height {
        match min_h {
            crate::style::DimensionValue::Pixels(v) => el = el.min_h(gpui::px(*v as f32)),
            crate::style::DimensionValue::Percentage(v) => el = el.min_h(gpui::relative(*v as f32)),
            crate::style::DimensionValue::Auto => {}
        }
    }
    if let Some(ref max_w) = style.max_width {
        match max_w {
            crate::style::DimensionValue::Pixels(v) => el = el.max_w(gpui::px(*v as f32)),
            crate::style::DimensionValue::Percentage(v) => el = el.max_w(gpui::relative(*v as f32)),
            crate::style::DimensionValue::Auto => {}
        }
    }
    if let Some(ref max_h) = style.max_height {
        match max_h {
            crate::style::DimensionValue::Pixels(v) => el = el.max_h(gpui::px(*v as f32)),
            crate::style::DimensionValue::Percentage(v) => el = el.max_h(gpui::relative(*v as f32)),
            crate::style::DimensionValue::Auto => {}
        }
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
        eprintln!("[GPUIX-RUST] apply_styles: background={}", bg);
        if let Some(hex) = parse_color_hex(bg) {
            el = el.bg(gpui::rgba(hex));
        } else {
            eprintln!("[GPUIX-RUST] apply_styles: failed to parse background color {}", bg)
        }
    }

    // Text color
    if let Some(ref color) = style.color {
        eprintln!("[GPUIX-RUST] apply_styles: text color={}", color);
        if let Some(hex) = parse_color_hex(color) {
            el = el.text_color(gpui::rgba(hex));
        } else {
            eprintln!("[GPUIX-RUST] apply_styles: failed to parse text color {}", color)
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
        _ => {}
    }

    el
}

fn emit_event(
    callback: &Option<ThreadsafeFunction<EventPayload>>,
    element_id: &str,
    event_type: &str,
    position: Option<gpui::Point<gpui::Pixels>>,
) {
    if let Some(cb) = callback {
        eprintln!("[GPUIX-RUST] emit_event -> id={} type={}", element_id, event_type);
        let payload = EventPayload {
            element_id: element_id.to_string(),
            event_type: event_type.to_string(),
            x: position.map(|p| f64::from(f32::from(p.x))),
            y: position.map(|p| f64::from(f32::from(p.y))),
            key: None,
            modifiers: Some(EventModifiers::default()),
        };
        cb.call(Ok(payload), ThreadsafeFunctionCallMode::Blocking);
    }
}

#[derive(Debug, Clone)]
#[napi(object)]
pub struct WindowSize {
    pub width: f64,
    pub height: f64,
}

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
