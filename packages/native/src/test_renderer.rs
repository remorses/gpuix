/// TestGpuixRenderer — GPU-backed GPUI test renderer exposed to Node.js via napi.
///
/// Uses gpui::VisualTestAppContext (real Metal rendering on macOS) with
/// TestDispatcher for deterministic scheduling. Runs the SAME GpuixView,
/// build_element(), apply_styles(), and event handlers as production.
///
/// Windows are positioned offscreen at (-10000, -10000) — invisible but
/// fully rendered by Metal. This enables capture_screenshot() for visual
/// test validation.
///
/// VisualTestAppContext is !Send — stored in thread_local.
/// All napi calls happen on the JS main thread (same safety pattern as
/// NodePlatform in renderer.rs).
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use napi::bindgen_prelude::*;
use napi_derive::napi;

use gpui::AppContext as _;

use crate::custom_elements::CustomElementRegistry;
use crate::element_tree::EventPayload;
use crate::renderer::{to_element_id, EventCallback, GpuixView};
use crate::retained_tree::RetainedTree;
use crate::style::StyleDesc;

// ── Thread-local storage for !Send GPUI types ────────────────────────

/// Bundles VisualTestAppContext + window handle + view entity.
/// Stored in thread_local because VisualTestAppContext is !Send (Rc<AppCell>).
struct VisualTestState {
    cx: gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    view: gpui::Entity<GpuixView>,
}

thread_local! {
    static TEST_STATE: RefCell<Option<VisualTestState>> = const { RefCell::new(None) };
}

/// Access VisualTestAppContext + window + view mutably within thread_local.
/// The closure receives (&mut cx, window_handle, &view_entity).
/// Returns Err if no TestGpuixRenderer has been created on this thread.
fn with_test_state<R>(
    f: impl FnOnce(
        &mut gpui::VisualTestAppContext,
        gpui::AnyWindowHandle,
        &gpui::Entity<GpuixView>,
    ) -> Result<R>,
) -> Result<R> {
    TEST_STATE.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let state = borrow
            .as_mut()
            .ok_or_else(|| Error::from_reason("TestGpuixRenderer not initialized"))?;
        f(&mut state.cx, state.window, &state.view)
    })
}

/// Convert JS button number (0=left, 1=middle, 2=right) to GPUI MouseButton.
fn u32_to_mouse_button(button: u32) -> gpui::MouseButton {
    match button {
        1 => gpui::MouseButton::Middle,
        2 => gpui::MouseButton::Right,
        _ => gpui::MouseButton::Left,
    }
}

// ── TestGpuixRenderer ────────────────────────────────────────────────

