# AGENTS.md - GPUIX Codebase Guide

## Project Goal

GPUIX enables building **native GPU-accelerated desktop applications** using **React and TypeScript**, powered by [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) (Zed's rendering framework).

Instead of Electron/web rendering, your React components render directly to the GPU via Metal/Vulkan.

```
React (TypeScript)  →  napi-rs  →  GPUI (Rust)  →  GPU
     Your code         Bridge      Native render    Metal/Vulkan
```

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│  JavaScript / TypeScript                                        │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  Your React App                                          │   │
│  │                                                          │   │
│  │  function App() {                                        │   │
│  │    const [count, setCount] = useState(0)                 │   │
│  │    return (                                              │   │
│  │      <div style={{ display: 'flex', bg: '#1e1e2e' }}>    │   │
│  │        <div onClick={() => setCount(c => c + 1)}>+</div> │   │
│  │      </div>                                              │   │
│  │    )                                                     │   │
│  │  }                                                       │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              ↓                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  @gpuix/react (packages/react)                           │   │
│  │                                                          │   │
│  │  - React Reconciler (react-reconciler)                   │   │
│  │  - Builds element tree from React components             │   │
│  │  - Serializes to JSON ElementDesc                        │   │
│  │  - Manages event handler registry                        │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              ↓ JSON                             │
└─────────────────────────────────────────────────────────────────┘
                               ↓ napi-rs FFI
┌─────────────────────────────────────────────────────────────────┐
│  Rust / Native                                                  │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  @gpuix/native (packages/native)                         │   │
│  │                                                          │   │
│  │  - GpuixRenderer: receives JSON, triggers re-render      │   │
│  │  - build_element(): ElementDesc → GPUI elements          │   │
│  │  - apply_styles(): StyleDesc → GPUI style methods        │   │
│  │  - Event handlers → ThreadsafeFunction callbacks to JS   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              ↓                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  GPUI (from zed)                                         │   │
│  │                                                          │   │
│  │  - Immediate-mode UI framework                           │   │
│  │  - Flexbox layout via Taffy                              │   │
│  │  - GPU rendering via Metal (macOS) / Vulkan (Linux)      │   │
│  │  - Native window management                              │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Key Insight: Immediate Mode Alignment

GPUI is **immediate-mode** - it rebuilds the entire UI tree every frame. This actually aligns perfectly with React's model:

| Traditional DOM Renderer | GPUIX |
|--------------------------|-------|
| `appendChild(node)` | Rebuild tree each render |
| `node.style.color = x` | Send full tree description |
| Mutation-based | Description-based |

We don't fight GPUI's architecture - we embrace it by sending a complete element description on every React render.

## Package Structure

```
gpuix/
├── packages/
│   ├── native/                 # Rust napi-rs bindings
│   │   ├── src/
│   │   │   ├── lib.rs          # Module exports
│   │   │   ├── renderer.rs     # GpuixRenderer, GpuixView, build_element()
│   │   │   ├── element_tree.rs # ElementDesc, EventPayload types
│   │   │   └── style.rs        # StyleDesc, color parsing
│   │   ├── examples/
│   │   │   └── hello.rs        # Pure GPUI test (no JS)
│   │   ├── Cargo.toml
│   │   └── build.rs
│   │
│   └── react/                  # React reconciler
│       ├── src/
│       │   ├── index.ts        # Public exports
│       │   ├── reconciler/
│       │   │   ├── host-config.ts  # React reconciler implementation
│       │   │   ├── reconciler.ts   # ReactReconciler instance
│       │   │   └── renderer.ts     # createRoot(), event bridge
│       │   ├── hooks/
│       │   │   ├── use-gpuix.ts    # Context access
│       │   │   └── use-window-size.ts
│       │   └── types/
│       │       └── host.ts     # TypeScript types
│       └── package.json
│
├── examples/
│   └── counter.tsx             # Example React app
│
└── AGENTS.md                   # This file
```

## Communication Flow

### Render Flow (JS → Rust)

```
1. React state changes
         ↓
2. React reconciler builds Instance tree
         ↓
3. instanceToElementDesc() converts to JSON-serializable format:
   {
     type: "div",
     id: "btn-1", 
     style: { display: "flex", backgroundColor: "#ff0000" },
     events: ["click", "mouseEnter"],
     children: [...]
   }
         ↓
4. renderer.render(JSON.stringify(tree))
         ↓
5. Rust parses JSON into ElementDesc structs
         ↓
6. build_element() recursively builds GPUI elements:
   div().id("btn-1").flex().bg(rgba(0xff0000ff)).on_click(...)
         ↓
7. GPUI renders to GPU
```

### Event Flow (Rust → JS)

```
1. User clicks element with id="btn-1"
         ↓
2. GPUI fires click event on element
         ↓
3. Rust closure calls emit_event("btn-1", "click", position)
         ↓
4. ThreadsafeFunction calls into JS with EventPayload
         ↓
5. JS event registry looks up handler:
   eventHandlers.get("btn-1")?.click?.(event)
         ↓
6. React handler runs: onClick={() => setCount(c => c + 1)}
         ↓
7. State update triggers re-render → back to Render Flow
```

## Key Types

### ElementDesc (Rust ↔ JS)

```rust
pub struct ElementDesc {
    pub element_type: String,      // "div", "text", "img"
    pub id: Option<String>,        // For event handling
    pub style: Option<StyleDesc>,  // CSS-like styles
    pub content: Option<String>,   // Text content
    pub events: Option<Vec<String>>, // ["click", "mouseEnter"]
    pub children: Option<Vec<ElementDesc>>,
}
```

### StyleDesc (CSS-like properties)

```rust
pub struct StyleDesc {
    // Flexbox
    pub display: Option<String>,        // "flex"
    pub flex_direction: Option<String>, // "row", "column"
    pub align_items: Option<String>,    // "center", "start", "end"
    pub justify_content: Option<String>,
    pub gap: Option<f64>,
    
    // Sizing
    pub width: Option<f64>,
    pub height: Option<f64>,
    
    // Spacing
    pub padding: Option<f64>,
    pub margin: Option<f64>,
    
    // Colors (parsed from "#rrggbb" or "rgb(r,g,b)")
    pub background_color: Option<String>,
    pub color: Option<String>,
    
    // Border
    pub border_radius: Option<f64>,
    pub border_width: Option<f64>,
    pub border_color: Option<String>,
}
```

### EventPayload (Rust → JS)

```rust
pub struct EventPayload {
    pub element_id: String,
    pub event_type: String,  // "click", "mouseEnter", etc.
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub key: Option<String>,
    pub modifiers: Option<EventModifiers>,
}
```

## Building

### Development (in zed workspace)

The native package depends on GPUI which has complex dependencies. For now, develop inside the zed workspace:

```bash
# In zed repo
cd crates/gpuix
cargo run --example hello --release
```

### Standalone Build (blocked)

Building outside zed workspace fails due to dependency conflicts:
- `core-graphics` version mismatch (0.24 vs 0.25)
- `font-kit` fork requirements

See "Current Blockers" below.

## Current Status

### Completed

- [x] React reconciler structure (based on opentui pattern)
- [x] Element tree serialization (ElementDesc)
- [x] Style mapping (CSS-like → GPUI style methods)
- [x] napi-rs bindings structure
- [x] GpuixRenderer with render() and run()
- [x] build_element() - converts ElementDesc to GPUI elements
- [x] apply_styles() - maps all common CSS properties
- [x] Event wiring (click, mouseDown, mouseUp, mouseMove)
- [x] ThreadsafeFunction callback to JS for events
- [x] Pure Rust example (hello.rs) - works in zed workspace

### TODO

#### High Priority

- [ ] **Build native package standalone** - Resolve GPUI dependency conflicts
- [ ] **Generate TypeScript types** - Run napi build to create .d.ts files
- [ ] **Test full pipeline** - JS → native → GPUI → screen
- [ ] **Re-render triggering** - Store Entity handle, call cx.notify() on tree update

#### Medium Priority

- [ ] **Focus management** - Wire up FocusHandle for keyboard events
- [ ] **onKeyDown/onKeyUp** - Keyboard event handlers
- [ ] **onFocus/onBlur** - Focus event handlers
- [ ] **Text input** - Handle text input (GPUI has no built-in input element)
- [ ] **More elements** - img (gpui::img), svg (gpui::svg), list (virtualized)

#### Low Priority

- [ ] **Window controls** - setTitle, resize, minimize, etc.
- [ ] **Multiple windows** - Support multiple GPUI windows
- [ ] **Hot reload** - Re-render on JS file changes
- [ ] **DevTools** - React DevTools integration
- [ ] **Animations** - Interpolated style transitions

## Current Blockers

### 1. GPUI Dependency Resolution

GPUI uses workspace-level dependencies and a custom font-kit fork. Building outside zed fails:

```
error: two different versions of crate `core_graphics` in the dependency graph
  - core-graphics 0.24.0 (from core-text)
  - core-graphics 0.25.0 (from font-kit fork)
```

**Workaround**: Develop in `zed/crates/gpuix`, copy built artifacts.

**Solution needed**: Either:
- Zed publishes GPUI as standalone crate
- We vendor all dependencies
- We patch Cargo.toml with exact version pins

### 2. napi-rs in Non-Node Context

The Rust example can't link because napi symbols aren't available outside Node.js:

```
Undefined symbols: _napi_call_function, _napi_create_string_utf8, ...
```

**Solution**: Examples should either:
- Be pure GPUI (like hello.rs)
- Run via Node.js/Bun with the built .node binary

## Testing

### Unit Tests (TODO)

```bash
# Test React reconciler
cd packages/react && bun test

# Test Rust element building
cd packages/native && cargo test
```

### Integration Test

```bash
# Once native builds work:
bun examples/counter.tsx
```

## Related Projects

- [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) - Zed's GPU UI framework
- [opentui](https://github.com/anomalyco/opentui) - Terminal UI with React (reconciler reference)
- [create-gpui-app](https://github.com/zed-industries/create-gpui-app) - Official GPUI starter template
- [react-reconciler](https://github.com/facebook/react/tree/main/packages/react-reconciler) - React's custom renderer API

## Contributing

1. For Rust changes, work in `zed/crates/gpuix` (easier to build)
2. Copy changes to `gpuix/packages/native/src/` when ready
3. TypeScript changes can be made directly in `packages/react/`
