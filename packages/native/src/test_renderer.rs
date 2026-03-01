/// TestGpuixRenderer — headless GPUI test renderer exposed to Node.js via napi.
///
/// Uses gpui::TestAppContext (no GPU, no window) to run the **same** GpuixView,
/// build_element(), apply_styles(), and event handlers as the production renderer.
/// Events are collected synchronously into a Vec instead of being queued on the
/// Node.js event loop.
///
/// Key design: GpuixView, build_element(), build_div(), apply_styles(),
/// emit_event_full() are the EXACT SAME code for both production and test.
/// The only difference is the event callback: production wraps ThreadsafeFunction,
/// tests wrap a Vec<EventPayload> collector.
///
/// TestAppContext and VisualTestContext are !Send — stored in thread_local.
/// All napi calls happen on the JS main thread, so this is safe (same pattern
/// as NodePlatform in renderer.rs).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::element_tree::EventPayload;
use crate::renderer::{to_element_id, EventCallback, GpuixView};
use crate::retained_tree::RetainedTree;
use crate::style::StyleDesc;

// ── Thread-local storage for !Send GPUI types ────────────────────────

thread_local! {
    /// Leaked VisualTestContext from TestAppContext::add_window_view().
    /// Stored as raw pointer to work around !Send constraint.
    static TEST_VCX: RefCell<Option<*mut gpui::VisualTestContext>> = const { RefCell::new(None) };

    /// Entity handle for the GpuixView — needed for cx.notify() to trigger re-renders.
    static TEST_VIEW: RefCell<Option<gpui::Entity<GpuixView>>> = const { RefCell::new(None) };

    /// Keep the original TestAppContext alive (its Rc<AppCell> holds the app).
    static TEST_APP_CX: RefCell<Option<gpui::TestAppContext>> = const { RefCell::new(None) };
}

/// Read the VisualTestContext pointer from thread_local.
/// Returns Err if no TestGpuixRenderer has been created on this thread.
fn get_vcx_ptr() -> Result<*mut gpui::VisualTestContext> {
    TEST_VCX
        .with(|v| *v.borrow())
        .ok_or_else(|| Error::from_reason("TestGpuixRenderer not initialized"))
}

/// Clone the Entity<GpuixView> from thread_local.
fn get_view() -> Result<gpui::Entity<GpuixView>> {
    TEST_VIEW.with(|v| {
        let guard = v.borrow();
        let opt: &Option<gpui::Entity<GpuixView>> = &*guard;
        opt.clone()
            .ok_or_else(|| Error::from_reason("TestGpuixRenderer not initialized"))
    })
}

// ── TestGpuixRenderer ────────────────────────────────────────────────

/// Headless GPUI test renderer. Uses the same GpuixView and rendering
/// pipeline as production, but backed by gpui::TestPlatform (no GPU, no window).
///
/// Usage from JS:
///   const r = new TestGpuixRenderer()
///   r.createElement(1, "div")
///   r.setRoot(1)
///   r.commitMutations()
///   r.flush()                  // triggers GpuixView::render()
///   r.simulateClick(50, 50)    // dispatches through GPUI hit testing
///   const events = r.drainEvents()
#[napi]
pub struct TestGpuixRenderer {
    tree: Arc<Mutex<RetainedTree>>,
    events: Arc<Mutex<Vec<EventPayload>>>,
}

