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

    pub fn destroy_element(&mut self, id: u64) {
        // Recursively remove children first
        if let Some(element) = self.elements.remove(&id) {
            for child_id in element.children {
                self.destroy_element(child_id);
            }
        }
        if self.root_id == Some(id) {
            self.root_id = None;
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
}
