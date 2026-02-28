---
title: gpui_node Platform Crate — Implementation Plan
description: Plan for creating a custom GPUI platform that runs inside Node.js via napi-rs, solving the main-thread blocking issue.
---

# gpui_node Platform Crate — Implementation Plan

## References

- **PR #50228 "GPUI on the web"**: https://github.com/zed-industries/zed/pull/50228
  Merged `2026-02-26`. Adds `gpui_web` crate that compiles GPUI to WASM and runs in the browser.
  Commit: `14f37ed5024bbccff2200797d1d9bf69aee01d66`

- **gpui_web source (at merge commit)**:
  https://github.com/zed-industries/zed/tree/14f37ed5024bbccff2200797d1d9bf69aee01d66/crates/gpui_web

- **gpui_platform crate** (dispatches to per-OS platform):
  https://github.com/zed-industries/zed/tree/14f37ed5024bbccff2200797d1d9bf69aee01d66/crates/gpui_platform

- **gpui_wgpu crate** (wgpu renderer shared by web + Linux):
  https://github.com/zed-industries/zed/tree/14f37ed5024bbccff2200797d1d9bf69aee01d66/crates/gpui_wgpu


## The Problem

GPUI's macOS platform calls `[NSApp run]` inside `Platform::run()`, which enters a
blocking native event loop that never returns. When our napi-rs binding calls
`gpui::Application::new().run(...)` (in `packages/native/src/renderer.rs:106`),
Node.js's V8 event loop dies after the first frame. No JS executes, no React state
updates, no re-renders.

```
renderer.run()                ← JS calls this
  → gpui::Application::run()
    → MacPlatform::run()
      → [NSApp run]           ← blocks forever
                                 Node.js event loop: dead
```


## The Solution

Create a **`gpui_node` platform crate** — a custom implementation of GPUI's `Platform`
and `PlatformDispatcher` traits designed for Node.js, modeled after `gpui_web`.

The key insight from PR #50228: **GPUI's core does not require a blocking event loop.**
`WebPlatform::run()` returns immediately and lets the browser drive rendering via
`requestAnimationFrame`. We do the same but with Node.js's libuv event loop.


## Architecture Overview

```
┌──────────────────────────────────────────────────────────────────┐
│  Node.js Process (main thread)                                   │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │  JavaScript / TypeScript                                   │  │
│  │                                                            │  │
│  │  React App                                                 │  │
│  │    ├── reconciler builds Instance tree                     │  │
│  │    ├── instanceToElementDesc() → JSON                      │  │
│  │    └── calls renderer.render(jsonTree)                     │  │
│  │                                                            │  │
│  │  Event handlers (onClick, etc.)                            │  │
│  │    ← called via ThreadsafeFunction from Rust               │  │
│  │    → trigger React state update → re-render → new JSON     │  │
│  └────────────────────────────────────────────────────────────┘  │
│         │ napi-rs FFI                    ↑ napi-rs callback      │
│  ┌──────┴────────────────────────────────┴────────────────────┐  │
│  │  Rust (napi-rs cdylib)                                     │  │
│  │                                                            │  │
│  │  GpuixRenderer (napi binding)                              │  │
│  │    ├── init() → creates NodePlatform, opens GPUI window    │  │
│  │    ├── render(json) → updates element tree, cx.notify()    │  │
│  │    └── tick() → pumps macOS events + GPUI foreground tasks │  │
│  │                                                            │  │
│  │  NodePlatform (implements gpui::Platform)                  │  │
│  │    ├── run() → returns immediately (non-blocking)          │  │
│  │    ├── open_window() → winit window + wgpu surface         │  │
│  │    └── NodeDispatcher                                      │  │
│  │         ├── dispatch() → std::thread pool                  │  │
│  │         ├── dispatch_on_main → napi ThreadsafeFunction     │  │
│  │         └── dispatch_after → queued with timestamp         │  │
│  │                                                            │  │
│  │  GPUI Core (unmodified)                                    │  │
│  │    ├── Taffy layout engine                                 │  │
│  │    ├── Element tree → Scene (draw commands)                │  │
│  │    └── gpui_wgpu WgpuRenderer → Metal/Vulkan               │  │
│  └────────────────────────────────────────────────────────────┘  │
│                              ↓                                   │
│                         Native GPU                               │
│                    Metal (macOS) / Vulkan (Linux)                 │
└──────────────────────────────────────────────────────────────────┘
```


