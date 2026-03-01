---
title: Custom Elements via Polymorphic Trait Interface
description: Plan for exposing Zed components (Editor, DiffView) as GPUIX React custom elements using a trait-based plugin architecture.
---

# Custom Elements via Polymorphic Trait Interface

## Goal

Expose complex native components from Zed (Editor, DiffView, etc.) as GPUIX React custom elements — used like built-in elements with props and callbacks:

```tsx
<editor
  value={code}
  language="typescript"
  readOnly={false}
  onChange={(e) => setCode(e.text)}
/>

<diff
  original={oldCode}
  modified={newCode}
  language="typescript"
/>
```

These elements render via GPUI's native GPU pipeline (not div trees), support React props/callbacks across the napi boundary, and are defined in separate Rust files using a polymorphic trait interface.

## Why a Trait Interface

GPUIX currently has two hardcoded element types — `div` and `text` — dispatched in `build_element()` via string matching. Adding complex components like the Zed Editor directly into `renderer.rs` would:

1. Make `renderer.rs` massive (the Editor alone is 29K lines of logic)
2. Couple the core renderer to Zed-specific dependencies
3. Make it hard for contributors to add new element types

Instead, define a **`CustomElement` trait** that any component can implement. The renderer dispatches to trait objects at render time. Each custom element lives in its own file with its own dependencies, cleanly separated from the core renderer.

```
renderer.rs (host)
  build_element()
    "div"  → build_div()             (built-in)
    "text" → build_text()            (built-in)
    _      → custom.render(w, cx)    (trait dispatch)

custom_elements/
  editor.rs    → impl CustomElement for EditorElement
  diff.rs      → impl CustomElement for DiffElement
  input.rs     → impl CustomElement for InputElement
```

## Binary Strategy

**Phase 1 (now):** Everything in a single binary. Custom elements are separate files but compiled into the same `@gpuix/native` crate. This keeps the build simple while we validate the architecture.

**Phase 2 (later):** Split into separate crates and `.node` binaries to reduce default binary size. The trait boundary makes this split mechanical — no architecture changes needed, just move files to a new crate and compile GPUI as a shared `dylib` so both addons share thread-local state.

```
Phase 1: Single binary
  gpuix_native.node
    ├── core renderer (div, text)
    └── custom elements (editor, diff)

Phase 2: Split binaries
  gpuix_native.node        (~5MB, lean)
    └── core renderer (div, text)
  gpuix_editor.node        (~150MB, Zed deps)
    └── custom elements (editor, diff)
  libgpui_shared.dylib     (shared GPUI, single set of thread-locals)
```

## Trait Definitions

### `CustomElement`

Each custom element instance implements this trait. The renderer holds `Box<dyn CustomElement>` per element ID and calls `render()` on each GPUI frame.

```rust
// src/custom_elements/mod.rs

/// A custom element that renders native GPUI content.
///
/// Lifecycle:
///   1. Factory creates instance via CustomElementFactory::create()
///   2. React sends props via set_prop() / set_text()
///   3. Each GPUI frame calls render() → returns AnyElement
///   4. React unmounts → destroy() for cleanup
pub trait CustomElement: 'static {
    /// Build GPUI elements for this frame.
    /// Called on every GPUI render cycle (immediate mode).
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<GpuixView>,
    ) -> gpui::AnyElement;

    /// Set a named prop from JS. Values are JSON-encoded.
    /// Called when React updates props on this element.
    ///
    /// Standard prop names per element type (e.g. "value", "language",
    /// "readOnly") — each CustomElement implementation defines its own.
    fn set_prop(&mut self, key: &str, value: serde_json::Value);

    /// Read a prop value back to JS. Returns None if prop doesn't exist.
    /// Used for imperative reads (e.g. getText() on an editor).
    fn get_prop(&self, key: &str) -> Option<serde_json::Value>;

    /// Return which event types this element can emit.
    /// The host uses this to know which events to wire up.
    /// e.g. ["change", "selectionChange", "save", "focus", "blur"]
    fn supported_events(&self) -> &[&str];

    /// Clean up resources (GPUI entities, subscriptions, etc.)
    fn destroy(&mut self);
}
```

