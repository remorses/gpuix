/// Custom element trait infrastructure for GPUIX.
///
/// Allows native GPUI components (input, editor, diff) to be used as
/// React custom elements with props and callbacks. The renderer dispatches
/// to trait objects at render time — each custom element lives in its own
/// file with its own dependencies, cleanly separated from the core renderer.
///
/// Architecture:
///   build_element()
///     "div" | "text" → build_host_container()  (built-in)
///     _              → registry.render(ctx)    (trait dispatch)
use std::collections::{HashMap, HashSet};

use crate::renderer::EventCallback;

pub mod anchored;
pub mod code;
pub mod diff;
pub mod img;
pub mod input;
pub mod markdown;

// ── Render context ───────────────────────────────────────────────────

/// Context passed to CustomElement::render() with everything needed
/// to build GPUI elements with events and focus.
pub struct CustomRenderContext<'a> {
    /// Numeric element ID (matches React's instance ID).
    pub id: u64,
    /// Event types registered by React (e.g. "keyDown", "click").
    pub events: &'a HashSet<String>,
    /// Callback for emitting events back to JS.
    pub event_callback: &'a Option<EventCallback>,
    /// Pre-created FocusHandle for this element (if it has keyboard/focus listeners).
    pub focus_handle: Option<&'a gpui::FocusHandle>,
    /// Style object from the retained element for layout and appearance.
    pub style: Option<&'a crate::style::StyleDesc>,
    /// Built child elements from the retained tree for this custom node.
    pub children: Vec<gpui::AnyElement>,
    /// Live text selection. Elements that paint text MUST route it through
    /// `crate::text::selectable_text` with this handle, otherwise their glyphs
    /// are invisible to a drag that starts outside them.
    pub selection: crate::text::SharedSelection,
    /// False when an ancestor set `userSelect: "none"`.
    pub selectable: bool,
    /// Inherited selection wash colour.
    pub selection_wash: gpui::Hsla,
    /// Everything this element inherits, so `var()` and `currentColor` in its
    /// style resolve the same way they do on a plain div.
    pub cascade: crate::inheritance::Inherited,
    /// `highlight` declared by the nearest ancestor, unresolved.
    ///
    /// A native element generates its text during `render()`, so the retained
    /// tree never sees it and the build-time resolver cannot produce ranges for
    /// it. `ctx.text` matches the exact string it is about to paint instead,
    /// which makes drift between the search pass and the paint pass impossible.
    pub highlight_set: Option<std::sync::Arc<crate::text::HighlightContext>>,
}

impl CustomRenderContext<'_> {
    /// Apply this element's `style` prop with its `hover` and `active`
    /// refinements. gpui keeps that state behind the element id, so the caller
    /// must have set `.id(..)`.
    pub fn styled_interactive<E>(&self, el: E) -> E
    where
        E: gpui::Styled + gpui::StatefulInteractiveElement,
    {
        let Some(style) = self.style else {
            return el;
        };
        crate::renderer::apply_interactive_styles(el, style, &self.cascade.scope())
    }

    /// Build a selectable text run for this element. `sub` distinguishes
    /// multiple runs painted by the same element, such as code-block lines, and
    /// must be stable across frames or the selection flickers.
    pub fn text(
        &self,
        sub: usize,
        text: impl Into<gpui::SharedString>,
        runs: Option<Vec<gpui::TextRun>>,
    ) -> gpui::AnyElement {
        let text = text.into();
        crate::text::selectable_text(crate::text::SelectableText {
            selectable: self.selectable,
            highlight: self
                .highlight_set
                .clone()
                .map(crate::text::HighlightSource::Native),
            ..crate::text::SelectableText::new(
                self.id,
                sub,
                text,
                runs,
                self.selection.clone(),
                self.selection_wash,
            )
        })
    }

    /// Chrome text: line numbers, language tags, file headers. Painted and
    /// logged for tests, but never part of a selection, so copying a code block
    /// yields code and not a column of line numbers.
    pub fn chrome_text(
        &self,
        text: impl Into<gpui::SharedString>,
        runs: Option<Vec<gpui::TextRun>>,
    ) -> gpui::AnyElement {
        crate::text::chrome_text(text.into(), runs)
    }
}

// ── Shared surface plumbing ──────────────────────────────────────────

