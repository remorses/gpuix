/// GpuixRenderer — napi-rs binding exposed to Node.js.
///
/// Mutation-based API: React's reconciler sends individual mutations
/// (createElement, appendChild, setStyle, etc.) instead of a full JSON tree.
/// Rust maintains a RetainedTree and rebuilds GPUI elements from it each frame.
///
/// Lifecycle:
///   const renderer = new GpuixRenderer(eventCallback)
///   renderer.init({ title: 'My App', width: 800, height: 600 })
///   renderer.createElement(1, "div")     // mutations from React reconciler
///   renderer.appendChild(0, 1)
///   renderer.commitMutations()           // signal batch complete
///   setImmediate(function loop() {       // drive the frame loop
///     renderer.tick()
///     setImmediate(loop)
///   })

use gpui::AppContext as _;
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::element_tree::{EventModifiers, EventPayload};
use crate::platform::NodePlatform;
use crate::retained_tree::RetainedTree;
use crate::style::{parse_color_hex, StyleDesc};

/// Validate and convert a JS number (f64) to a u64 element ID.
/// JS numbers are f64 — lossless for integers up to 2^53.
fn to_element_id(id: f64) -> Result<u64> {
    if !id.is_finite() || id < 0.0 || id.fract() != 0.0 || id > 9_007_199_254_740_991.0 {
        return Err(Error::from_reason(format!("Invalid element id: {}", id)));
    }
    Ok(id as u64)
}

// Thread-local storage for the NodePlatform reference.
// NodePlatform contains RefCell fields (making it !Send/!Sync), but napi-rs
// requires GpuixRenderer to be Send. Since all napi methods are called from
// the JS main thread, storing the platform in a thread_local is safe.
thread_local! {
    static NODE_PLATFORM: RefCell<Option<Rc<NodePlatform>>> = const { RefCell::new(None) };
    static GPUI_WINDOW: RefCell<Option<gpui::AnyWindowHandle>> = const { RefCell::new(None) };
}

/// The main GPUI renderer exposed to Node.js.
#[napi]
pub struct GpuixRenderer {
    event_callback: Option<ThreadsafeFunction<EventPayload>>,
    tree: Arc<Mutex<RetainedTree>>,
    initialized: Arc<Mutex<bool>>,
    needs_redraw: Arc<AtomicBool>,
}