### `CustomElementFactory`

Factories create element instances. One factory per element type, registered at startup.

```rust
/// Factory for creating CustomElement instances.
/// Registered once at init time, called each time React creates
/// an element of this type.
pub trait CustomElementFactory: 'static {
    /// The element type name that React uses.
    /// e.g. "editor", "diff", "input"
    fn element_type(&self) -> &str;

    /// Create a new element instance.
    fn create(
        &self,
        id: u64,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<GpuixView>,
    ) -> Box<dyn CustomElement>;
}
```

### Registration

```rust
/// Registry stored on GpuixView (or a shared struct).
pub struct CustomElementRegistry {
    factories: HashMap<String, Box<dyn CustomElementFactory>>,
    instances: HashMap<u64, Box<dyn CustomElement>>,
}

impl CustomElementRegistry {
    pub fn register(&mut self, factory: Box<dyn CustomElementFactory>) {
        self.factories.insert(factory.element_type().to_string(), factory);
    }

    /// Called by build_element() for unknown element types.
    pub fn get_or_create(
        &mut self,
        id: u64,
        element_type: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<GpuixView>,
    ) -> Option<&mut Box<dyn CustomElement>> {
        if !self.instances.contains_key(&id) {
            let factory = self.factories.get(element_type)?;
            let instance = factory.create(id, window, cx);
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
}
```

## Integration with Existing Renderer

### `build_element()` changes

```rust
// renderer.rs — modified build_element()
pub(crate) fn build_element(
    id: u64,
    tree: &RetainedTree,
    event_callback: &Option<EventCallback>,
    focus_handles: &HashMap<u64, gpui::FocusHandle>,
    custom_registry: &mut CustomElementRegistry,  // new param
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    let Some(element) = tree.elements.get(&id) else {
        return gpui::Empty.into_any_element();
    };

    match element.element_type.as_str() {
        "div"  => build_div(element, tree, event_callback, focus_handles,
                            custom_registry, window, cx),
        "text" => build_text(element),

        // Polymorphic dispatch for all custom elements
        custom_type => {
            if let Some(custom) = custom_registry.get_or_create(
                id, custom_type, window, cx
            ) {
                custom.render(window, cx)
            } else {
                log::warn!("Unknown element type: {}", custom_type);
                gpui::Empty.into_any_element()
            }
        }
    }
}
```

### `GpuixView` changes

```rust
pub(crate) struct GpuixView {
    pub(crate) tree: Arc<Mutex<RetainedTree>>,
    pub(crate) event_callback: Option<EventCallback>,
    pub(crate) window_title: String,
    pub(crate) focus_handles: HashMap<u64, gpui::FocusHandle>,
    pub(crate) _focus_subscriptions: Vec<gpui::Subscription>,
    pub(crate) custom_registry: CustomElementRegistry,  // new field
}
```

### Napi methods for custom props

```rust
// New napi methods on GpuixRenderer

/// Set a custom prop on an element (for non-div/text elements).
/// Key is the prop name, value is JSON-encoded.
#[napi]
pub fn set_custom_prop(&self, id: f64, key: String, value: String) -> Result<()> {
    let id = to_element_id(id)?;
    // Store in RetainedElement or forward directly to CustomElement
    let mut tree = self.tree.lock().unwrap();
    tree.set_custom_prop(id, key, value);
    self.needs_redraw.store(true, Ordering::Relaxed);
    Ok(())
}

/// Get a custom prop value from an element.
#[napi]
pub fn get_custom_prop(&self, id: f64, key: String) -> Result<Option<String>> {
    let id = to_element_id(id)?;
    // Forward to CustomElement instance
    // ...
}
```

### RetainedElement changes

```rust
pub struct RetainedElement {
    pub id: u64,
    pub element_type: String,
    pub style: Option<StyleDesc>,
    pub content: Option<String>,
    pub events: HashSet<String>,
    pub children: Vec<u64>,
    pub parent: Option<u64>,
    pub custom_props: HashMap<String, serde_json::Value>,  // new field
}
```

