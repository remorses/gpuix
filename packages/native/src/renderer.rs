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
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::custom_elements::{CustomElementRegistry, CustomRenderContext};
use crate::element_tree::EventPayload;
use crate::platform::NodePlatform;
use crate::retained_tree::RetainedTree;
use crate::style::{parse_color_hex, StyleDesc};

/// Parse a CSS font-weight value (string or number) into a GPUI FontWeight.
/// Accepts named keywords ("bold", "semibold"), numeric strings ("700"),
/// and raw numbers (700). Falls back to 400 (normal) for unrecognized values.
fn parse_font_weight(value: &crate::style::FontWeightValue) -> gpui::FontWeight {
    match value {
        crate::style::FontWeightValue::Num(n) => {
            gpui::FontWeight((*n as f32).clamp(1.0, 1000.0))
        }
        crate::style::FontWeightValue::Str(s) => {
            let lower = s.trim().to_ascii_lowercase();
            match lower.as_str() {
                "100" | "thin" => gpui::FontWeight(100.0),
                "200" | "extralight" | "extra-light" => gpui::FontWeight(200.0),
                "300" | "light" => gpui::FontWeight(300.0),
                "400" | "normal" => gpui::FontWeight(400.0),
                "500" | "medium" => gpui::FontWeight(500.0),
                "600" | "semibold" | "semi-bold" => gpui::FontWeight(600.0),
                "700" | "bold" => gpui::FontWeight(700.0),
                "800" | "extrabold" | "extra-bold" => gpui::FontWeight(800.0),
                "900" | "black" => gpui::FontWeight(900.0),
                _ => lower
                    .parse::<f32>()
                    .map(|n| gpui::FontWeight(n.clamp(1.0, 1000.0)))
                    .unwrap_or(gpui::FontWeight(400.0)),
            }
        }
    }
}

/// Abstracted event callback — both production and test renderers use this.
/// Production: wraps ThreadsafeFunction (async, queued on Node.js event loop).
/// Tests: wraps Arc<Mutex<Vec<EventPayload>>> (synchronous collection).
pub(crate) type EventCallback = Arc<dyn Fn(EventPayload) + Send + Sync>;