/// GPU-backed GPUI test renderer. Uses VisualTestAppContext (real Metal
/// rendering on macOS) with TestDispatcher for deterministic scheduling.
/// Same GpuixView and rendering pipeline as production.
///
/// Usage from JS:
///   const r = new TestGpuixRenderer()
///   r.createElement(1, "div")
///   r.setRoot(1)
///   r.commitMutations()
///   r.flush()                  // triggers GpuixView::render() via Metal
///   r.simulateClick(50, 50)    // dispatches through GPUI hit testing
///   const events = r.drainEvents()
///   r.captureScreenshot("/tmp/test.png")  // saves rendered UI as PNG
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

        // Create VisualTestAppContext with real macOS Metal rendering +
        // TestDispatcher for deterministic scheduling.
        let mac_platform = gpui_macos::MacPlatform::new(false);
        let mut cx = gpui::VisualTestAppContext::new(Rc::new(mac_platform));

        // Open an offscreen window at (-10000, -10000) — invisible but fully
        // rendered by Metal. Uses the same GpuixView as production.
        let window_handle = cx
            .open_offscreen_window_default(|_window, app| {
                app.new(|_cx| GpuixView {
                    tree: tree_clone,
                    event_callback: callback_clone,
                    window_title: "GPUIX Test".to_string(),
                    focus_handles: HashMap::new(),
                    _focus_subscriptions: Vec::new(),
                    custom_registry: CustomElementRegistry::with_defaults(),
                })
            })
            .map_err(|e| Error::from_reason(format!("Failed to open test window: {}", e)))?;

        // Get the root entity (Entity<GpuixView>) from the window.
        let view = window_handle
            .entity(&cx)
            .map_err(|e| Error::from_reason(format!("Failed to get root view: {}", e)))?;

        // Convert typed WindowHandle<GpuixView> to AnyWindowHandle for simulation methods.
        let window: gpui::AnyWindowHandle = window_handle.into();

        // Store !Send types in thread_local (same pattern as NodePlatform).
        TEST_STATE.with(|cell| {
            *cell.borrow_mut() = Some(VisualTestState { cx, window, view });
        });

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
    pub fn set_event_listener(&self, id: f64, event_type: String, has_handler: bool) -> Result<()> {
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

    /// Set a custom prop on an element (for non-div/text elements like input, editor, diff).
    #[napi]
    pub fn set_custom_prop(&self, id: f64, key: String, value_json: String) -> Result<()> {
        let id = to_element_id(id)?;
        let value: serde_json::Value = serde_json::from_str(&value_json)
            .map_err(|e| Error::from_reason(format!("Failed to parse custom prop value: {}", e)))?;
        self.tree.lock().unwrap().set_custom_prop(id, key, value);
        Ok(())
    }

    /// Get a custom prop value from an element.
    #[napi]
    pub fn get_custom_prop(&self, id: f64, key: String) -> Result<Option<String>> {
        let id = to_element_id(id)?;
        let tree = self.tree.lock().unwrap();
        Ok(tree
            .get_custom_prop(id, &key)
            .map(|v| serde_json::to_string(v).unwrap_or_default()))
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
        with_test_state(|cx, window, view| {
            let view = view.clone();
            cx.update_window(window, |_, _window, app| {
                view.update(app, |_, cx| {
                    cx.notify();
                });
            })
            .map_err(|e| Error::from_reason(e.to_string()))?;

            cx.run_until_parked();
            Ok(())
        })
    }

    /// Simulate a click at the given window coordinates.
    /// Dispatches MouseDown + MouseUp through GPUI's input pipeline,
    /// which triggers the same event handlers as production.
    /// IMPORTANT: Call flush() before this — hit testing requires laid-out elements.
    #[napi]
    pub fn simulate_click(&self, x: f64, y: f64) -> Result<()> {
        with_test_state(|cx, window, _view| {
            cx.simulate_click(
                window,
                gpui::point(gpui::px(x as f32), gpui::px(y as f32)),
                gpui::Modifiers::default(),
            );
            Ok(())
        })
    }

    /// Simulate key strokes through GPUI's input pipeline.
    /// Format: space-separated keys, e.g. "a", "enter", "cmd-shift-p".
    /// The focused element receives keyDown/keyUp events.
    #[napi]
    pub fn simulate_keystrokes(&self, keystrokes: String) -> Result<()> {
        with_test_state(|cx, window, _view| {
            cx.simulate_keystrokes(window, &keystrokes);
            Ok(())
        })
    }

    /// Simulate a single key down event through GPUI's input pipeline.
    /// Format: modifier-key string, e.g. "a", "enter", "cmd-s".
    /// Unlike simulate_keystrokes, this dispatches ONLY a KeyDownEvent —
    /// no automatic KeyUpEvent follows. Use with simulate_key_up for
    /// fine-grained key event testing.
    #[napi]
    pub fn simulate_key_down(&self, keystroke: String, is_held: Option<bool>) -> Result<()> {
        with_test_state(|cx, window, _view| {
            let parsed = gpui::Keystroke::parse(&keystroke).map_err(|e| {
                Error::from_reason(format!("Invalid keystroke '{}': {}", keystroke, e))
            })?;

            cx.simulate_event(
                window,
                gpui::KeyDownEvent {
                    keystroke: parsed,
                    is_held: is_held.unwrap_or(false),
                    prefer_character_input: false,
                },
            );

            Ok(())
        })
    }

    /// Simulate a single key up event through GPUI's input pipeline.
    /// Format: modifier-key string, e.g. "a", "enter", "cmd-s".
    /// Pairs with simulate_key_down for fine-grained key event testing.
    #[napi]
    pub fn simulate_key_up(&self, keystroke: String) -> Result<()> {
        with_test_state(|cx, window, _view| {
            let parsed = gpui::Keystroke::parse(&keystroke).map_err(|e| {
                Error::from_reason(format!("Invalid keystroke '{}': {}", keystroke, e))
            })?;

            cx.simulate_event(window, gpui::KeyUpEvent { keystroke: parsed });

            Ok(())
        })
    }

    /// Simulate a mouse move to the given coordinates.
    /// pressed_button: optional mouse button held during move (0=left, 1=middle, 2=right).
    /// Used to simulate drag events.
    #[napi]
    pub fn simulate_mouse_move(&self, x: f64, y: f64, pressed_button: Option<u32>) -> Result<()> {
        with_test_state(|cx, window, _view| {
            let button: Option<gpui::MouseButton> = pressed_button.map(u32_to_mouse_button);

            cx.simulate_mouse_move(
                window,
                gpui::point(gpui::px(x as f32), gpui::px(y as f32)),
                button,
                gpui::Modifiers::default(),
            );

            Ok(())
        })
    }

    /// Focus an element by its numeric ID.
    /// The element must have a FocusHandle (created by sync_focus_handles when
    /// the element has keyDown, keyUp, focus, or blur listeners).
    /// Call flush() before this so the element tree and focus handles exist.
    #[napi]
    pub fn focus_element(&self, id: f64) -> Result<()> {
        let id = to_element_id(id)?;

        with_test_state(|cx, window, view| {
            let view = view.clone();

            cx.update_window(window, |_, window, app| {
                view.update(app, |view, cx| {
                    if let Some(handle) = view.focus_handles.get(&id) {
                        handle.focus(window, cx);
                    }
                });
            })
            .map_err(|e| Error::from_reason(e.to_string()))?;

            cx.run_until_parked();
            Ok(())
        })
    }

    /// Simulate a mouse down event at the given window coordinates.
    /// Button: 0=left, 1=middle, 2=right. Defaults to left (0).
    #[napi]
    pub fn simulate_mouse_down(&self, x: f64, y: f64, button: Option<u32>) -> Result<()> {
        with_test_state(|cx, window, _view| {
            cx.simulate_mouse_down(
                window,
                gpui::point(gpui::px(x as f32), gpui::px(y as f32)),
                u32_to_mouse_button(button.unwrap_or(0)),
                gpui::Modifiers::default(),
            );
            Ok(())
        })
    }

    /// Simulate a mouse up event at the given window coordinates.
    /// Button: 0=left, 1=middle, 2=right. Defaults to left (0).
    #[napi]
    pub fn simulate_mouse_up(&self, x: f64, y: f64, button: Option<u32>) -> Result<()> {
        with_test_state(|cx, window, _view| {
            cx.simulate_mouse_up(
                window,
                gpui::point(gpui::px(x as f32), gpui::px(y as f32)),
                u32_to_mouse_button(button.unwrap_or(0)),
                gpui::Modifiers::default(),
            );
            Ok(())
        })
    }

    /// Simulate a scroll wheel event at the given position.
    /// delta_x and delta_y are in pixels (negative = scroll up/left).
    #[napi]
    pub fn simulate_scroll_wheel(&self, x: f64, y: f64, delta_x: f64, delta_y: f64) -> Result<()> {
        with_test_state(|cx, window, _view| {
            cx.simulate_event(
                window,
                gpui::ScrollWheelEvent {
                    position: gpui::point(gpui::px(x as f32), gpui::px(y as f32)),
                    delta: gpui::ScrollDelta::Pixels(gpui::point(
                        gpui::px(delta_x as f32),
                        gpui::px(delta_y as f32),
                    )),
                    modifiers: gpui::Modifiers::default(),
                    touch_phase: gpui::TouchPhase::Moved,
                },
            );
            Ok(())
        })
    }

    /// Capture a screenshot of the current rendered state and save as PNG.
    /// macOS only — requires Metal GPU rendering via VisualTestAppContext.
    #[napi]
    pub fn capture_screenshot(&self, path: String) -> Result<()> {
        with_test_state(|cx, window, view| {
            let view = view.clone();

            // Flush: notify view and run until parked so layout/rendering are current.
            cx.update_window(window, |_, _window, app| {
                view.update(app, |_, cx| {
                    cx.notify();
                });
            })
            .map_err(|e| Error::from_reason(e.to_string()))?;

            // Force a window refresh before capture so render_to_image reads
            // the most recent frame scene.
            cx.update_window(window, |_, window, _app| {
                window.refresh();
            })
            .map_err(|e| Error::from_reason(e.to_string()))?;

            cx.run_until_parked();

            // Capture via GPUI's render_to_image (Metal texture → RgbaImage).
            let image = cx
                .capture_screenshot(window)
                .map_err(|e| Error::from_reason(format!("Screenshot capture failed: {}", e)))?;

            // Save as PNG (format inferred from file extension).
            image
                .save(&path)
                .map_err(|e| Error::from_reason(format!("Failed to save screenshot: {}", e)))?;

            Ok(())
        })
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
                        obj.insert("style".to_string(), serde_json::Value::Object(filtered));
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