## React Side (host-config.ts)

```ts
// New prop handling in host-config.ts

// Props that are NOT style or event props get forwarded as custom props.
// This is how React sends "value", "language", "readOnly" etc. to Rust.
const STYLE_PROPS = new Set(["style", "className"])
const EVENT_PROPS = { onClick: "click", onMouseDown: "mouseDown", /* ... */ }

function commitUpdate(instance, oldProps, newProps) {
    // 1. Diff styles (existing)
    // 2. Diff events (existing)
    // 3. Diff custom props (new)
    for (const [key, value] of Object.entries(newProps)) {
        if (STYLE_PROPS.has(key)) continue
        if (key in EVENT_PROPS) continue
        if (key === "children") continue
        if (oldProps[key] !== value) {
            renderer.setCustomProp(instance.id, key, JSON.stringify(value))
        }
    }
}
```

### React component types

```ts
// New intrinsic element types
declare global {
    namespace JSX {
        interface IntrinsicElements {
            div: DivProps
            text: TextProps
            editor: EditorProps
            diff: DiffProps
        }
    }
}

interface EditorProps {
    value?: string
    language?: string
    readOnly?: boolean
    showLineNumbers?: boolean
    showGutter?: boolean
    placeholder?: string
    onChange?: (e: { text: string }) => void
    onSelectionChange?: (e: { ranges: SelectionRange[] }) => void
    onSave?: () => void
    onFocus?: () => void
    onBlur?: () => void
    style?: StyleProps
}

interface DiffProps {
    original?: string
    modified?: string
    language?: string
    readOnly?: boolean
    showWordDiffs?: boolean
    diffMode?: "inline" | "sideBySide"
    style?: StyleProps
}
```

## Custom Element Implementations

### Editor (`custom_elements/editor.rs`)

```rust
use crate::custom_elements::{CustomElement, CustomElementFactory};

pub struct EditorFactory;

impl CustomElementFactory for EditorFactory {
    fn element_type(&self) -> &str { "editor" }

    fn create(
        &self, id: u64,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<GpuixView>,
    ) -> Box<dyn CustomElement> {
        // Create a standalone Editor with project: None
        // (no LSP, no git, no workspace — pure text editing)
        let editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            // or Editor::multi_line / Editor::auto_height based on props
            editor.set_show_gutter(true, cx);
            editor
        });
        Box::new(EditorElement { editor, id })
    }
}

pub struct EditorElement {
    editor: Entity<Editor>,
    id: u64,
}

impl CustomElement for EditorElement {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<GpuixView>,
    ) -> gpui::AnyElement {
        // The Zed Editor's Render impl returns EditorElement
        // which handles all the GPU painting internally.
        self.editor.update(cx, |editor, cx| {
            editor.render(window, cx).into_any_element()
        })
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "value" => { /* editor.set_text(...) */ }
            "readOnly" => { /* editor.set_read_only(...) */ }
            "language" => { /* set language on buffer */ }
            "showLineNumbers" => { /* editor.set_show_line_numbers(...) */ }
            "showGutter" => { /* editor.set_show_gutter(...) */ }
            "placeholder" => { /* editor.set_placeholder_text(...) */ }
            _ => {}
        }
    }

    fn get_prop(&self, key: &str) -> Option<serde_json::Value> {
        match key {
            "value" => { /* editor.text(cx) */ }
            _ => None,
        }
    }

    fn supported_events(&self) -> &[&str] {
        &["change", "selectionChange", "save", "focus", "blur"]
    }

    fn destroy(&mut self) {
        // Entity<Editor> is dropped automatically
    }
}
```

### Diff View (`custom_elements/diff.rs`)