#[napi]
impl TestGpuixRenderer {
    #[napi(constructor)]
    pub fn new() -> Result<Self> {
        let tree = Arc::new(Mutex::new(RetainedTree::new()));
        let events: Arc<Mutex<Vec<EventPayload>>> = Arc::new(Mutex::new(Vec::new()));

        // Event callback: push to Vec instead of ThreadsafeFunction.
        let events_clone = events.clone();
        let event_callback: Option<EventCallback> = Some(Arc::new(move |payload: EventPayload| {
            events_clone.lock().unwrap().push(payload);
        }));

        let tree_clone = tree.clone();
        let callback_clone = event_callback.clone();

        // Create headless GPUI app with TestPlatform (no GPU, no window).
        let mut cx = gpui::TestAppContext::single();

        // Open a test window with the same GpuixView used in production.
        // add_window_view returns (Entity<GpuixView>, &'static mut VisualTestContext).
        // The VisualTestContext is leaked (Rc::into_raw) — valid for the test lifetime.
        let (view, vcx) = cx.add_window_view(|_window, _cx| GpuixView {
            tree: tree_clone,
            event_callback: callback_clone,
            window_title: "GPUIX Test".to_string(),
            focus_handles: HashMap::new(),
            _focus_subscriptions: Vec::new(),
        });

        // Store !Send types in thread_local (same pattern as NodePlatform).
        TEST_VCX.with(|v| *v.borrow_mut() = Some(vcx as *mut _));
        TEST_VIEW.with(|v| *v.borrow_mut() = Some(view));
        TEST_APP_CX.with(|v| *v.borrow_mut() = Some(cx));

        Ok(Self { tree, events })
    }

    // ── Mutation API (same interface as GpuixRenderer) ────────────────

    #[napi]
    pub fn create_element(&self, id: f64, element_type: String) -> Result<()> {
        let id = to_element_id(id)?;
        self.tree.lock().unwrap().create_element(id, element_type);
        Ok(())
    }

    /// Destroy an element and all descendants. Returns destroyed IDs
    /// so JS can clean up event handlers.
    #[napi]
    pub fn destroy_element(&self, id: f64) -> Result<Vec<f64>> {
        let id = to_element_id(id)?;
        let destroyed = self.tree.lock().unwrap().destroy_element(id);
        Ok(destroyed.iter().map(|&id| id as f64).collect())
    }

    #[napi]
    pub fn append_child(&self, parent_id: f64, child_id: f64) -> Result<()> {
        let parent_id = to_element_id(parent_id)?;
        let child_id = to_element_id(child_id)?;
        self.tree.lock().unwrap().append_child(parent_id, child_id);
        Ok(())
    }

    #[napi]
    pub fn remove_child(&self, parent_id: f64, child_id: f64) -> Result<()> {
        let parent_id = to_element_id(parent_id)?;
        let child_id = to_element_id(child_id)?;
        self.tree.lock().unwrap().remove_child(parent_id, child_id);
        Ok(())
    }

    #[napi]
    pub fn insert_before(&self, parent_id: f64, child_id: f64, before_id: f64) -> Result<()> {
        let parent_id = to_element_id(parent_id)?;
        let child_id = to_element_id(child_id)?;
        let before_id = to_element_id(before_id)?;
        self.tree
            .lock()
            .unwrap()
            .insert_before(parent_id, child_id, before_id);
        Ok(())
    }

    #[napi]
    pub fn set_style(&self, id: f64, style_json: String) -> Result<()> {
        let id = to_element_id(id)?;
        let style: StyleDesc = serde_json::from_str(&style_json)
            .map_err(|e| Error::from_reason(format!("Failed to parse style: {}", e)))?;
        self.tree.lock().unwrap().set_style(id, style);
        Ok(())
    }

    #[napi]
    pub fn set_text(&self, id: f64, content: String) -> Result<()> {
        let id = to_element_id(id)?;
        self.tree.lock().unwrap().set_text(id, content);
        Ok(())
    }

    #[napi]
    pub fn set_event_listener(
        &self,
        id: f64,
        event_type: String,
        has_handler: bool,
    ) -> Result<()> {
        let id = to_element_id(id)?;
        self.tree
            .lock()
            .unwrap()
            .set_event_listener(id, event_type, has_handler);
        Ok(())
    }

    /// Set the root element (called from appendChildToContainer).
    #[napi]
    pub fn set_root(&self, id: f64) -> Result<()> {
        let id = to_element_id(id)?;
        self.tree.lock().unwrap().root_id = Some(id);
        Ok(())
    }

    /// Signal that a batch of mutations is complete.
    /// In tests, this is a no-op — flush() handles the actual re-render.
    #[napi]
    pub fn commit_mutations(&self) -> Result<()> {
        Ok(())
    }

    // ── Test-specific methods ────────────────────────────────────────

