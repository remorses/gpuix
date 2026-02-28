/// GpuixRenderer — napi-rs binding exposed to Node.js.
///
/// This is the main entry point for JS. Instead of the old blocking run() API,
/// we now have init() + render() + tick():
///
///   renderer.init({ title: 'My App', width: 800, height: 600 })
///   renderer.render(jsonTree)          // send element tree
///   setImmediate(function loop() {     // drive the frame loop
///     renderer.tick()
///     setImmediate(loop)
///   })
///
/// init() creates a NodePlatform (non-blocking), opens a GPUI window with wgpu.
/// render() updates the element tree and notifies GPUI to re-render.
/// tick() pumps the GPUI foreground task queue and triggers frame rendering.

use gpui::AppContext as _;
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::element_tree::{ElementDesc, EventModifiers, EventPayload};
use crate::platform::NodePlatform;
use crate::style::parse_color_hex;

static ELEMENT_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

// Thread-local storage for the NodePlatform reference.
// NodePlatform contains RefCell fields (making it !Send/!Sync), but napi-rs
// requires GpuixRenderer to be Send. Since all napi methods are called from
// the JS main thread, storing the platform in a thread_local is safe and
// avoids the Arc<Mutex<Rc<...>>> impossibility.
//
// The on_quit callback registered by GPUI's Application::new_app() stores
// an Rc<AppCell> clone inside NodePlatform.callbacks.quit, which keeps the
// entire GPUI app state alive as long as this thread_local holds the platform.
thread_local! {
    static NODE_PLATFORM: RefCell<Option<Rc<NodePlatform>>> = const { RefCell::new(None) };
    // Store the GPUI window handle so render() can notify it to re-render.
    // cx.notify() is GPUI's proper invalidation mechanism — it marks the entity
    // dirty so the next frame calls Render::render(). This is better than
    // force_render which bypasses GPUI's dirty tracking.
    static GPUI_WINDOW: RefCell<Option<gpui::AnyWindowHandle>> = const { RefCell::new(None) };
}

fn generate_element_id() -> String {
    let id = ELEMENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("__gpuix_{}", id)
}

/// The main GPUI renderer exposed to Node.js.
///
/// Lifecycle:
/// 1. new GpuixRenderer(eventCallback) — creates the binding
/// 2. renderer.init({ ... }) — creates NodePlatform + window (non-blocking)
/// 3. renderer.render(json) — sends element tree to GPUI
/// 4. renderer.tick() — pumps events + renders frame (call from setImmediate loop)
#[napi]
pub struct GpuixRenderer {
    event_callback: Option<ThreadsafeFunction<EventPayload>>,
    current_tree: Arc<Mutex<Option<ElementDesc>>>,
    initialized: Arc<Mutex<bool>>,
    /// Set to true by render() when a new tree arrives, cleared by tick().
    /// Controls whether request_frame uses force_render: true.
    /// Without this, GPUI won't know the view is dirty and won't call Render::render().
    needs_redraw: Arc<AtomicBool>,
}