/// Validate and convert a JS number (f64) to a u64 element ID.
/// JS numbers are f64 — lossless for integers up to 2^53.
pub(crate) fn to_element_id(id: f64) -> Result<u64> {
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
    /// Shared scroll handles — GpuixView writes here during render(),
    /// napi methods read from here for programmatic scroll control.
    /// ScrollHandle is Rc<RefCell<...>> so its methods (set_offset, offset,
    /// scroll_to_item) work without an App context.
    ///
    /// NOTE: This is a singleton — if multiple renderers/windows coexist,
    /// the last one to render wins. Acceptable for now (single-window only).
    /// TODO: Scope by renderer/window ID when multi-window support is added.
    static SCROLL_HANDLES: RefCell<HashMap<u64, gpui::ScrollHandle>> = RefCell::new(HashMap::new());
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
        // Wrap ThreadsafeFunction in Arc so GpuixView uses the abstracted EventCallback.
        let callback: Option<EventCallback> = self.event_callback.as_ref().map(|tsf| {
            let tsf = tsf.clone();
            Arc::new(move |payload: EventPayload| {
                tsf.call(Ok(payload), ThreadsafeFunctionCallMode::NonBlocking);
            }) as EventCallback
        });

        let app = gpui::Application::with_platform(platform);
        app.run(move |cx: &mut gpui::App| {
            let bounds = gpui::Bounds::centered(
                None,
                gpui::size(gpui::px(width as f32), gpui::px(height as f32)),
                cx,
            );

            let window_handle = cx
                .open_window(
                    gpui::WindowOptions {
                        window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
                        ..Default::default()
                    },
                    |_window, cx| {
                        cx.new(|_| GpuixView {
                            tree: tree.clone(),
                            event_callback: callback.clone(),
                            window_title: title,
                            focus_handles: HashMap::new(),
                            _focus_subscriptions: Vec::new(),
                            custom_registry: CustomElementRegistry::with_defaults(),
                            scroll_handles: HashMap::new(),
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
        let style: StyleDesc = serde_json::from_str(&style_json)
            .map_err(|e| Error::from_reason(format!("Failed to parse style: {}", e)))?;
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

    /// Set a custom prop on an element (for non-div/text elements like input, editor, diff).
    /// Key is the prop name, value is JSON-encoded.
    #[napi]
    pub fn set_custom_prop(&self, id: f64, key: String, value_json: String) -> Result<()> {
        let id = to_element_id(id)?;
        let value: serde_json::Value = serde_json::from_str(&value_json)
            .map_err(|e| Error::from_reason(format!("Failed to parse custom prop value: {}", e)))?;
        let mut tree = self.tree.lock().unwrap();
        tree.set_custom_prop(id, key, value);
        Ok(())
    }

    /// Get a custom prop value from an element. Returns JSON string or null.
    #[napi]
    pub fn get_custom_prop(&self, id: f64, key: String) -> Result<Option<String>> {
        let id = to_element_id(id)?;
        let tree = self.tree.lock().unwrap();
        Ok(tree
            .get_custom_prop(id, &key)
            .map(|v| serde_json::to_string(v).unwrap_or_default()))
    }

    /// Signal that a batch of mutations is complete. Triggers re-render.
    #[napi]
    pub fn commit_mutations(&self) -> Result<()> {
        self.needs_redraw.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Apply a batch of mutations in a single FFI call.
    ///
    /// Accepts a JSON array of mutation tuples. Each tuple is an array where
    /// the first element is the operation name (string) and remaining elements
    /// are the arguments:
    ///
    ///   ["createElement",    id, "type"]
    ///   ["destroyElement",   id]
    ///   ["appendChild",      parentId, childId]
    ///   ["removeChild",      parentId, childId]
    ///   ["insertBefore",     parentId, childId, beforeId]
    ///   ["setStyle",         id, "{styleJson}"]
    ///   ["setText",          id, "content"]
    ///   ["setEventListener", id, "eventType", true|false]
    ///   ["setRoot",          id]
    ///   ["setCustomProp",    id, "key", "{valueJson}"]
    ///
    /// Returns accumulated destroyed IDs from all destroyElement ops.
    /// Acquires the tree mutex ONCE for the entire batch.
    #[napi]
    pub fn apply_batch(&self, json: String) -> Result<Vec<f64>> {
        let ops: Vec<serde_json::Value> = serde_json::from_str(&json)
            .map_err(|e| Error::from_reason(format!("Failed to parse batch: {}", e)))?;
        let mut tree = self.tree.lock().unwrap();
        let destroyed = apply_batch_to_tree(&mut tree, &ops)?;
        self.needs_redraw.store(true, Ordering::SeqCst);
        Ok(destroyed)
    }

    // ── Frame loop ───────────────────────────────────────────────────

    #[napi]
    pub fn tick(&self) -> Result<()> {
        let initialized = *self.initialized.lock().unwrap();
        if !initialized {
            return Err(Error::from_reason(
                "Renderer not initialized. Call init() first.",
            ));
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
        Ok(WindowSize {
            width: 800.0,
            height: 600.0,
        })
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

    // ── Scroll API ───────────────────────────────────────────────────
    // ScrollHandle is Rc<RefCell<...>> — its methods work without an App context.
    // GpuixView syncs handles to the SCROLL_HANDLES thread_local on each render.

    /// Set the scroll offset of a scrollable element.
    /// x and y are negative pixel values (scroll down = more negative y).
    #[napi]
    pub fn scroll_to(&self, element_id: f64, x: f64, y: f64) -> Result<()> {
        let id = to_element_id(element_id)?;
        SCROLL_HANDLES.with(|cell| {
            let handles = cell.borrow();
            if let Some(handle) = handles.get(&id) {
                handle.set_offset(gpui::point(gpui::px(x as f32), gpui::px(y as f32)));
            }
        });
        Ok(())
    }

    /// Scroll a child into view by its index in the children list.
    #[napi]
    pub fn scroll_to_item(&self, element_id: f64, index: f64) -> Result<()> {
        let id = to_element_id(element_id)?;
        let index = index as usize;
        SCROLL_HANDLES.with(|cell| {
            let handles = cell.borrow();
            if let Some(handle) = handles.get(&id) {
                handle.scroll_to_item(index);
            }
        });
        Ok(())
    }

    /// Get the current scroll offset of a scrollable element.
    /// Returns [x, y] or null if the element has no scroll handle.
    #[napi]
    pub fn get_scroll_offset(&self, element_id: f64) -> Result<Option<Vec<f64>>> {
        let id = to_element_id(element_id)?;
        Ok(SCROLL_HANDLES.with(|cell| {
            let handles = cell.borrow();
            handles.get(&id).map(|handle| {
                let offset = handle.offset();
                vec![
                    f64::from(f32::from(offset.x)),
                    f64::from(f32::from(offset.y)),
                ]
            })
        }))
    }
}

// ── GPUI View ────────────────────────────────────────────────────────

pub(crate) struct GpuixView {
    pub(crate) tree: Arc<Mutex<RetainedTree>>,
    pub(crate) event_callback: Option<EventCallback>,
    pub(crate) window_title: String,
    /// Persistent FocusHandles keyed by element ID.
    /// Created lazily for elements with keyboard or focus/blur listeners.
    /// Handles persist across renders so GPUI maintains focus state.
    pub(crate) focus_handles: HashMap<u64, gpui::FocusHandle>,
    /// Keep subscriptions alive — dropping them unsubscribes.
    pub(crate) _focus_subscriptions: Vec<gpui::Subscription>,
    /// Registry for custom element types (input, editor, diff, etc.).
    /// Stores factories (one per type) and live instances (one per element ID).
    pub(crate) custom_registry: CustomElementRegistry,
    /// Persistent ScrollHandles keyed by element ID.
    /// Created lazily for elements with overflow: "scroll" (or per-axis scroll).
    /// Handles persist across renders so GPUI maintains scroll offset state.
    pub(crate) scroll_handles: HashMap<u64, gpui::ScrollHandle>,
}

impl GpuixView {
    /// Sync focus handles with the current element tree.
    /// Creates handles for new focusable elements, subscribes on_focus/on_blur,
    /// and cleans up handles for destroyed elements.
    fn sync_focus_handles(
        &mut self,
        tree: &RetainedTree,
        callback: &Option<EventCallback>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // Create handles for elements that need focus but don't have one yet.
        for (&id, element) in &tree.elements {
            let needs_focus = element.events.contains("keyDown")
                || element.events.contains("keyUp")
                || element.events.contains("focus")
                || element.events.contains("blur");

            if needs_focus && !self.focus_handles.contains_key(&id) {
                let handle = cx.focus_handle();

                // Subscribe to focus events if listeners exist.
                if element.events.contains("focus") {
                    let cb = callback.clone();
                    self._focus_subscriptions.push(cx.on_focus(
                        &handle,
                        window,
                        move |_this, _window, _cx| {
                            emit_event_full(&cb, id, "focus", |_| {});
                        },
                    ));
                }
                if element.events.contains("blur") {
                    let cb = callback.clone();
                    self._focus_subscriptions.push(cx.on_blur(
                        &handle,
                        window,
                        move |_this, _window, _cx| {
                            emit_event_full(&cb, id, "blur", |_| {});
                        },
                    ));
                }

                self.focus_handles.insert(id, handle);
            }
        }

        // Clean up handles for elements that no longer exist.
        self.focus_handles
            .retain(|id, _| tree.elements.contains_key(id));
    }
}

impl gpui::Render for GpuixView {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::IntoElement;

        window.set_window_title(&self.window_title);

        // Clone Arc so we don't borrow self.tree — frees self for focus_handles access.
        let tree_arc = self.tree.clone();
        let tree = tree_arc.lock().unwrap();
        let callback = self.event_callback.clone();

        // Sync focus handles before building elements.
        self.sync_focus_handles(&tree, &callback, window, cx);

        // Ensure custom element instances are destroyed when their IDs disappear.
        self.custom_registry
            .prune_missing(|id| tree.elements.contains_key(&id));

        // Clean up scroll handles for destroyed elements (IDs removed from tree).
        // Scrollability-based cleanup (element still exists but style changed
        // from scroll to non-scroll) is handled inside build_div().
        self.scroll_handles
            .retain(|id, _| tree.elements.contains_key(id));

        // Build the element tree. custom_registry, focus_handles, and scroll_handles
        // are different fields of self, so Rust allows borrowing all simultaneously.
        let result = match tree.root_id {
            Some(root_id) => build_element(
                root_id,
                &tree,
                &callback,
                &self.focus_handles,
                &mut self.scroll_handles,
                &mut self.custom_registry,
                window,
                cx,
            ),
            None => gpui::Empty.into_any_element(),
        };

        // Sync scroll handles to thread_local so napi methods (scrollTo,
        // getScrollOffset) can access them without an App context.
        SCROLL_HANDLES.with(|cell| {
            let mut handles = cell.borrow_mut();
            handles.clear();
            for (&id, handle) in &self.scroll_handles {
                handles.insert(id, handle.clone());
            }
        });

        result
    }
}

// ── Element builders ─────────────────────────────────────────────────

pub(crate) fn build_element(
    id: u64,
    tree: &RetainedTree,
    event_callback: &Option<EventCallback>,
    focus_handles: &HashMap<u64, gpui::FocusHandle>,
    scroll_handles: &mut HashMap<u64, gpui::ScrollHandle>,
    custom_registry: &mut CustomElementRegistry,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    use gpui::IntoElement;

    let Some(element) = tree.elements.get(&id) else {
        return gpui::Empty.into_any_element();
    };

    match element.element_type.as_str() {
        "div" => {
            custom_registry.destroy(id);
            build_div(
                element,
                tree,
                event_callback,
                focus_handles,
                scroll_handles,
                custom_registry,
                window,
                cx,
            )
        }
        "text" => {
            custom_registry.destroy(id);
            build_text(
                element,
                tree,
                event_callback,
                focus_handles,
                scroll_handles,
                custom_registry,
                window,
                cx,
            )
        }

        // Polymorphic dispatch for all custom elements.
        custom_type => {
            let custom_children: Vec<gpui::AnyElement> = element
                .children
                .iter()
                .filter(|child_id| tree.elements.contains_key(child_id))
                .map(|child_id| {
                    build_element(
                        *child_id,
                        tree,
                        event_callback,
                        focus_handles,
                        scroll_handles,
                        custom_registry,
                        window,
                        cx,
                    )
                })
                .collect();

            if let Some(instance) = custom_registry.get_or_create(id, custom_type) {
                // Sync known props from RetainedElement to the CustomElement instance.
                // Missing keys are explicitly reset with null to avoid stale state.
                let supported_props: Vec<String> = instance
                    .supported_props()
                    .iter()
                    .map(|key| (*key).to_string())
                    .collect();

                for key in &supported_props {
                    let value = element
                        .custom_props
                        .get(key)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    instance.set_prop(key, value);
                }

                // Also pass through unknown props for forward compatibility.
                for (key, value) in &element.custom_props {
                    if !supported_props.iter().any(|known| known == key) {
                        instance.set_prop(key, value.clone());
                    }
                }

                // Only pass events the custom element declares support for.
                let supported_events: Vec<String> = instance
                    .supported_events()
                    .iter()
                    .map(|event| (*event).to_string())
                    .collect();
                let filtered_events: HashSet<String> = element
                    .events
                    .iter()
                    .filter(|event| supported_events.iter().any(|supported| supported == *event))
                    .cloned()
                    .collect();

                let ctx = CustomRenderContext {
                    id,
                    events: &filtered_events,
                    event_callback,
                    focus_handle: focus_handles.get(&id),
                    style: element.style.as_ref(),
                    children: custom_children,
                };

                instance.render(ctx, window, cx)
            } else {
                log::warn!("Unknown element type: {}", custom_type);
                gpui::Empty.into_any_element()
            }
        }
    }
}

pub(crate) fn build_div(
    element: &crate::retained_tree::RetainedElement,
    tree: &RetainedTree,
    event_callback: &Option<EventCallback>,
    focus_handles: &HashMap<u64, gpui::FocusHandle>,
    scroll_handles: &mut HashMap<u64, gpui::ScrollHandle>,
    custom_registry: &mut CustomElementRegistry,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    let element_id_str = format!("__gpuix_{}", element.id);
    let mut el = gpui::div().id(gpui::SharedString::from(element_id_str));

    if let Some(ref style) = element.style {
        el = apply_styles(el, style);
    }

    // ── Overflow: scroll ─────────────────────────────────────────────
    // overflow_scroll() requires StatefulInteractiveElement (only on Stateful<Div>),
    // so we handle it here rather than in apply_styles (which takes E: Styled).
    //
    // CSS precedence: axis-specific props (overflowX/Y) override the shorthand
    // (overflow). E.g. { overflow: "scroll", overflowY: "hidden" } → scroll X only.
    if let Some(ref style) = element.style {
        // Resolve each axis: axis-specific overrides shorthand.
        let resolved_x = style
            .overflow_x
            .as_deref()
            .or(style.overflow.as_deref());
        let resolved_y = style
            .overflow_y
            .as_deref()
            .or(style.overflow.as_deref());

        let needs_scroll_x = resolved_x == Some("scroll");
        let needs_scroll_y = resolved_y == Some("scroll");

        if needs_scroll_x && needs_scroll_y {
            el = el.overflow_scroll();
        } else if needs_scroll_x {
            el = el.overflow_x_scroll();
        } else if needs_scroll_y {
            el = el.overflow_y_scroll();
        }

        // Attach a persistent ScrollHandle when scrolling is enabled.
        // The handle persists across renders (stored in GpuixView::scroll_handles)
        // so GPUI maintains the scroll offset between frames.
        if needs_scroll_x || needs_scroll_y {
            let handle = scroll_handles
                .entry(element.id)
                .or_insert_with(gpui::ScrollHandle::new);
            el = el.track_scroll(handle);
        } else {
            // Element is no longer scrollable — remove stale handle.
            scroll_handles.remove(&element.id);
        }
    } else {
        // No style at all — remove stale handle if it existed.
        scroll_handles.remove(&element.id);
    }

    // If a FocusHandle was pre-created for this element (by sync_focus_handles),
    // attach it via track_focus. This makes the element focusable — clicking it
    // or tabbing to it gives it keyboard focus. The handle persists across renders
    // because it's stored in GpuixView::focus_handles.
    if let Some(handle) = focus_handles.get(&element.id) {
        el = el.track_focus(handle);
    }

    // Wire up events.
    // Some events (on_hover, on_click) require a stateful element (.id()),
    // which we already set above. Others (on_mouse_down, on_key_down) work
    // on any InteractiveElement.
    for event_type in &element.events {
        let id = element.id;
        let callback = event_callback.clone();
        match event_type.as_str() {
            // ── Click ────────────────────────────────────────────
            "click" => {
                el = el.on_click(move |click_event, _window, _cx| {
                    emit_event_full(&callback, id, "click", |p| {
                        let (x, y) = point_to_xy(click_event.position());
                        p.x = Some(x);
                        p.y = Some(y);
                        p.modifiers = Some(click_event.modifiers().into());
                        p.click_count = Some(click_event.click_count() as u32);
                        p.is_right_click = Some(click_event.is_right_click());
                    });
                });
            }

            // ── Mouse down (all buttons) ─────────────────────────
            "mouseDown" => {
                // Wire all three buttons so JS gets right-click, middle-click, etc.
                for &button in &[
                    gpui::MouseButton::Left,
                    gpui::MouseButton::Middle,
                    gpui::MouseButton::Right,
                ] {
                    let callback = callback.clone();
                    el = el.on_mouse_down(button, move |mouse_event, _window, _cx| {
                        emit_event_full(&callback, id, "mouseDown", |p| {
                            let (x, y) = point_to_xy(mouse_event.position);
                            p.x = Some(x);
                            p.y = Some(y);
                            p.button = Some(mouse_button_to_u32(mouse_event.button));
                            p.click_count = Some(mouse_event.click_count as u32);
                            p.modifiers = Some(mouse_event.modifiers.into());
                        });
                    });
                }
            }

            // ── Mouse up (all buttons) ───────────────────────────
            "mouseUp" => {
                for &button in &[
                    gpui::MouseButton::Left,
                    gpui::MouseButton::Middle,
                    gpui::MouseButton::Right,
                ] {
                    let callback = callback.clone();
                    el = el.on_mouse_up(button, move |mouse_event, _window, _cx| {
                        emit_event_full(&callback, id, "mouseUp", |p| {
                            let (x, y) = point_to_xy(mouse_event.position);
                            p.x = Some(x);
                            p.y = Some(y);
                            p.button = Some(mouse_button_to_u32(mouse_event.button));
                            p.click_count = Some(mouse_event.click_count as u32);
                            p.modifiers = Some(mouse_event.modifiers.into());
                        });
                    });
                }
            }

            // ── Mouse move ───────────────────────────────────────
            "mouseMove" => {
                el = el.on_mouse_move(move |mouse_event, _window, _cx| {
                    emit_event_full(&callback, id, "mouseMove", |p| {
                        let (x, y) = point_to_xy(mouse_event.position);
                        p.x = Some(x);
                        p.y = Some(y);
                        p.modifiers = Some(mouse_event.modifiers.into());
                        p.pressed_button = mouse_event.pressed_button.map(mouse_button_to_u32);
                    });
                });
            }

            // ── Hover (mouseEnter + mouseLeave) ──────────────────
            // GPUI's on_hover fires with true on enter, false on leave.
            // We split into two distinct event types for the React side.
            "mouseEnter" | "mouseLeave" => {
                // Only wire once even if both mouseEnter and mouseLeave are registered.
                // Check if we already wired on_hover via the other event.
                let has_enter = element.events.contains("mouseEnter");
                let has_leave = element.events.contains("mouseLeave");
                // Wire on first encounter (mouseEnter sorts before mouseLeave).
                if event_type.as_str() == "mouseEnter" || !has_enter {
                    let callback_enter = if has_enter {
                        event_callback.clone()
                    } else {
                        None
                    };
                    let callback_leave = if has_leave {
                        event_callback.clone()
                    } else {
                        None
                    };
                    el = el.on_hover(move |&is_hovered, _window, _cx| {
                        if is_hovered {
                            emit_event_full(&callback_enter, id, "mouseEnter", |p| {
                                p.hovered = Some(true);
                            });
                        } else {
                            emit_event_full(&callback_leave, id, "mouseLeave", |p| {
                                p.hovered = Some(false);
                            });
                        }
                    });
                }
            }

            // ── Mouse down outside ───────────────────────────────
            // Fires when the user clicks OUTSIDE this element.
            // Critical for "click outside to close" pattern (dropdowns, modals).
            "mouseDownOutside" => {
                el = el.on_mouse_down_out(move |mouse_event, _window, _cx| {
                    emit_event_full(&callback, id, "mouseDownOutside", |p| {
                        let (x, y) = point_to_xy(mouse_event.position);
                        p.x = Some(x);
                        p.y = Some(y);
                        p.button = Some(mouse_button_to_u32(mouse_event.button));
                        p.modifiers = Some(mouse_event.modifiers.into());
                    });
                });
            }

            // ── Scroll wheel ─────────────────────────────────────
            "scroll" => {
                el = el.on_scroll_wheel(move |scroll_event, _window, _cx| {
                    emit_event_full(&callback, id, "scroll", |p| {
                        let (x, y) = point_to_xy(scroll_event.position);
                        p.x = Some(x);
                        p.y = Some(y);
                        p.modifiers = Some(scroll_event.modifiers.into());
                        p.precise = Some(scroll_event.delta.precise());

                        // Convert ScrollDelta to pixel values.
                        // For Lines delta, we use a default line height of 20px.
                        let line_height = gpui::px(20.0);
                        let pixel_delta = scroll_event.delta.pixel_delta(line_height);
                        p.delta_x = Some(f64::from(f32::from(pixel_delta.x)));
                        p.delta_y = Some(f64::from(f32::from(pixel_delta.y)));

                        p.touch_phase = Some(match scroll_event.touch_phase {
                            gpui::TouchPhase::Started => "started".to_string(),
                            gpui::TouchPhase::Moved => "moved".to_string(),
                            gpui::TouchPhase::Ended => "ended".to_string(),
                        });
                    });
                });
            }

            // ── Key down ─────────────────────────────────────────
            // Requires .focusable() (set above). Element must be focused
            // (clicked or tabbed to) for these to fire.
            "keyDown" => {
                el = el.on_key_down(move |key_event, _window, _cx| {
                    emit_event_full(&callback, id, "keyDown", |p| {
                        p.key = Some(key_event.keystroke.key.clone());
                        p.key_char = key_event.keystroke.key_char.clone();
                        p.is_held = Some(key_event.is_held);
                        p.modifiers = Some(key_event.keystroke.modifiers.into());
                    });
                });
            }

            // ── Key up ───────────────────────────────────────────
            "keyUp" => {
                el = el.on_key_up(move |key_event, _window, _cx| {
                    emit_event_full(&callback, id, "keyUp", |p| {
                        p.key = Some(key_event.keystroke.key.clone());
                        p.key_char = key_event.keystroke.key_char.clone();
                        p.modifiers = Some(key_event.keystroke.modifiers.into());
                    });
                });
            }

            // ── Focus / Blur ─────────────────────────────────────
            // Event emission is handled by FocusHandle subscriptions
            // set up in GpuixView::sync_focus_handles(). The handle is
            // attached to this element via .track_focus() above.
            "focus" | "blur" => {}

            _ => {}
        }
    }

    // Text content
    if let Some(ref content) = element.content {
        el = el.child(content.clone());
    }

    // Children
    for &child_id in &element.children {
        el = el.child(build_element(
            child_id,
            tree,
            event_callback,
            focus_handles,
            scroll_handles,
            custom_registry,
            window,
            cx,
        ));
    }

    el.into_any_element()
}

pub(crate) fn build_text(
    element: &crate::retained_tree::RetainedElement,
    tree: &RetainedTree,
    event_callback: &Option<EventCallback>,
    focus_handles: &HashMap<u64, gpui::FocusHandle>,
    scroll_handles: &mut HashMap<u64, gpui::ScrollHandle>,
    custom_registry: &mut CustomElementRegistry,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    // Fast path: plain text leaf without style.
    if element.style.is_none() && element.children.is_empty() {
        return element
            .content
            .clone()
            .unwrap_or_default()
            .into_any_element();
    }

    let mut el = gpui::div();

    if let Some(ref style) = element.style {
        if let Some(hex) = style.color.as_ref().and_then(|c| parse_color_hex(c)) {
            el = el.text_color(gpui::rgba(hex));
        }
        if let Some(size) = style.font_size {
            el = el.text_size(gpui::px(size as f32));
        }
        if let Some(ref bg) = style.background_color.as_ref().or(style.background.as_ref()) {
            if let Some(hex) = parse_color_hex(bg) {
                el = el.bg(gpui::rgba(hex));
            }
        }
        if let Some(ref family) = style.font_family {
            el = el.font_family(family.clone());
        }
        if let Some(ref weight) = style.font_weight {
            el = el.font_weight(parse_font_weight(weight));
        }
        match style.white_space.as_deref() {
            Some("nowrap") => el = el.whitespace_nowrap(),
            Some("normal") => el = el.whitespace_normal(),
            _ => {}
        }
        match style.text_overflow.as_deref() {
            Some("ellipsis") => el = el.text_ellipsis(),
            Some("ellipsis-start") => el = el.text_ellipsis_start(),
            _ => {}
        }
        if let Some(clamp) = style.line_clamp {
            if clamp >= 1.0 {
                el = el.line_clamp(clamp as usize);
            }
        }
    }

    if let Some(ref content) = element.content {
        el = el.child(content.clone());
    }

    for &child_id in &element.children {
        el = el.child(build_element(
            child_id,
            tree,
            event_callback,
            focus_handles,
            scroll_handles,
            custom_registry,
            window,
            cx,
        ));
    }

    el.into_any_element()
}

// ── Style application ────────────────────────────────────────────────

pub(crate) fn apply_width<E: gpui::Styled>(el: E, dim: &crate::style::DimensionValue) -> E {
    match dim {
        crate::style::DimensionValue::Pixels(v) => el.w(gpui::px(*v as f32)),
        crate::style::DimensionValue::Percentage(v) if *v >= 0.999 => el.w_full(),
        crate::style::DimensionValue::Percentage(v) => el.w(gpui::relative(*v as f32)),
        crate::style::DimensionValue::Auto => el,
    }
}

pub(crate) fn apply_height<E: gpui::Styled>(el: E, dim: &crate::style::DimensionValue) -> E {
    match dim {
        crate::style::DimensionValue::Pixels(v) => el.h(gpui::px(*v as f32)),
        crate::style::DimensionValue::Percentage(v) if *v >= 0.999 => el.h_full(),
        crate::style::DimensionValue::Percentage(v) => el.h(gpui::relative(*v as f32)),
        crate::style::DimensionValue::Auto => el,
    }
}

pub(crate) fn apply_styles<E: gpui::Styled>(mut el: E, style: &StyleDesc) -> E {
    if style.display.as_deref() == Some("flex") {
        el = el.flex();
    }
    if style.flex_direction.as_deref() == Some("column") {
        el = el.flex_col();
    }
    if style.flex_direction.as_deref() == Some("row") {
        el = el.flex_row();
    }
    match style.flex_wrap.as_deref() {
        Some("wrap") => el = el.flex_wrap(),
        Some("wrap-reverse") => el = el.flex_wrap_reverse(),
        Some("nowrap") => el = el.flex_nowrap(),
        _ => {}
    }
    if let Some(grow) = style.flex_grow {
        el.style().flex_grow = Some(grow as f32);
    }
    if let Some(shrink) = style.flex_shrink {
        el.style().flex_shrink = Some(shrink as f32);
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
    match style.align_self.as_deref() {
        Some("center") => { el.style().align_self = Some(gpui::AlignItems::Center); }
        Some("start") | Some("flex-start") => { el.style().align_self = Some(gpui::AlignItems::FlexStart); }
        Some("end") | Some("flex-end") => { el.style().align_self = Some(gpui::AlignItems::FlexEnd); }
        Some("stretch") => { el.style().align_self = Some(gpui::AlignItems::Stretch); }
        Some("baseline") => { el.style().align_self = Some(gpui::AlignItems::Baseline); }
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
    match style.position.as_deref() {
        Some("absolute") => el = el.absolute(),
        Some("relative") => el = el.relative(),
        _ => {}
    }
    if let Some(top) = style.top {
        el = el.top(gpui::px(top as f32));
    }
    if let Some(right) = style.right {
        el = el.right(gpui::px(right as f32));
    }
    if let Some(bottom) = style.bottom {
        el = el.bottom(gpui::px(bottom as f32));
    }
    if let Some(left) = style.left {
        el = el.left(gpui::px(left as f32));
    }
    if let Some(ref bg) = style
        .background_color
        .as_ref()
        .or(style.background.as_ref())
    {
        if let Some(hex) = parse_color_hex(bg) {
            el = el.bg(gpui::rgba(hex));
        }
    }
    if let Some(ref color) = style.color {
        if let Some(hex) = parse_color_hex(color) {
            el = el.text_color(gpui::rgba(hex));
        }
    }
    if let Some(ref family) = style.font_family {
        el = el.font_family(family.clone());
    }
    if let Some(ref weight) = style.font_weight {
        el = el.font_weight(parse_font_weight(weight));
    }
    match style.white_space.as_deref() {
        Some("nowrap") => el = el.whitespace_nowrap(),
        Some("normal") => el = el.whitespace_normal(),
        _ => {}
    }
    match style.text_overflow.as_deref() {
        Some("ellipsis") => el = el.text_ellipsis(),
        Some("ellipsis-start") => el = el.text_ellipsis_start(),
        _ => {}
    }
    if let Some(clamp) = style.line_clamp {
        if clamp >= 1.0 {
            el = el.line_clamp(clamp as usize);
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
    // Overflow: hidden is on the Styled trait, so we handle it here.
    // overflow: "scroll" requires StatefulInteractiveElement — handled in build_div().
    // CSS precedence: axis-specific (overflowX/Y) overrides the shorthand (overflow).
    {
        let resolved_x = style.overflow_x.as_deref().or(style.overflow.as_deref());
        let resolved_y = style.overflow_y.as_deref().or(style.overflow.as_deref());
        // Only apply hidden here — scroll is handled in build_div.
        if resolved_x == Some("hidden") && resolved_y == Some("hidden") {
            el = el.overflow_hidden();
        } else if resolved_x == Some("hidden") {
            el = el.overflow_x_hidden();
        } else if resolved_y == Some("hidden") {
            el = el.overflow_y_hidden();
        }
    }

    el
}

// ── Event emission ───────────────────────────────────────────────────

/// Helper to convert a GPUI Point<Pixels> to (f64, f64).
pub(crate) fn point_to_xy(p: gpui::Point<gpui::Pixels>) -> (f64, f64) {
    (f64::from(f32::from(p.x)), f64::from(f32::from(p.y)))
}

/// Convert GPUI MouseButton to our u32 encoding: 0=left, 1=middle, 2=right.
pub(crate) fn mouse_button_to_u32(button: gpui::MouseButton) -> u32 {
    match button {
        gpui::MouseButton::Left => 0,
        gpui::MouseButton::Middle => 1,
        gpui::MouseButton::Right => 2,
        gpui::MouseButton::Navigate(_) => 3,
    }
}

/// General-purpose event emitter. Builds a default EventPayload, lets the
/// caller customize it via a closure, then sends it through the callback.
/// Production: queues on Node.js event loop via ThreadsafeFunction.
/// Tests: pushes to a synchronous Vec for drainEvents().
pub(crate) fn emit_event_full(
    callback: &Option<EventCallback>,
    element_id: u64,
    event_type: &str,
    build: impl FnOnce(&mut EventPayload),
) {
    if let Some(cb) = callback {
        let mut payload = EventPayload {
            element_id: element_id as f64,
            event_type: event_type.to_string(),
            ..Default::default()
        };
        build(&mut payload);
        cb(payload);
    }
}

// ── Batch processing ─────────────────────────────────────────────

/// Parsed batch operation — typed enum for atomic validation.
/// All ops are parsed and validated BEFORE any tree mutation occurs.
/// This prevents partial application on malformed batches.
enum BatchOp {
    CreateElement { id: u64, element_type: String },
    DestroyElement { id: u64 },
    AppendChild { parent_id: u64, child_id: u64 },
    RemoveChild { parent_id: u64, child_id: u64 },
    InsertBefore { parent_id: u64, child_id: u64, before_id: u64 },
    SetStyle { id: u64, style: StyleDesc },
    SetText { id: u64, content: String },
    SetEventListener { id: u64, event_type: String, has_handler: bool },
    SetRoot { id: u64 },
    SetCustomProp { id: u64, key: String, value: serde_json::Value },
}

/// Parse all batch ops from JSON into typed enums.
/// Returns Err on the first invalid op — no tree mutation has occurred yet.
fn parse_batch_ops(ops: &[serde_json::Value]) -> Result<Vec<BatchOp>> {
    let mut parsed = Vec::with_capacity(ops.len());

    for (i, op) in ops.iter().enumerate() {
        let arr = op
            .as_array()
            .ok_or_else(|| Error::from_reason(format!("Batch op {} is not an array", i)))?;
        let op_name = arr
            .first()
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::from_reason(format!("Batch op {} missing op name string", i)))?;

        let batch_op = match op_name {
            "createElement" => BatchOp::CreateElement {
                id: batch_id(arr, 1, i)?,
                element_type: batch_str(arr, 2, i)?,
            },
            "destroyElement" => BatchOp::DestroyElement {
                id: batch_id(arr, 1, i)?,
            },
            "appendChild" => BatchOp::AppendChild {
                parent_id: batch_id(arr, 1, i)?,
                child_id: batch_id(arr, 2, i)?,
            },
            "removeChild" => BatchOp::RemoveChild {
                parent_id: batch_id(arr, 1, i)?,
                child_id: batch_id(arr, 2, i)?,
            },
            "insertBefore" => BatchOp::InsertBefore {
                parent_id: batch_id(arr, 1, i)?,
                child_id: batch_id(arr, 2, i)?,
                before_id: batch_id(arr, 3, i)?,
            },
            "setStyle" => {
                let style_json = batch_str(arr, 2, i)?;
                let style: StyleDesc = serde_json::from_str(&style_json).map_err(|e| {
                    Error::from_reason(format!("Batch op {} setStyle parse error: {}", i, e))
                })?;
                BatchOp::SetStyle {
                    id: batch_id(arr, 1, i)?,
                    style,
                }
            }
            "setText" => BatchOp::SetText {
                id: batch_id(arr, 1, i)?,
                content: batch_str(arr, 2, i)?,
            },
            "setEventListener" => {
                let has_handler = arr
                    .get(3)
                    .and_then(|v| v.as_bool().or_else(|| v.as_u64().map(|n| n != 0)))
                    .ok_or_else(|| {
                        Error::from_reason(format!(
                            "Batch op {} setEventListener missing/invalid hasHandler at index 3",
                            i
                        ))
                    })?;
                BatchOp::SetEventListener {
                    id: batch_id(arr, 1, i)?,
                    event_type: batch_str(arr, 2, i)?,
                    has_handler,
                }
            }
            "setRoot" => BatchOp::SetRoot {
                id: batch_id(arr, 1, i)?,
            },
            "setCustomProp" => {
                let value_json = batch_str(arr, 3, i)?;
                let value: serde_json::Value =
                    serde_json::from_str(&value_json).map_err(|e| {
                        Error::from_reason(format!(
                            "Batch op {} setCustomProp parse error: {}",
                            i, e
                        ))
                    })?;
                BatchOp::SetCustomProp {
                    id: batch_id(arr, 1, i)?,
                    key: batch_str(arr, 2, i)?,
                    value,
                }
            }
            _ => {
                return Err(Error::from_reason(format!(
                    "Batch op {} unknown operation: {:?}",
                    i, op_name
                )));
            }
        };
        parsed.push(batch_op);
    }

    Ok(parsed)
}

/// Apply a batch of mutation tuples to a RetainedTree.
/// Shared between GpuixRenderer::apply_batch and TestGpuixRenderer::apply_batch.
/// Returns accumulated destroyed IDs (as f64) from all destroyElement ops.
///
/// ATOMIC: all ops are parsed and validated first. If any op is malformed,
/// the tree is left unchanged and an error is returned. This prevents
/// partial application that could desync JS and Rust state.
///
/// Batch format: JSON array of tuples [opcode, ...args].
/// See GpuixRenderer::apply_batch for opcode documentation.
pub(crate) fn apply_batch_to_tree(
    tree: &mut RetainedTree,
    ops: &[serde_json::Value],
) -> Result<Vec<f64>> {
    // Phase 1: parse and validate all ops (no mutation).
    let parsed = parse_batch_ops(ops)?;

    // Phase 2: apply all validated ops to the tree.
    let mut destroyed_ids: Vec<f64> = Vec::new();
    for batch_op in parsed {
        match batch_op {
            BatchOp::CreateElement { id, element_type } => {
                tree.create_element(id, element_type);
            }
            BatchOp::DestroyElement { id } => {
                let destroyed = tree.destroy_element(id);
                destroyed_ids.extend(destroyed.iter().map(|&id| id as f64));
            }
            BatchOp::AppendChild { parent_id, child_id } => {
                tree.append_child(parent_id, child_id);
            }
            BatchOp::RemoveChild { parent_id, child_id } => {
                tree.remove_child(parent_id, child_id);
            }
            BatchOp::InsertBefore { parent_id, child_id, before_id } => {
                tree.insert_before(parent_id, child_id, before_id);
            }
            BatchOp::SetStyle { id, style } => {
                tree.set_style(id, style);
            }
            BatchOp::SetText { id, content } => {
                tree.set_text(id, content);
            }
            BatchOp::SetEventListener { id, event_type, has_handler } => {
                tree.set_event_listener(id, event_type, has_handler);
            }
            BatchOp::SetRoot { id } => {
                tree.root_id = Some(id);
            }
            BatchOp::SetCustomProp { id, key, value } => {
                tree.set_custom_prop(id, key, value);
            }
        }
    }

    Ok(destroyed_ids)
}

/// Extract a u64 element ID from a batch tuple at the given index.
fn batch_id(arr: &[serde_json::Value], idx: usize, op_idx: usize) -> Result<u64> {
    let v = arr
        .get(idx)
        .and_then(|v| v.as_f64())
        .ok_or_else(|| Error::from_reason(format!("Batch op {} missing id at index {}", op_idx, idx)))?;
    to_element_id(v)
}

/// Extract a String from a batch tuple at the given index.
fn batch_str(arr: &[serde_json::Value], idx: usize, op_idx: usize) -> Result<String> {
    arr.get(idx)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            Error::from_reason(format!(
                "Batch op {} missing string at index {}",
                op_idx, idx
            ))
        })
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