    /// Notify the view entity and run GPUI until parked.
    /// This triggers GpuixView::render() → build_element() → GPUI layout.
    /// Must be called after mutations and before simulating events (GPUI's
    /// hit testing requires elements to be laid out).
    #[napi]
    pub fn flush(&self) -> Result<()> {
        let vcx_ptr = get_vcx_ptr()?;
        let view = get_view()?;

        // SAFETY: vcx_ptr was obtained from VisualTestContext::into_mut() which
        // leaked an Rc. The pointer is valid for the lifetime of the test.
        // All access happens on the JS main thread (thread_local guarantees this).
        let vcx = unsafe { &mut *vcx_ptr };

        // Notify the view entity that it needs to re-render.
        vcx.update(|_window, cx| {
            view.update(cx, |_, cx| {
                cx.notify();
            });
        });

        // Drive GPUI to process the render (layout, hit-test registration).
        vcx.run_until_parked();

        Ok(())
    }

    /// Simulate a click at the given window coordinates.
    /// Dispatches MouseDown + MouseUp through GPUI's input pipeline,
    /// which triggers the same event handlers as production.
    /// IMPORTANT: Call flush() before this — hit testing requires laid-out elements.
    #[napi]
    pub fn simulate_click(&self, x: f64, y: f64) -> Result<()> {
        let vcx_ptr = get_vcx_ptr()?;
        let vcx = unsafe { &mut *vcx_ptr };

        vcx.simulate_click(
            gpui::point(gpui::px(x as f32), gpui::px(y as f32)),
            gpui::Modifiers::default(),
        );

        Ok(())
    }

    /// Simulate key strokes through GPUI's input pipeline.
    /// Format: space-separated keys, e.g. "a", "enter", "cmd-shift-p".
    /// The focused element receives keyDown/keyUp events.
    #[napi]
    pub fn simulate_keystrokes(&self, keystrokes: String) -> Result<()> {
        let vcx_ptr = get_vcx_ptr()?;
        let vcx = unsafe { &mut *vcx_ptr };

        vcx.simulate_keystrokes(&keystrokes);

        Ok(())
    }

    /// Simulate a mouse move to the given coordinates.
    #[napi]
    pub fn simulate_mouse_move(&self, x: f64, y: f64) -> Result<()> {
        let vcx_ptr = get_vcx_ptr()?;
        let vcx = unsafe { &mut *vcx_ptr };

        vcx.simulate_mouse_move(
            gpui::point(gpui::px(x as f32), gpui::px(y as f32)),
            None,
            gpui::Modifiers::default(),
        );

        Ok(())
    }

    /// Focus an element by its numeric ID.
    /// The element must have a FocusHandle (created by sync_focus_handles when
    /// the element has keyDown, keyUp, focus, or blur listeners).
    /// Call flush() before this so the element tree and focus handles exist.
    #[napi]
    pub fn focus_element(&self, id: f64) -> Result<()> {
        let id = to_element_id(id)?;
        let vcx_ptr = get_vcx_ptr()?;
        let view = get_view()?;
        let vcx = unsafe { &mut *vcx_ptr };

        vcx.update(|window, cx| {
            view.update(cx, |view, _cx| {
                if let Some(handle) = view.focus_handles.get(&id) {
                    handle.focus(window, _cx);
                }
            });
        });

        vcx.run_until_parked();
        Ok(())
    }

    /// Simulate a scroll wheel event at the given position.
    /// delta_x and delta_y are in pixels (negative = scroll up/left).
    #[napi]
    pub fn simulate_scroll_wheel(
        &self,
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<()> {
        let vcx_ptr = get_vcx_ptr()?;
        let vcx = unsafe { &mut *vcx_ptr };

        vcx.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(gpui::px(x as f32), gpui::px(y as f32)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(
                gpui::px(delta_x as f32),
                gpui::px(delta_y as f32),
            )),
            modifiers: gpui::Modifiers::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });

        Ok(())
    }

    /// Return and clear all collected events since the last drain.
    /// Events are collected synchronously — no event loop queuing.
    #[napi]
    pub fn drain_events(&self) -> Vec<EventPayload> {
        let mut events = self.events.lock().unwrap();
        events.drain(..).collect()
    }

    // ── Tree inspection ──────────────────────────────────────────────

    /// Get all text content in the tree (depth-first order).
    #[napi]
    pub fn get_all_text(&self) -> Vec<String> {
        let tree = self.tree.lock().unwrap();
        let mut texts = Vec::new();
        if let Some(root_id) = tree.root_id {
            Self::collect_text(root_id, &tree, &mut texts);
        }
        texts
    }

    /// Find element IDs matching the given type (e.g. "div", "text").
    #[napi]
    pub fn find_by_type(&self, element_type: String) -> Vec<f64> {
        let tree = self.tree.lock().unwrap();
        tree.elements
            .values()
            .filter(|e| e.element_type == element_type)
            .map(|e| e.id as f64)
            .collect()
    }

    /// Check if an element has a specific event listener.
    #[napi]
    pub fn has_event_listener(&self, id: f64, event_type: String) -> Result<bool> {
        let id = to_element_id(id)?;
        let tree = self.tree.lock().unwrap();
        Ok(tree
            .elements
            .get(&id)
            .map(|e| e.events.contains(&event_type))
            .unwrap_or(false))
    }

    /// Get the text content of an element.
    #[napi]
    pub fn get_text(&self, id: f64) -> Result<Option<String>> {
        let id = to_element_id(id)?;
        let tree = self.tree.lock().unwrap();
        Ok(tree.elements.get(&id).and_then(|e| e.content.clone()))
    }

    /// Get the full tree as JSON for snapshot testing.
    #[napi]
    pub fn get_tree_json(&self) -> Result<String> {
        let tree = self.tree.lock().unwrap();
        let json = match tree.root_id {
            Some(root_id) => Self::element_to_json(root_id, &tree),
            None => serde_json::Value::Null,
        };
        serde_json::to_string_pretty(&json)
            .map_err(|e| Error::from_reason(format!("JSON serialization failed: {}", e)))
    }

    // ── Private helpers ──────────────────────────────────────────────

    fn collect_text(id: u64, tree: &RetainedTree, texts: &mut Vec<String>) {
        if let Some(element) = tree.elements.get(&id) {
            if let Some(ref content) = element.content {
                texts.push(content.clone());
            }
            for &child_id in &element.children {
                Self::collect_text(child_id, tree, texts);
            }
        }
    }

    fn element_to_json(id: u64, tree: &RetainedTree) -> serde_json::Value {
        let Some(element) = tree.elements.get(&id) else {
            return serde_json::Value::Null;
        };

        let mut obj = serde_json::Map::new();
        obj.insert(
            "type".to_string(),
            serde_json::Value::String(element.element_type.clone()),
        );
        obj.insert("id".to_string(), serde_json::json!(element.id));

        if let Some(ref content) = element.content {
            obj.insert(
                "text".to_string(),
                serde_json::Value::String(content.clone()),
            );
        }

        if let Some(ref style) = element.style {
            if let Ok(style_json) = serde_json::to_value(style) {
                // Only include non-null style fields.
                if let serde_json::Value::Object(ref map) = style_json {
                    let filtered: serde_json::Map<String, serde_json::Value> = map
                        .iter()
                        .filter(|(_, v)| !v.is_null())
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    if !filtered.is_empty() {
                        obj.insert(
                            "style".to_string(),
                            serde_json::Value::Object(filtered),
                        );
                    }
                }
            }
        }

        if !element.events.is_empty() {
            let mut events: Vec<String> = element.events.iter().cloned().collect();
            events.sort();
            obj.insert("events".to_string(), serde_json::json!(events));
        }

        if !element.children.is_empty() {
            let children: Vec<serde_json::Value> = element
                .children
                .iter()
                .map(|&cid| Self::element_to_json(cid, tree))
                .filter(|v| !v.is_null())
                .collect();
            if !children.is_empty() {
                obj.insert("children".to_string(), serde_json::Value::Array(children));
            }
        }

        serde_json::Value::Object(obj)
    }
}