#[napi]
impl GpuixRenderer {
    #[napi(constructor)]
    pub fn new(event_callback: Option<ThreadsafeFunction<EventPayload>>) -> Self {
        // Initialize logging
        let _ = env_logger::try_init();

        Self {
            event_callback,
            current_tree: Arc::new(Mutex::new(None)),
            initialized: Arc::new(Mutex::new(false)),
            needs_redraw: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Initialize the GPUI application with a non-blocking NodePlatform.
    /// Creates a native window and wgpu rendering surface.
    /// This returns immediately — it does NOT block like the old run().
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

        // Create the NodePlatform
        let platform = Rc::new(NodePlatform::new());

        // Store platform reference in thread_local for tick()
        NODE_PLATFORM.with(|p| {
            *p.borrow_mut() = Some(platform.clone());
        });

        let tree = self.current_tree.clone();
        let callback = self.event_callback.clone();

        // Create the GPUI Application with our custom platform
        // Application::with_platform() + run() — run() returns immediately for NodePlatform
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
                        window_title: Arc::new(Mutex::new(Some(title))),
                    })
                },
            )
            .unwrap();

            // Store window handle for render() to notify GPUI of tree changes
            GPUI_WINDOW.with(|w| {
                *w.borrow_mut() = Some(window_handle.into());
            });

            cx.activate(true);
        });

        *self.initialized.lock().unwrap() = true;
        eprintln!("[GPUIX-RUST] init() complete — window created, non-blocking");

        Ok(())
    }

    /// Send a new element tree to GPUI. Triggers re-render on next tick().
    #[napi]
    pub fn render(&self, tree_json: String) -> Result<()> {
        let tree: ElementDesc = serde_json::from_str(&tree_json).map_err(|e| {
            Error::from_reason(format!("Failed to parse element tree: {}", e))
        })?;

        let mut current = self.current_tree.lock().unwrap();
        *current = Some(tree);

        // Signal that the tree changed — tick() will pass force_render: true
        // to the request_frame callback, making GPUI call GpuixView::render()
        self.needs_redraw.store(true, Ordering::SeqCst);

        Ok(())
    }

    /// Pump the event loop. Call this from JS on every tick (via setImmediate).
    /// Processes: OS events, GPUI foreground tasks, delayed tasks, frame rendering.
    #[napi]
    pub fn tick(&self) -> Result<()> {
        let initialized = *self.initialized.lock().unwrap();
        if !initialized {
            return Err(Error::from_reason("Renderer not initialized. Call init() first."));
        }

        // Check if render() sent a new tree — if so, force GPUI to redraw
        let force_render = self.needs_redraw.swap(false, Ordering::SeqCst);

        // Pump OS events + drain GPUI tasks + trigger frame render
        NODE_PLATFORM.with(|p| {
            if let Some(ref platform) = *p.borrow() {
                platform.tick(force_render);
            }
        });

        Ok(())
    }

    /// Check if the renderer has been initialized.
    #[napi]
    pub fn is_initialized(&self) -> bool {
        *self.initialized.lock().unwrap()
    }

    #[napi]
    pub fn get_window_size(&self) -> Result<WindowSize> {
        Ok(WindowSize {
            width: 800.0,
            height: 600.0,
        })
    }

    // Keep these for backwards compatibility during transition
    #[napi]
    pub fn set_window_title(&self, _title: String) -> Result<()> {
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
    fn render(
        &mut self,
        window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::IntoElement;

        let has_tree = self.tree.lock().unwrap().is_some();
        eprintln!("[GPUIX-RUST] GpuixView::render() called, has_tree={has_tree}");

        if let Some(title) = self.window_title.lock().unwrap().as_ref() {
            window.set_window_title(title);
        }

        let tree = self.tree.lock().unwrap();

        match tree.as_ref() {
            Some(desc) => build_element(desc, &self.event_callback),
            None => gpui::Empty.into_any_element(),
        }
    }
}