```rust
pub struct DiffFactory;

impl CustomElementFactory for DiffFactory {
    fn element_type(&self) -> &str { "diff" }

    fn create(
        &self, id: u64,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<GpuixView>,
    ) -> Box<dyn CustomElement> {
        // Create an Editor configured in diff mode:
        //   - expand_all_diff_hunks
        //   - read_only
        //   - disable_diagnostics
        //   - disable diff hunk controls
        let editor = cx.new(|cx| {
            let mut editor = Editor::multi_line(window, cx);
            editor.set_read_only(true);
            editor.disable_diagnostics(cx);
            editor.set_expand_all_diff_hunks(cx);
            editor
        });
        Box::new(DiffElement { editor, id })
    }
}

pub struct DiffElement {
    editor: Entity<Editor>,
    id: u64,
}

impl CustomElement for DiffElement {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<GpuixView>,
    ) -> gpui::AnyElement {
        self.editor.update(cx, |editor, cx| {
            editor.render(window, cx).into_any_element()
        })
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "original" => { /* set diff base text on buffer */ }
            "modified" => { /* set buffer text */ }
            "language" => { /* set language */ }
            "showWordDiffs" => { /* toggle word-level diff highlights */ }
            _ => {}
        }
    }

    fn get_prop(&self, _key: &str) -> Option<serde_json::Value> { None }

    fn supported_events(&self) -> &[&str] { &["focus", "blur"] }

    fn destroy(&mut self) {}
}
```

## Event Flow for Custom Elements

Custom elements emit events through the same `EventCallback` mechanism as built-in elements. The element ID links the event to the React component.

```
Custom element detects change (e.g. Editor text edited)
  → EditorElement subscribes to Editor::EditorEvent::Edited
  → Calls emit_event_full(callback, id, "change", |p| { p.text = ... })
  → ThreadsafeFunction queues on Node.js event loop
  → JS event registry finds handler by (id, "change")
  → React handler fires: onChange({ text: "new content" })
  → React re-renders if state changed
```

Custom elements subscribe to GPUI entity events (like `cx.subscribe(&editor, ...)`) during `create()` and forward relevant events through `EventCallback`.

## Initialization & Registration

```rust
// In GpuixRenderer::init() or GpuixView construction:
let mut registry = CustomElementRegistry::new();
registry.register(Box::new(EditorFactory));
registry.register(Box::new(DiffFactory));
// Future: registry.register(Box::new(InputFactory));

// When splitting into separate binaries later,
// plugins call register() after loading:
//   plugin.attach(renderer)  →  renderer.register(EditorFactory)
```

## Zed Dependencies Required

The Editor (even with `project: None`) needs these Zed crates initialized:

| Crate | Why | Init needed |
|-------|-----|-------------|
| `gpui` | Rendering | Already have it |
| `theme` | Colors, syntax | `ThemeSettings::register(cx)` + load a theme |
| `settings` | Editor config | `SettingsStore::init(cx)` |
| `language` | Buffer, tree-sitter | `LanguageRegistry::new()` |
| `text` | Rope data structure | Used by language/buffer |
| `multi_buffer` | Buffer abstraction | Used by Editor |
| `ui` | Icons, small components | Used by gutter |

A `init_zed_subsystems(cx)` function should be called during `GpuixRenderer::init()` to set up these globals. This is a one-time cost. When we split binaries later, this init moves to the editor plugin.

## File Structure (Phase 1)

```
packages/native/src/
  lib.rs
  renderer.rs              (modified: uses CustomElementRegistry)
  retained_tree.rs         (modified: custom_props field)
  element_tree.rs
  style.rs
  platform/
  custom_elements/
    mod.rs                 (trait defs + registry)
    editor.rs              (EditorElement + EditorFactory)
    diff.rs                (DiffElement + DiffFactory)
```

## Open Questions

- **Theme bridging:** How to let React apps configure the Zed theme. Options: pass a theme JSON from JS, or define a minimal "GPUIX theme" that maps to Zed's ThemeColors.
- **Keyboard input:** The Editor uses GPUI's `EntityInputHandler` for IME. Need to verify this works through the NodePlatform — the platform must support the text input protocol.
- **Focus integration:** When a custom element is focused, the React reconciler's focus system and the GPUI focus system need to agree. The Editor manages its own `FocusHandle` internally.
- **Sizing:** Editor in `Full` mode wants to fill available space. Need to make sure the parent div's layout gives it proper bounds via flexbox.