/// Prepare the stateful gpui root of a custom element.
///
/// Applies the declared styles including `hover` and `active`, records the last
/// painted box so `getElementBounds` and automation locators can find the
/// element, and installs the mouse handlers the adapter listed in
/// `supported_events`. Call it before adding children.
///
/// The caller must have given `el` a host-derived id already: gpui keys both the
/// pseudo-style state and the accessibility node off that id.
pub(crate) fn custom_surface(
    mut el: gpui::Stateful<gpui::Div>,
    ctx: &CustomRenderContext,
) -> gpui::Stateful<gpui::Div> {
    use gpui::prelude::*;

    el = ctx.styled_interactive(el);
    // `bounds_tracker` is `absolute().size_full()`, so it needs a positioned
    // parent to measure.
    if ctx
        .style
        .and_then(|style| style.position.as_deref())
        .is_none()
    {
        el = el.relative();
    }
    el = el.child(crate::automation::bounds_tracker(ctx.id, None, None));
    wire_standard_events(el, ctx)
}

/// Attach the mouse events a custom element declares in `supported_events`.
///
/// Declaring an event and never installing a handler is worse than not
/// supporting it: the prop type-checks, the listener is registered on the JS
/// side, and nothing ever fires.
/// Generic over the element, not just `Stateful<Div>`, because `<img>` and
/// `<svg>` are gpui leaves and cannot hold a child. They declare the same props
/// as everything else, so they have to wire the same events.
pub(crate) fn wire_standard_events<E: gpui::StatefulInteractiveElement>(
    mut el: E,
    ctx: &CustomRenderContext,
) -> E {
    let id = ctx.id;
    for event in ctx.events {
        let callback = ctx.event_callback.clone();
        match event.as_str() {
            "click" => {
                // Match retained hosts: GPUI's semantic click is unreliable under
                // embedded AppKit pumping, so primary mouse-up is the click boundary.
                el = el.on_mouse_up(gpui::MouseButton::Left, move |mouse_event, _window, _cx| {
                    crate::renderer::emit_event_full(&callback, id, "click", |p| {
                        let (x, y) = crate::renderer::point_to_xy(mouse_event.position);
                        p.x = Some(x);
                        p.y = Some(y);
                        p.button = Some(0);
                        p.click_count = Some(mouse_event.click_count as u32);
                        p.modifiers = Some(mouse_event.modifiers.into());
                        p.is_right_click = Some(false);
                    });
                });
            }
            "mouseEnter" | "mouseLeave" => {
                // gpui reports both edges through one listener, so wire it once.
                if event == "mouseEnter" || !ctx.events.contains("mouseEnter") {
                    let enter = ctx.events.contains("mouseEnter");
                    let leave = ctx.events.contains("mouseLeave");
                    let callback = ctx.event_callback.clone();
                    el = el.on_hover(move |&hovered, _window, _cx| {
                        let kind = if hovered { "mouseEnter" } else { "mouseLeave" };
                        if (hovered && enter) || (!hovered && leave) {
                            crate::renderer::emit_event_full(&callback, id, kind, |p| {
                                p.hovered = Some(hovered);
                            });
                        }
                    });
                }
            }
            _ => {}
        }
    }
    el
}

// ── Traits ───────────────────────────────────────────────────────────

/// A custom element that renders native GPUI content.
///
/// Lifecycle:
///   1. Factory creates instance via CustomElementFactory::create()
///   2. Registry synchronizes changed props and declared event capabilities
///   3. Each GPUI frame calls render() → returns AnyElement
///   4. React unmounts → destroy() for cleanup
pub trait CustomElement: 'static {
    /// Build GPUI elements for this frame.
    /// Called on every GPUI render cycle (immediate mode).
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement;

    /// Apply a changed prop from the retained tree. Removed props arrive as null.
    fn set_prop(&mut self, key: &str, value: serde_json::Value);

    /// Immutable prop capability declaration for this adapter.
    fn supported_props(&self) -> &'static [&'static str];

    /// Immutable event capability declaration for this adapter.
    fn supported_events(&self) -> &'static [&'static str];

    /// Clean up resources (GPUI entities, subscriptions, etc.)
    fn destroy(&mut self);
}