fn build_element(
    desc: &ElementDesc,
    event_callback: &Option<ThreadsafeFunction<EventPayload>>,
) -> gpui::AnyElement {
    use gpui::IntoElement;

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

    let element_id = desc.id.clone().unwrap_or_else(generate_element_id);

    // Debug: log what styles this div gets
    if let Some(ref style) = desc.style {
        if style.background_color.is_some() || style.background.is_some() {
            eprintln!("[GPUIX-RUST] build_div id={element_id} bg={:?} w={:?} h={:?} p={:?} display={:?}",
                style.background_color.as_ref().or(style.background.as_ref()),
                style.width, style.height, style.padding, style.display);
        }
    }

    let mut el = gpui::div().id(gpui::SharedString::from(element_id.clone()));

    // Apply styles
    if let Some(ref style) = desc.style {
        el = apply_styles(el, style);
    }

    // Wire up events
    if let Some(ref events) = desc.events {
        for event in events {
            match event.as_str() {
                "click" => {
                    let id = element_id.clone();
                    let callback = event_callback.clone();
                    el = el.on_click(move |click_event, _window, _cx| {
                        // Don't call cx.refresh_windows() — let JS-driven
                        // renderer.render() be the re-render trigger via tick()
                        emit_event(&callback, &id, "click", Some(click_event.position()));
                    });
                }
                "mouseDown" => {
                    let id = element_id.clone();
                    let callback = event_callback.clone();
                    el = el.on_mouse_down(
                        gpui::MouseButton::Left,
                        move |mouse_event, _window, _cx| {
                            emit_event(&callback, &id, "mouseDown", Some(mouse_event.position));
                        },
                    );
                }
                "mouseUp" => {
                    let id = element_id.clone();
                    let callback = event_callback.clone();
                    el = el.on_mouse_up(
                        gpui::MouseButton::Left,
                        move |mouse_event, _window, _cx| {
                            emit_event(&callback, &id, "mouseUp", Some(mouse_event.position));
                        },
                    );
                }
                "mouseMove" => {
                    let id = element_id.clone();
                    let callback = event_callback.clone();
                    el = el.on_mouse_move(move |mouse_event, _window, _cx| {
                        emit_event(&callback, &id, "mouseMove", Some(mouse_event.position));
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
    match dim {
        crate::style::DimensionValue::Pixels(v) => el.w(gpui::px(*v as f32)),
        // relative(1.0) = 100% of parent width
        crate::style::DimensionValue::Percentage(v) if *v >= 0.999 => el.w_full(),
        crate::style::DimensionValue::Percentage(v) => el.w(gpui::relative(*v as f32)),
        crate::style::DimensionValue::Auto => el,
    }
}

fn apply_height<E: gpui::Styled>(el: E, dim: &crate::style::DimensionValue) -> E {
    match dim {
        crate::style::DimensionValue::Pixels(v) => el.h(gpui::px(*v as f32)),
        // relative(1.0) = 100% of parent height
        crate::style::DimensionValue::Percentage(v) if *v >= 0.999 => el.h_full(),
        crate::style::DimensionValue::Percentage(v) => el.h(gpui::relative(*v as f32)),
        crate::style::DimensionValue::Auto => el,
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
            crate::style::DimensionValue::Percentage(v) => {
                el = el.min_w(gpui::relative(*v as f32))
            }
            crate::style::DimensionValue::Auto => {}
        }
    }
    if let Some(ref min_h) = style.min_height {
        match min_h {
            crate::style::DimensionValue::Pixels(v) => el = el.min_h(gpui::px(*v as f32)),
            crate::style::DimensionValue::Percentage(v) => {
                el = el.min_h(gpui::relative(*v as f32))
            }
            crate::style::DimensionValue::Auto => {}
        }
    }
    if let Some(ref max_w) = style.max_width {
        match max_w {
            crate::style::DimensionValue::Pixels(v) => el = el.max_w(gpui::px(*v as f32)),
            crate::style::DimensionValue::Percentage(v) => {
                el = el.max_w(gpui::relative(*v as f32))
            }
            crate::style::DimensionValue::Auto => {}
        }
    }
    if let Some(ref max_h) = style.max_height {
        match max_h {
            crate::style::DimensionValue::Pixels(v) => el = el.max_h(gpui::px(*v as f32)),
            crate::style::DimensionValue::Percentage(v) => {
                el = el.max_h(gpui::relative(*v as f32))
            }
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
    if let Some(ref bg) = style
        .background_color
        .as_ref()
        .or(style.background.as_ref())
    {
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
        let payload = EventPayload {
            element_id: element_id.to_string(),
            event_type: event_type.to_string(),
            x: position.map(|p| f64::from(f32::from(p.x))),
            y: position.map(|p| f64::from(f32::from(p.y))),
            key: None,
            modifiers: Some(EventModifiers::default()),
        };
        cb.call(Ok(payload), ThreadsafeFunctionCallMode::NonBlocking);
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
