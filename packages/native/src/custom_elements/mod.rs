/// Custom element trait infrastructure for GPUIX.
///
/// Allows native GPUI components (input, editor, diff) to be used as
/// React custom elements with props and callbacks. The renderer dispatches
/// to trait objects at render time — each custom element lives in its own
/// file with its own dependencies, cleanly separated from the core renderer.
///
/// Architecture:
///   build_element()
///     "div"  → build_div()             (built-in)
///     "text" → build_text()            (built-in)
///     _      → registry.render(ctx)    (trait dispatch)
use std::collections::{HashMap, HashSet};

use crate::renderer::EventCallback;

pub mod input;
pub mod anchored;

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
}

// ── Traits ───────────────────────────────────────────────────────────

/// A custom element that renders native GPUI content.
///
/// Lifecycle:
///   1. Factory creates instance via CustomElementFactory::create()
///   2. React sends props via set_prop() (called each GPUI frame before render)
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

    /// Set a named prop from JS. Values are JSON-encoded.
    /// Called when React updates props on this element.
    fn set_prop(&mut self, key: &str, value: serde_json::Value);

    /// Return known prop keys. Missing keys are reset to null/default each frame.
    fn supported_props(&self) -> &[&str];

    /// Read a prop value back to JS. Returns None if prop doesn't exist.
    fn get_prop(&self, key: &str) -> Option<serde_json::Value>;

    /// Return which event types this element can emit.
    fn supported_events(&self) -> &[&str];

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

/// Stores factories (one per type) and live instances (one per element ID).
pub struct CustomElementRegistry {
    factories: HashMap<String, Box<dyn CustomElementFactory>>,
    instances: HashMap<u64, Box<dyn CustomElement>>,
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
        registry.register(Box::new(anchored::AnchoredFactory));
        registry
    }

    pub fn register(&mut self, factory: Box<dyn CustomElementFactory>) {
        self.factories
            .insert(factory.element_type().to_string(), factory);
    }

    /// Get an existing instance or create one via the factory.
    /// Returns None if no factory is registered for this element type.
    pub fn get_or_create(
        &mut self,
        id: u64,
        element_type: &str,
    ) -> Option<&mut Box<dyn CustomElement>> {
        if !self.instances.contains_key(&id) {
            let factory = self.factories.get(element_type)?;
            let instance = factory.create(id);
            self.instances.insert(id, instance);
        }
        self.instances.get_mut(&id)
    }

    /// Called when React destroys an element.
    pub fn destroy(&mut self, id: u64) {
        if let Some(mut el) = self.instances.remove(&id) {
            el.destroy();
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

    /// Check if a type name has a registered factory.
    pub fn is_custom_type(&self, element_type: &str) -> bool {
        self.factories.contains_key(element_type)
    }
}