## How the Main Thread Blocking Is Solved

Instead of calling `Application::run()` (which delegates to `MacPlatform::run()` →
`[NSApp run]` blocking forever), we:

1. Create a `NodePlatform` where `run()` **returns immediately** — just like
   `WebPlatform::run()` does at `gpui_web/src/platform.rs:103-118`.

2. Expose a **`tick()` method** from napi-rs that JS calls on every iteration of the
   Node.js event loop (via `setImmediate` or a libuv `uv_prepare` handle). Each tick:
   - Pumps pending macOS events: `CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0, true)`
   - Drains GPUI's foreground task queue (runnables dispatched to main thread)
   - Optionally triggers a frame render

3. The **Node.js event loop drives everything**: JS runs, React reconciles, Rust renders.
   Neither side blocks the other.

```
Node.js event loop tick
  ├── JS: process I/O, timers, microtasks
  ├── JS: React reconciler may produce new tree → calls renderer.render(json)
  └── JS: calls renderer.tick()
       └── Rust: pump macOS events + GPUI tasks + render frame
```


## Detailed Component Design

### 1. NodePlatform (Rust, implements `gpui::Platform`)

**Reference**: `gpui_web/src/platform.rs` (341 lines)

```rust
pub struct NodePlatform {
    background_executor: BackgroundExecutor,
    foreground_executor: ForegroundExecutor,
    text_system: Arc<dyn PlatformTextSystem>,
    active_window: RefCell<Option<AnyWindowHandle>>,
    wgpu_context: RefCell<Option<WgpuContext>>,
}
```

**Key methods:**

| Method | Implementation | Reference |
|--------|---------------|-----------|
| `run(on_finish_launching)` | Init wgpu synchronously (native, can block briefly), call callback, return | `gpui_web/platform.rs:103` — web does async init; we do sync via `pollster::block_on` |
| `open_window(handle, params)` | Create `winit::Window`, create wgpu surface from it, return `NodeWindow` | `gpui_web/platform.rs:146` — web creates `<canvas>` + wgpu surface |
| `text_system()` | Return `CosmicTextSystem` (same as web uses in `gpui_wgpu`) | `gpui_web/platform.rs:63` — uses `CosmicTextSystem` |
| `background_executor()` | Standard `BackgroundExecutor` with `NodeDispatcher` | Same pattern as web |
| `quit()` | Set a flag, JS checks it on next tick | Web: no-op |
| Everything else | No-op stubs (clipboard, menus, file dialogs, etc.) | Web stubs everything too |

### 2. NodeDispatcher (Rust, implements `gpui::PlatformDispatcher`)

**Reference**: `gpui_web/src/dispatcher.rs` (333 lines)

The web dispatcher uses `setTimeout`, `queueMicrotask`, `Atomics.waitAsync`, and
Web Workers. Our Node.js equivalent:

| Web API | Node.js Equivalent |
|---------|-------------------|
| `setTimeout(cb, ms)` | Store `(Instant + duration, runnable)` in a queue, drain during `tick()` |
| `queueMicrotask(cb)` | Execute immediately or push to high-priority queue |
| `requestAnimationFrame(cb)` | Timer-based or manual `tick()` call |
| `wasm_thread::spawn` (Web Workers) | `std::thread::spawn` (native threads) |
| `MainThreadMailbox` + `Atomics.waitAsync` | `napi::ThreadsafeFunction` to post back to V8 main thread |
| `SharedArrayBuffer` + `Atomics` | `std::sync::mpsc` or `crossbeam-channel` |

```rust
pub struct NodeDispatcher {
    main_thread_id: ThreadId,
    background_sender: PriorityQueueSender<RunnableVariant>,
    main_thread_queue: Arc<Mutex<Vec<RunnableVariant>>>,
    delayed_queue: Arc<Mutex<BinaryHeap<DelayedRunnable>>>,
    // napi ThreadsafeFunction to call back into JS/V8 main thread
    // (used when a background thread needs to dispatch to main)
    main_thread_waker: Arc<dyn Fn() + Send + Sync>,
}
```