#[napi]
impl GpuixRenderer {
    #[napi(constructor)]
    pub fn new(event_callback: Option<ThreadsafeFunction<EventPayload>>) -> Self {
        let _ = env_logger::try_init();
        Self {
            event_callback,
            tree: Arc::new(Mutex::new(RetainedTree::new())),
            initialized: Arc::new(Mutex::new(false)),
            needs_redraw: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Initialize the GPUI application with a non-blocking NodePlatform.
    #[napi]
    pub fn init(&self, options: Option<WindowOptions>) -> Result<()> {
        let options = options.unwrap_or_default();

        {
            let initialized = self.initialized.lock().unwrap();
            if *initialized {
                return Err(Error::from_reason("Renderer is already initialized"));
            }
        }

        let width = options.width.unwrap_or(800.0);
        let height = options.height.unwrap_or(600.0);
        let title = options.title.clone().unwrap_or_else(|| "GPUIX".to_string());

        let platform = Rc::new(NodePlatform::new());
        NODE_PLATFORM.with(|p| {
            *p.borrow_mut() = Some(platform.clone());
        });

        let tree = self.tree.clone();
        let callback = self.event_callback.clone();

        let app = gpui::Application::with_platform(platform);
        app.run(move |cx: &mut gpui::App| {
            let bounds = gpui::Bounds::centered(
                None,
                gpui::size(gpui::px(width as f32), gpui::px(height as f32)),
                cx,
            );

            let window_handle = cx.open_window(
                gpui::WindowOptions {
                    window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_window, cx| {
                    cx.new(|_| GpuixView {
                        tree: tree.clone(),
                        event_callback: callback.clone(),
                        window_title: title,
                    })
                },
            )
            .unwrap();

            GPUI_WINDOW.with(|w| {
                *w.borrow_mut() = Some(window_handle.into());
            });

            cx.activate(true);
        });

        *self.initialized.lock().unwrap() = true;
        eprintln!("[GPUIX-RUST] init() complete — window created, non-blocking");

        Ok(())
    }

    // ── Mutation API ─────────────────────────────────────────────────

    #[napi]
    pub fn create_element(&self, id: f64, element_type: String) -> Result<()> {
        let id = to_element_id(id)?;
        let mut tree = self.tree.lock().unwrap();
        tree.create_element(id, element_type);
        Ok(())
    }

    /// Destroy an element and all descendants. Returns array of destroyed IDs
    /// so JS can clean up event handlers for the entire subtree.
    #[napi]
    pub fn destroy_element(&self, id: f64) -> Result<Vec<f64>> {
        let id = to_element_id(id)?;
        let mut tree = self.tree.lock().unwrap();
        let destroyed = tree.destroy_element(id);
        Ok(destroyed.iter().map(|&id| id as f64).collect())
    }

    #[napi]
    pub fn append_child(&self, parent_id: f64, child_id: f64) -> Result<()> {
        let parent_id = to_element_id(parent_id)?;
        let child_id = to_element_id(child_id)?;
        let mut tree = self.tree.lock().unwrap();
        tree.append_child(parent_id, child_id);
        Ok(())
    }

    #[napi]
    pub fn remove_child(&self, parent_id: f64, child_id: f64) -> Result<()> {
        let parent_id = to_element_id(parent_id)?;
        let child_id = to_element_id(child_id)?;
        let mut tree = self.tree.lock().unwrap();
        tree.remove_child(parent_id, child_id);
        Ok(())
    }

    #[napi]
    pub fn insert_before(&self, parent_id: f64, child_id: f64, before_id: f64) -> Result<()> {
        let parent_id = to_element_id(parent_id)?;
        let child_id = to_element_id(child_id)?;
        let before_id = to_element_id(before_id)?;
        let mut tree = self.tree.lock().unwrap();
        tree.insert_before(parent_id, child_id, before_id);
        Ok(())
    }

    #[napi]
    pub fn set_style(&self, id: f64, style_json: String) -> Result<()> {
        let id = to_element_id(id)?;
        let style: StyleDesc = serde_json::from_str(&style_json).map_err(|e| {
            Error::from_reason(format!("Failed to parse style: {}", e))
        })?;
        let mut tree = self.tree.lock().unwrap();
        tree.set_style(id, style);
        Ok(())
    }

    #[napi]
    pub fn set_text(&self, id: f64, content: String) -> Result<()> {
        let id = to_element_id(id)?;
        let mut tree = self.tree.lock().unwrap();
        tree.set_text(id, content);
        Ok(())
    }

    #[napi]
    pub fn set_event_listener(&self, id: f64, event_type: String, has_handler: bool) -> Result<()> {
        let id = to_element_id(id)?;
        let mut tree = self.tree.lock().unwrap();
        tree.set_event_listener(id, event_type, has_handler);
        Ok(())
    }

    /// Set the root element (called from appendChildToContainer).
    #[napi]
    pub fn set_root(&self, id: f64) -> Result<()> {
        let id = to_element_id(id)?;
        let mut tree = self.tree.lock().unwrap();
        tree.root_id = Some(id);
        Ok(())
    }

    /// Signal that a batch of mutations is complete. Triggers re-render.
    #[napi]
    pub fn commit_mutations(&self) -> Result<()> {
        self.needs_redraw.store(true, Ordering::SeqCst);
        Ok(())
    }

    // ── Frame loop ───────────────────────────────────────────────────

    #[napi]
    pub fn tick(&self) -> Result<()> {
        let initialized = *self.initialized.lock().unwrap();
        if !initialized {
            return Err(Error::from_reason("Renderer not initialized. Call init() first."));
        }

        let force_render = self.needs_redraw.swap(false, Ordering::SeqCst);

        NODE_PLATFORM.with(|p| {
            if let Some(ref platform) = *p.borrow() {
                platform.tick(force_render);
            }
        });

        Ok(())
    }

    #[napi]
    pub fn is_initialized(&self) -> bool {
        *self.initialized.lock().unwrap()
    }

    #[napi]
    pub fn get_window_size(&self) -> Result<WindowSize> {
        Ok(WindowSize { width: 800.0, height: 600.0 })
    }

    #[napi]
    pub fn set_window_title(&self, _title: String) -> Result<()> {
        Ok(())
    }

    #[napi]
    pub fn focus_element(&self, _element_id: f64) -> Result<()> {
        Ok(())
    }

    #[napi]
    pub fn blur(&self) -> Result<()> {
        Ok(())
    }
}

// ── GPUI View ────────────────────────────────────────────────────────

struct GpuixView {
    tree: Arc<Mutex<RetainedTree>>,
    event_callback: Option<ThreadsafeFunction<EventPayload>>,
    window_title: String,
}

impl gpui::Render for GpuixView {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::IntoElement;

        window.set_window_title(&self.window_title);

        let tree = self.tree.lock().unwrap();

        match tree.root_id {
            Some(root_id) => build_element(root_id, &tree, &self.event_callback),
            None => gpui::Empty.into_any_element(),
        }
    }
}

// ── Element builders ─────────────────────────────────────────────────

fn build_element(
    id: u64,
    tree: &RetainedTree,
    event_callback: &Option<ThreadsafeFunction<EventPayload>>,
) -> gpui::AnyElement {
    use gpui::IntoElement;

    let Some(element) = tree.elements.get(&id) else {
        return gpui::Empty.into_any_element();
    };

    match element.element_type.as_str() {
        "div" => build_div(element, tree, event_callback),
        "text" => build_text(element),
        _ => gpui::Empty.into_any_element(),
    }
}

fn build_div(
    element: &crate::retained_tree::RetainedElement,
    tree: &RetainedTree,
    event_callback: &Option<ThreadsafeFunction<EventPayload>>,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    let element_id_str = format!("__gpuix_{}", element.id);
    let mut el = gpui::div().id(gpui::SharedString::from(element_id_str));

    if let Some(ref style) = element.style {
        el = apply_styles(el, style);
    }

    // Wire up events
    for event_type in &element.events {
        let id = element.id;
        let callback = event_callback.clone();
        match event_type.as_str() {
            "click" => {
                el = el.on_click(move |click_event, _window, _cx| {
                    emit_event(&callback, id, "click", Some(click_event.position()));
                });
            }
            "mouseDown" => {
                el = el.on_mouse_down(gpui::MouseButton::Left, move |mouse_event, _window, _cx| {
                    emit_event(&callback, id, "mouseDown", Some(mouse_event.position));
                });
            }
            "mouseUp" => {
                el = el.on_mouse_up(gpui::MouseButton::Left, move |mouse_event, _window, _cx| {
                    emit_event(&callback, id, "mouseUp", Some(mouse_event.position));
                });
            }
            "mouseMove" => {
                el = el.on_mouse_move(move |mouse_event, _window, _cx| {
                    emit_event(&callback, id, "mouseMove", Some(mouse_event.position));
                });
            }
            _ => {}
        }
    }

    // Text content
    if let Some(ref content) = element.content {
        el = el.child(content.clone());
    }

    // Children
    for &child_id in &element.children {
        el = el.child(build_element(child_id, tree, event_callback));
    }

    el.into_any_element()
}

fn build_text(element: &crate::retained_tree::RetainedElement) -> gpui::AnyElement {
    use gpui::prelude::*;

    let content = element.content.clone().unwrap_or_default();

    if let Some(ref style) = element.style {
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

// ── Style application ────────────────────────────────────────────────

fn apply_width<E: gpui::Styled>(el: E, dim: &crate::style::DimensionValue) -> E {
    match dim {
        crate::style::DimensionValue::Pixels(v) => el.w(gpui::px(*v as f32)),
        crate::style::DimensionValue::Percentage(v) if *v >= 0.999 => el.w_full(),
        crate::style::DimensionValue::Percentage(v) => el.w(gpui::relative(*v as f32)),
        crate::style::DimensionValue::Auto => el,
    }
}

fn apply_height<E: gpui::Styled>(el: E, dim: &crate::style::DimensionValue) -> E {
    match dim {
        crate::style::DimensionValue::Pixels(v) => el.h(gpui::px(*v as f32)),
        crate::style::DimensionValue::Percentage(v) if *v >= 0.999 => el.h_full(),
        crate::style::DimensionValue::Percentage(v) => el.h(gpui::relative(*v as f32)),
        crate::style::DimensionValue::Auto => el,
    }
}

fn apply_styles<E: gpui::Styled>(mut el: E, style: &StyleDesc) -> E {
    if style.display.as_deref() == Some("flex") {
        el = el.flex();
    }
    if style.flex_direction.as_deref() == Some("column") {
        el = el.flex_col();
    }
    if style.flex_direction.as_deref() == Some("row") {
        el = el.flex_row();
    }
    if style.flex_grow.is_some() {
        el = el.flex_grow();
    }
    if style.flex_shrink.is_some() {
        el = el.flex_shrink();
    }
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
    if let Some(gap) = style.gap {
        el = el.gap(gpui::px(gap as f32));
    }
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
    if let Some(ref bg) = style.background_color.as_ref().or(style.background.as_ref()) {
        if let Some(hex) = parse_color_hex(bg) {
            el = el.bg(gpui::rgba(hex));
        }
    }
    if let Some(ref color) = style.color {
        if let Some(hex) = parse_color_hex(color) {
            el = el.text_color(gpui::rgba(hex));
        }
    }
    if let Some(radius) = style.border_radius {
        el = el.rounded(gpui::px(radius as f32));
    }
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
    if let Some(opacity) = style.opacity {
        el = el.opacity(opacity as f32);
    }
    match style.cursor.as_deref() {
        Some("pointer") => el = el.cursor_pointer(),
        Some("default") => el = el.cursor_default(),
        _ => {}
    }
    match style.overflow.as_deref() {
        Some("hidden") => el = el.overflow_hidden(),
        _ => {}
    }

    el
}

// ── Event emission ───────────────────────────────────────────────────

fn emit_event(
    callback: &Option<ThreadsafeFunction<EventPayload>>,
    element_id: u64,
    event_type: &str,
    position: Option<gpui::Point<gpui::Pixels>>,
) {
    if let Some(cb) = callback {
        let payload = EventPayload {
            element_id: element_id as f64,
            event_type: event_type.to_string(),
            x: position.map(|p| f64::from(f32::from(p.x))),
            y: position.map(|p| f64::from(f32::from(p.y))),
            key: None,
            modifiers: Some(EventModifiers::default()),
        };
        cb.call(Ok(payload), ThreadsafeFunctionCallMode::NonBlocking);
    }
}

// ── Types ────────────────────────────────────────────────────────────

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