/// Factory for creating CustomElement instances.
/// One factory per element type, registered at startup.
pub trait CustomElementFactory: 'static {
    /// The element type name that React uses (e.g. "input", "editor", "diff").
    fn element_type(&self) -> &str;

    /// Create a new element instance.
    fn create(&self, id: u64) -> Box<dyn CustomElement>;
}

// ── Registry ─────────────────────────────────────────────────────────

/// Stores one custom adapter together with the state already synchronized into it.
struct CustomElementEntry {
    element_type: String,
    element: Box<dyn CustomElement>,
    applied_props: HashMap<String, serde_json::Value>,
}

impl CustomElementEntry {
    fn sync(&mut self, props: &HashMap<String, serde_json::Value>) {
        let supported_props = self.element.supported_props();
        for &key in supported_props {
            let value = props.get(key).cloned().unwrap_or(serde_json::Value::Null);
            if self.applied_props.get(key) != Some(&value) {
                self.element.set_prop(key, value.clone());
                self.applied_props.insert(key.to_string(), value);
            }
        }

        for (key, value) in props {
            if supported_props.contains(&key.as_str()) || self.applied_props.get(key) == Some(value)
            {
                continue;
            }
            self.element.set_prop(key, value.clone());
            self.applied_props.insert(key.clone(), value.clone());
        }

        let removed_unknown: Vec<String> = self
            .applied_props
            .keys()
            .filter(|key| !supported_props.contains(&key.as_str()) && !props.contains_key(*key))
            .cloned()
            .collect();
        for key in removed_unknown {
            self.element.set_prop(&key, serde_json::Value::Null);
            self.applied_props.remove(&key);
        }
    }
}

/// Stores factories (one per type) and live adapters (one per element ID).
pub struct CustomElementRegistry {
    factories: HashMap<String, Box<dyn CustomElementFactory>>,
    instances: HashMap<u64, CustomElementEntry>,
}