**Key methods:**

| Method | Implementation |
|--------|---------------|
| `dispatch(runnable, priority)` | Send to background thread pool via channel |
| `dispatch_on_main_thread(runnable, priority)` | If already on main: push to `main_thread_queue`. If on bg thread: push + signal via `main_thread_waker` |
| `dispatch_after(duration, runnable)` | Push to `delayed_queue` with `Instant::now() + duration` |
| `is_main_thread()` | `thread::current().id() == self.main_thread_id` |
| `now()` | `Instant::now()` |

**Draining during tick:**

```rust
pub fn drain_main_thread_queue(&self) {
    // 1. Drain immediate runnables
    let runnables: Vec<_> = self.main_thread_queue.lock().drain(..).collect();
    for runnable in runnables {
        if !runnable.metadata().is_closed() {
            runnable.run();
        }
    }

    // 2. Drain delayed runnables whose time has passed
    let now = Instant::now();
    let mut delayed = self.delayed_queue.lock();
    while let Some(entry) = delayed.peek() {
        if entry.deadline <= now {
            let entry = delayed.pop().unwrap();
            if !entry.runnable.metadata().is_closed() {
                entry.runnable.run();
            }
        } else {
            break;
        }
    }
}
```

### 3. NodeWindow (Rust, implements `gpui::PlatformWindow`)

**Reference**: `gpui_web/src/window.rs` (689 lines)

| Web | Node.js |
|-----|---------|
| `document.createElement("canvas")` | `winit::Window::new()` — creates native OS window |
| `wgpu::SurfaceTarget::Canvas(canvas)` | `wgpu::SurfaceTarget::Window(Box::new(winit_window))` |
| `requestAnimationFrame` loop | `tick()` call triggers `request_frame` callback |
| `ResizeObserver` | `winit` `WindowEvent::Resized` |
| Canvas pointer/keyboard events | `winit` `WindowEvent::*` events |
| `window.device_pixel_ratio()` | `winit_window.scale_factor()` |

```rust
pub struct NodeWindow {
    winit_window: winit::window::Window,
    renderer: WgpuRenderer,
    callbacks: RefCell<NodeWindowCallbacks>,
    bounds: RefCell<Bounds<Pixels>>,
    scale_factor: Cell<f32>,
    mouse_position: Cell<Point<Pixels>>,
    modifiers: Cell<Modifiers>,
}
```

**Key methods:**

| Method | Implementation |
|--------|---------------|
| `draw(scene)` | `self.renderer.draw(scene)` — wgpu renders to native surface |
| `completed_frame()` | No-op (wgpu presents automatically) |
| `on_request_frame(cb)` | Store callback; `tick()` calls it each frame |
| `on_input(cb)` | Store callback; winit events → `PlatformInput` → callback |
| `on_resize(cb)` | Store callback; winit resize → callback |

### 4. GpuixRenderer (napi-rs, exposed to JS)

This replaces the current `packages/native/src/renderer.rs`. Instead of calling
`gpui::Application::new().run(...)` (blocking), it:

```rust
#[napi]
pub struct GpuixRenderer {
    app: Option<Rc<AppCell>>,        // GPUI application state (non-blocking)
    window_handle: Option<AnyWindowHandle>,
    current_tree: Arc<Mutex<Option<ElementDesc>>>,
    event_callback: Option<ThreadsafeFunction<EventPayload>>,
    dispatcher: Arc<NodeDispatcher>,
}

#[napi]
impl GpuixRenderer {
    /// Initialize GPUI with NodePlatform (non-blocking).
    /// Creates a native window and wgpu rendering surface.
    #[napi]
    pub fn init(&mut self, options: WindowOptions) -> Result<()> {
        let platform = Rc::new(NodePlatform::new());
        let app = Application::with_platform(platform);

        // run() returns immediately because NodePlatform::run() is non-blocking
        app.run(|cx| {
            let window = cx.open_window(options, |window, cx| {
                cx.new(|_| GpuixView { tree: self.current_tree.clone(), ... })
            });
            self.window_handle = Some(window);
        });

        Ok(())
    }

    /// Send a new element tree to GPUI. Triggers re-render.
    #[napi]
    pub fn render(&self, tree_json: String) -> Result<()> {
        let tree: ElementDesc = serde_json::from_str(&tree_json)?;
        *self.current_tree.lock().unwrap() = Some(tree);

        // Notify GPUI that the view needs re-rendering
        if let Some(handle) = self.window_handle {
            // cx.notify() on the GpuixView entity
        }
        Ok(())
    }

    /// Pump the event loop. Call this from JS on every tick.
    /// Processes: macOS events, GPUI foreground tasks, pending renders.
    #[napi]
    pub fn tick(&self) -> Result<()> {
        // 1. Pump native OS events (macOS: CFRunLoopRunInMode)
        self.pump_native_events();

        // 2. Drain GPUI dispatcher's main-thread queue
        self.dispatcher.drain_main_thread_queue();

        // 3. Trigger frame render if needed
        if let Some(ref window) = self.node_window {
            window.request_frame();
        }

        Ok(())
    }
}
```


## Who Does What

### Who creates the window?

**Rust** creates the window, using `winit` (cross-platform window library).
When JS calls `renderer.init()`, Rust creates a `winit::Window` and a wgpu surface
from its native handle. GPUI's `open_window()` receives this as a `NodeWindow`.

### Who does the rendering?

**GPUI + wgpu** renders, completely in Rust:

```
React tree (JSON) → build_element() → GPUI elements
  → Taffy layout → Scene (draw commands)
  → WgpuRenderer.draw(scene) → Metal/Vulkan GPU commands
  → wgpu Surface present → pixels on screen
```

JS never touches pixels. It only sends the element description.

### Who drives the frame loop?

**Node.js** drives the frame loop by calling `renderer.tick()` periodically:

```typescript
// Option A: setImmediate loop (yields to I/O between frames)
function loop() {
  renderer.tick()
  setImmediate(loop)
}
loop()

// Option B: Fixed frame rate
setInterval(() => renderer.tick(), 16) // ~60fps

// Option C: On-demand (render only when tree changes)
function onReactRender(tree: ElementDesc) {
  renderer.render(JSON.stringify(tree))
  renderer.tick() // render immediately
}
```

### Where do click handlers execute?

**In JavaScript**, on the Node.js main thread:

```
1. User clicks on a GPUI element (native OS event)
2. renderer.tick() pumps macOS events
3. winit delivers WindowEvent::MouseInput to Rust
4. Rust maps to GPUI PlatformInput::MouseDown
5. GPUI dispatches to the element with matching ID
6. Element's on_click closure fires (set up in build_element)
7. Closure calls ThreadsafeFunction → crosses into JS
8. JS event-registry.ts looks up handler by element ID
9. React handler runs: onClick={() => setCount(c => c + 1)}
10. React reconciler produces new tree → renderer.render(json)
11. Next tick() renders the updated UI
```

This is the same flow as today, except `tick()` replaces the blocking event loop.

### How does headless / image rendering work?

For rendering to images (screenshots, PDFs), skip the window entirely:

```rust
// Create an offscreen wgpu texture instead of a window surface
let texture = device.create_texture(&TextureDescriptor {
    size: Extent3d { width: 1920, height: 1080, depth_or_array_layers: 1 },
    format: TextureFormat::Rgba8Unorm,
    usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    ..Default::default()
});

// Render scene to the texture
renderer.draw_to_texture(&scene, &texture);

// Read pixels back to CPU
let buffer = device.create_buffer(&BufferDescriptor {
    size: (1920 * 1080 * 4) as u64,
    usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
    ..Default::default()
});
encoder.copy_texture_to_buffer(...);

// Return pixel data to JS as Buffer
```


## File Structure

