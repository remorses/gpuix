/// Retained element tree — the Rust-side source of truth for the UI.
///
/// React's reconciler sends mutations (create, append, remove, etc.) via napi.
/// This tree stores those mutations. On each GPUI frame, GpuixView::render()
/// walks this tree to build ephemeral GPUI elements.
///
/// All IDs are u64 — JS generates them with an incrementing counter,
/// passes them as numbers across napi (no string allocation).

use std::collections::{HashMap, HashSet};

use crate::style::StyleDesc;

pub struct RetainedElement {
    pub id: u64,
    pub element_type: String,
    pub style: Option<StyleDesc>,
    pub content: Option<String>,
    pub events: HashSet<String>,
    pub children: Vec<u64>,
    pub parent: Option<u64>,
    /// Props for custom elements (input, editor, diff, etc.).
    /// Keyed by prop name, values are JSON. Ignored for "div" and "text".
    pub custom_props: HashMap<String, serde_json::Value>,
}

impl RetainedElement {
    pub fn new(id: u64, element_type: String) -> Self {
        Self {
            id,
            element_type,
            style: None,
            content: None,
            events: HashSet::new(),
            children: Vec::new(),
            parent: None,
            custom_props: HashMap::new(),
        }
    }
}

pub struct RetainedTree {
    pub elements: HashMap<u64, RetainedElement>,
    /// The root element ID set by appendChildToContainer.
    pub root_id: Option<u64>,
}

impl RetainedTree {
    pub fn new() -> Self {
        Self {
            elements: HashMap::new(),
            root_id: None,
        }
    }

    pub fn create_element(&mut self, id: u64, element_type: String) {
        self.elements.insert(id, RetainedElement::new(id, element_type));
    }

    /// Recursively destroy an element and all its children.
    /// Returns all destroyed IDs so the caller can clean up JS-side state.
    pub fn destroy_element(&mut self, id: u64) -> Vec<u64> {
        let mut destroyed = Vec::new();
        self.destroy_element_recursive(id, &mut destroyed);
        if self.root_id == Some(id) {
            self.root_id = None;
        }
        destroyed
    }

    fn destroy_element_recursive(&mut self, id: u64, destroyed: &mut Vec<u64>) {
        if let Some(element) = self.elements.remove(&id) {
            destroyed.push(id);
            for child_id in element.children {
                self.destroy_element_recursive(child_id, destroyed);
            }
        }
    }

    pub fn append_child(&mut self, parent_id: u64, child_id: u64) {
        // Remove from old parent if any
        if let Some(old_parent_id) = self.elements.get(&child_id).and_then(|e| e.parent) {
            if let Some(old_parent) = self.elements.get_mut(&old_parent_id) {
                old_parent.children.retain(|c| *c != child_id);
            }
        }
        // Set new parent
        if let Some(child) = self.elements.get_mut(&child_id) {
            child.parent = Some(parent_id);
        }
        // Add to new parent's children
        if let Some(parent) = self.elements.get_mut(&parent_id) {
            parent.children.push(child_id);
        }
    }

    pub fn remove_child(&mut self, parent_id: u64, child_id: u64) {
        if let Some(parent) = self.elements.get_mut(&parent_id) {
            parent.children.retain(|c| *c != child_id);
        }
        if let Some(child) = self.elements.get_mut(&child_id) {
            child.parent = None;
        }
    }

    pub fn insert_before(&mut self, parent_id: u64, child_id: u64, before_id: u64) {
        // Remove from old parent if any
        if let Some(old_parent_id) = self.elements.get(&child_id).and_then(|e| e.parent) {
            if let Some(old_parent) = self.elements.get_mut(&old_parent_id) {
                old_parent.children.retain(|c| *c != child_id);
            }
        }
        // Set new parent
        if let Some(child) = self.elements.get_mut(&child_id) {
            child.parent = Some(parent_id);
        }
        // Insert before the target
        if let Some(parent) = self.elements.get_mut(&parent_id) {
            let pos = parent.children.iter().position(|c| *c == before_id).unwrap_or(parent.children.len());
            parent.children.insert(pos, child_id);
        }
    }

    pub fn set_style(&mut self, id: u64, style: StyleDesc) {
        if let Some(element) = self.elements.get_mut(&id) {
            // StyleDesc with all None fields = no style (cleared)
            element.style = Some(style);
        }
    }

    pub fn set_text(&mut self, id: u64, content: String) {
        if let Some(element) = self.elements.get_mut(&id) {
            element.content = Some(content);
        }
    }

    pub fn set_event_listener(&mut self, id: u64, event_type: String, has_handler: bool) {
        if let Some(element) = self.elements.get_mut(&id) {
            if has_handler {
                element.events.insert(event_type);
            } else {
                element.events.remove(&event_type);
            }
        }
    }

    /// Set a custom prop on an element (for non-div/text elements).
    pub fn set_custom_prop(&mut self, id: u64, key: String, value: serde_json::Value) {
        if let Some(element) = self.elements.get_mut(&id) {
            if value.is_null() {
                element.custom_props.remove(&key);
            } else {
                element.custom_props.insert(key, value);
            }
        }
    }

    /// Read a custom prop value from an element.
    pub fn get_custom_prop(&self, id: u64, key: &str) -> Option<&serde_json::Value> {
        self.elements.get(&id)?.custom_props.get(key)
    }
}