impl CustomElementRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
            instances: HashMap::new(),
        }
    }

    /// Create a registry pre-loaded with all built-in custom elements.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(input::InputFactory));
        registry.register(Box::new(input::TextareaFactory));
        registry.register(Box::new(anchored::AnchoredFactory));
        registry.register(Box::new(img::ImgFactory));
        registry.register(Box::new(img::SvgFactory));
        registry.register(Box::new(code::CodeFactory));
        registry.register(Box::new(diff::DiffFactory));
        registry.register(Box::new(markdown::MarkdownFactory));
        registry
    }

    pub fn register(&mut self, factory: Box<dyn CustomElementFactory>) {
        self.factories
            .insert(factory.element_type().to_string(), factory);
    }

    /// Get an existing adapter or create one via the registered factory.
    /// Reusing an ID for another type destroys the old adapter first.
    fn get_or_create(&mut self, id: u64, element_type: &str) -> Option<&mut CustomElementEntry> {
        if self
            .instances
            .get(&id)
            .is_some_and(|entry| entry.element_type != element_type)
        {
            self.destroy(id);
        }

        match self.instances.entry(id) {
            std::collections::hash_map::Entry::Occupied(entry) => Some(entry.into_mut()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let factory = self.factories.get(element_type)?;
                Some(entry.insert(CustomElementEntry {
                    element_type: element_type.to_string(),
                    element: factory.create(id),
                    applied_props: HashMap::new(),
                }))
            }
        }
    }

    /// Synchronize one retained frame into an adapter and render it.
    pub fn render(
        &mut self,
        element_type: &str,
        props: &HashMap<String, serde_json::Value>,
        ctx: CustomRenderContext,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::IntoElement;

        let Some(entry) = self.get_or_create(ctx.id, element_type) else {
            log::warn!("Unknown element type: {element_type}");
            return gpui::Empty.into_any_element();
        };

        entry.sync(props);
        let supported = entry.element.supported_events();
        let filtered: HashSet<String> = ctx
            .events
            .iter()
            .filter(|event| supported.contains(&event.as_str()))
            .cloned()
            .collect();
        let ctx = CustomRenderContext {
            events: &filtered,
            ..ctx
        };
        entry.element.render(ctx, window, cx)
    }

    /// Called when React destroys an element.
    pub fn destroy(&mut self, id: u64) {
        if let Some(mut entry) = self.instances.remove(&id) {
            entry.element.destroy();
        }
    }

    /// Remove and destroy instances whose IDs no longer exist in the tree.
    pub fn prune_missing<F>(&mut self, mut is_live: F)
    where
        F: FnMut(u64) -> bool,
    {
        let stale_ids: Vec<u64> = self
            .instances
            .keys()
            .copied()
            .filter(|id| !is_live(*id))
            .collect();

        for id in stale_ids {
            self.destroy(id);
        }
    }

    /// Destroy every live instance. Only for app teardown.
    ///
    /// An instance can hold a `gpui::Entity` handle: `<input>` keeps an
    /// `Entity<TextEditorState>`. gpui's leak detector panics if any handle is
    /// still alive when the `App` drops, so the registry has to be emptied
    /// while the `App` is still there.
    pub fn destroy_all(&mut self) {
        let ids: Vec<u64> = self.instances.keys().copied().collect();
        for id in ids {
            self.destroy(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::*;

    struct RecordingElement {
        updates: Rc<RefCell<Vec<(String, serde_json::Value)>>>,
        destroyed: Rc<Cell<usize>>,
    }

    impl CustomElement for RecordingElement {
        fn render(
            &mut self,
            _ctx: CustomRenderContext,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<crate::renderer::GpuixView>,
        ) -> gpui::AnyElement {
            unreachable!("prop synchronization does not render")
        }

        fn set_prop(&mut self, key: &str, value: serde_json::Value) {
            self.updates.borrow_mut().push((key.to_string(), value));
        }

        fn supported_props(&self) -> &'static [&'static str] {
            &["source"]
        }

        fn supported_events(&self) -> &'static [&'static str] {
            &["click"]
        }

        fn destroy(&mut self) {
            self.destroyed.set(self.destroyed.get() + 1);
        }
    }

    struct RecordingFactory {
        element_type: &'static str,
        updates: Rc<RefCell<Vec<(String, serde_json::Value)>>>,
        destroyed: Rc<Cell<usize>>,
    }

    impl CustomElementFactory for RecordingFactory {
        fn element_type(&self) -> &str {
            self.element_type
        }

        fn create(&self, _id: u64) -> Box<dyn CustomElement> {
            Box::new(RecordingElement {
                updates: self.updates.clone(),
                destroyed: self.destroyed.clone(),
            })
        }
    }

    #[test]
    fn sync_applies_only_changes_and_resets_removed_unknown_props() {
        let updates = Rc::new(RefCell::new(Vec::new()));
        let destroyed = Rc::new(Cell::new(0));
        let mut entry = CustomElementEntry {
            element_type: "recording".to_string(),
            element: Box::new(RecordingElement {
                updates: updates.clone(),
                destroyed,
            }),
            applied_props: HashMap::new(),
        };
        let props = HashMap::from([
            ("source".to_string(), serde_json::json!("first")),
            ("future".to_string(), serde_json::json!(true)),
        ]);
        entry.sync(&props);
        assert_eq!(
            updates.borrow().as_slice(),
            [
                ("source".to_string(), serde_json::json!("first")),
                ("future".to_string(), serde_json::json!(true)),
            ]
        );

        entry.sync(&props);
        assert_eq!(updates.borrow().len(), 2);

        entry.sync(&HashMap::new());
        assert_eq!(
            updates.borrow().as_slice(),
            [
                ("source".to_string(), serde_json::json!("first")),
                ("future".to_string(), serde_json::json!(true)),
                ("source".to_string(), serde_json::Value::Null),
                ("future".to_string(), serde_json::Value::Null),
            ]
        );
    }

    #[test]
    fn reusing_an_id_for_another_type_destroys_the_previous_adapter() {
        let updates = Rc::new(RefCell::new(Vec::new()));
        let destroyed = Rc::new(Cell::new(0));
        let mut registry = CustomElementRegistry::new();
        for element_type in ["first", "second"] {
            registry.register(Box::new(RecordingFactory {
                element_type,
                updates: updates.clone(),
                destroyed: destroyed.clone(),
            }));
        }

        assert!(registry.get_or_create(42, "first").is_some());
        assert!(registry.get_or_create(42, "second").is_some());
        assert_eq!(destroyed.get(), 1);
    }
}