```
packages/native/
├── Cargo.toml              # Updated deps: gpui (pinned), winit, wgpu, etc.
├── build.rs
├── src/
│   ├── lib.rs              # Module exports
│   ├── renderer.rs         # GpuixRenderer napi binding (rewritten)
│   ├── element_tree.rs     # ElementDesc, EventPayload (keep as-is)
│   ├── style.rs            # StyleDesc, color parsing (keep as-is)
│   ├── platform/
│   │   ├── mod.rs
│   │   ├── node_platform.rs   # NodePlatform: implements gpui::Platform
│   │   ├── node_dispatcher.rs # NodeDispatcher: implements PlatformDispatcher
│   │   ├── node_window.rs     # NodeWindow: implements PlatformWindow
│   │   └── node_display.rs    # NodeDisplay: implements PlatformDisplay
│   └── view.rs             # GpuixView: Render impl + build_element()
```


## Implementation Steps

### Step 1: Update dependencies

Update `Cargo.toml` to depend on GPUI at the same commit as the web PR
(`14f37ed5024bbccff2200797d1d9bf69aee01d66`), plus `winit` and `gpui_wgpu`:

```toml
[dependencies]
gpui = { git = "https://github.com/zed-industries/zed", rev = "14f37ed5" }
gpui_wgpu = { git = "https://github.com/zed-industries/zed", rev = "14f37ed5" }
winit = "0.30"
napi = { version = "2", features = ["napi8", "serde-json"] }
napi-derive = "2"
parking_lot = "0.12"
```

### Step 2: Implement NodeDispatcher

Reference: `gpui_web/src/dispatcher.rs` (333 lines)

This is the simplest component. No browser APIs — just channels and thread pools.
Most of the web dispatcher's complexity comes from `SharedArrayBuffer` / `Atomics`
for cross-thread WASM signaling. In native Rust, `std::sync::mpsc` just works.

**Tests**: unit test that dispatch/dispatch_after/drain work correctly.

### Step 3: Implement NodeDisplay

Reference: `gpui_web/src/display.rs` (98 lines)

Trivial — return fixed screen bounds. Can later query from winit.

### Step 4: Implement NodePlatform

Reference: `gpui_web/src/platform.rs` (341 lines)

Copy the structure from `WebPlatform`. Most methods are stubs. The important ones:
- `run()` — returns immediately (init wgpu synchronously)
- `open_window()` — creates winit window + wgpu surface → returns `NodeWindow`
- `text_system()` — return `CosmicTextSystem` from `gpui_wgpu`

### Step 5: Implement NodeWindow

Reference: `gpui_web/src/window.rs` (689 lines)

This is the largest component. Maps winit events to GPUI PlatformInput.
The web version has ~200 lines of event mapping in `events.rs`; our version
will be similar but mapping winit events instead of DOM events.

**Winit event mapping:**

| winit Event | GPUI PlatformInput |
|-------------|-------------------|
| `WindowEvent::CursorMoved` | `MouseMoveEvent` |
| `WindowEvent::MouseInput` | `MouseDownEvent` / `MouseUpEvent` |
| `WindowEvent::MouseWheel` | `ScrollWheelEvent` |
| `WindowEvent::KeyboardInput` | `KeyDownEvent` / `KeyUpEvent` |
| `WindowEvent::Resized` | resize callback |
| `WindowEvent::Focused` | active_status_change callback |
| `WindowEvent::CursorEntered/Left` | hover_status_change callback |

### Step 6: Rewrite GpuixRenderer napi binding

Replace the current blocking `run()` with `init()` + `render()` + `tick()`.

### Step 7: Update JS side

Update `@gpuix/react` and the counter example to use the new API:

```typescript
const renderer = createRenderer(onEvent)

// Non-blocking init — creates window, returns immediately
renderer.init({ title: 'GPUIX Counter', width: 800, height: 600 })

// Create React root and render
const root = createRoot(renderer)
root.render(<App />)

// Drive the event loop
function loop() {
  renderer.tick()
  setImmediate(loop)
}
loop()
```


## Open Questions

1. **winit event loop vs manual pumping**: winit's `EventLoop::run()` also blocks.
   We need winit's `EventLoop::pump_events()` (added in winit 0.30) which processes
   pending events and returns. This maps perfectly to our `tick()` model.

2. **macOS main thread requirement**: Cocoa requires UI operations on the main thread.
   Since Node.js runs on the main thread and our `tick()` is called from JS (main
   thread), this should work — winit + Cocoa events are pumped on the correct thread.

3. **Frame timing**: Should `tick()` always trigger a frame, or only when the tree
   changed? For efficiency, only render when dirty. But for animations, we'd need
   a timer-based frame loop. Start with always-render, optimize later.

4. **wgpu device creation**: On native, wgpu device creation is synchronous
   (`pollster::block_on`). This briefly blocks the Node.js main thread (~100ms on
   first init). Acceptable for startup.

5. **Multiple windows**: The current design supports one window. For multiple windows,
   we'd need a window registry and per-window tick handling. Defer to later.


## Future Work: Improving the JS ↔ Rust Bridge

The current approach sends the **entire element tree as JSON** on every React render.
This works and aligns with GPUI's immediate-mode model (it rebuilds the full element
tree every frame anyway), but there are better approaches for the future.

### Current approach: full JSON tree per render

```
React reconciler → instanceToElementDesc() → JSON.stringify(fullTree)
  → napi FFI (string) → serde_json::from_str → build_element() → GPUI elements
```

**Why it's OK for now:**
- GPUI is immediate-mode — it rebuilds the full element tree every frame regardless
- For typical UIs (100-1000 elements), JSON serialization is ~1-5ms, well under 16ms
- Simple to implement and debug
- The bottleneck is the blocking event loop, not serialization

**Limitations:**
- Serialization/deserialization overhead scales with tree size
- React already computes diffs, but we throw them away and send the full tree
- Limited to element types we've manually mapped (`div`, `text`)
- Every new GPUI feature needs mapping in both JS types and Rust `build_element()`
- Event handlers require 2 FFI crossings (Rust→JS delivery, JS→Rust re-render)
- No access to GPUI features like `cx.spawn()`, entities, subscriptions

### Phase 2: Mutation-based protocol (React Native model)

Instead of sending the full tree, forward React reconciler mutations as individual
napi calls. React already computes exactly what changed — we just forward those
changes instead of rebuilding the full description.

```rust
#[napi]
pub fn create_element(&self, id: String, element_type: String) -> Result<()> { ... }
#[napi]
pub fn set_style(&self, id: String, property: String, value: String) -> Result<()> { ... }
#[napi]
pub fn set_text(&self, id: String, content: String) -> Result<()> { ... }
#[napi]
pub fn append_child(&self, parent_id: String, child_id: String) -> Result<()> { ... }
#[napi]
pub fn remove_child(&self, parent_id: String, child_id: String) -> Result<()> { ... }
#[napi]
pub fn set_event_listener(&self, id: String, event_type: String, has_handler: bool) -> Result<()> { ... }
```

The Rust side maintains a **retained element tree** (HashMap<String, ElementNode>)
and only updates the parts that changed. On each frame, GPUI's `Render::render()`
reads from this retained tree.

This is how React Native's Fabric renderer works — the reconciler sends mutations
through the bridge, and the native side maintains the view hierarchy.

**Benefits:**
- No serialization overhead
- Only changed elements cross the FFI boundary
- Each napi call is typed (no JSON parsing)
- Natural fit for React's reconciler model

### Phase 3: Binary protocol with SharedArrayBuffer

For maximum performance, use a compact binary format in shared memory:

- Allocate a SharedArrayBuffer between JS and Rust
- JS writes element mutations as packed binary commands
- Rust reads them directly — zero copy, zero serialization
- Use Atomics for synchronization

This is only worth doing if profiling shows the FFI boundary is the bottleneck,
which is unlikely for typical UI workloads.

### Phase 4: Expose more GPUI primitives to JS

Instead of only exposing `div` and `text`, expose GPUI's full element vocabulary:
- `img()` — GPU-accelerated images
- `svg()` — vector graphics
- `canvas()` — custom drawing
- `uniform_list()` — virtualized lists
- `deferred()` / `anchored()` — popovers and tooltips
- Custom `Element` trait implementations via Rust plugins

This would let React components use GPUI's full power while keeping state
management in JS/React.
