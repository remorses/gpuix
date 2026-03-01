---
title: Native Test Renderer Plan
description: Migrate from mocked TypeScript TestRenderer to real GPUI-backed test renderer using gpui's TestPlatform (no GPU, no window). Tests exercise the same Rust rendering pipeline as production.
---

# Native Test Renderer Plan

## Goal

Replace the mocked `TestRenderer` (pure TypeScript, in-memory element tree) with a real native test renderer backed by GPUI's test infrastructure. Tests run the **same Rust code** as production — `build_element()`, `apply_styles()`, event wiring, focus management — but with no GPU and no visible window.

## Current State

`packages/react/src/testing.ts` contains a `TestRenderer` class that:

- Maintains an in-memory element tree in TypeScript
- Simulates events by dispatching directly to the JS event registry
- Never touches Rust/GPUI — no layout, no style application, no real event dispatching
- Events bypass GPUI's hit-test system and focus model

## Research Findings

### gpui test-support feature

The gpui crate at our pinned commit (`14f37ed5024b...`) has a `test-support` feature that is safe to enable on macOS:

```toml
# gpui's Cargo.toml
[features]
test-support = [
    "leak-detection",
    "collections/test-support",
    "http_client/test-support",
    "wayland",    # → bitflags (cross-platform, no-op on macOS)
    "x11",        # → scap?/x11 (scap not present on macOS, ? = no-op)
]
```

### Accessible types (with test-support feature)

| Type | Visibility | Purpose |
|---|---|---|
| `TestAppContext` | **pub** | Creates TestPlatform internally, sets GpuiMode::test() |
| `TestDispatcher` | **pub** | Deterministic scheduler, seeded RNG |
| `VisualTestContext` | **pub** (cross-platform) | Window-scoped: `simulate_click()`, `simulate_keystrokes()`, `draw()` |

### Inaccessible types (pub(crate), NOT a blocker)

| Type | Why not a blocker |
|---|---|
| `TestPlatform` | Created internally by `TestAppContext::single()` |
| `TestWindow` | Created internally by `add_window_view()` — draw() is a no-op |
| `TestDisplay` | Created internally — returns fixed 1920×1080 |
| `GpuiMode` | Set internally by `TestAppContext` to `GpuiMode::test()` |

### Key API details

- `TestAppContext::single()` — no-arg constructor, creates everything internally
- `TestAppContext::add_window_view(|window, cx| { ... })` — opens a test window, returns `(Entity<V>, &mut VisualTestContext)`
- `VisualTestContext::simulate_click(position, modifiers)` — dispatches MouseDown + MouseUp through TestWindow's input callback, then `run_until_parked()`
- **Critical**: `cx.draw()` must be called before simulating events — GPUI's hit testing requires elements to be laid out first

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  vitest (JS)                                                │
│                                                             │
│  const t = new TestGpuixRenderer(cb)                        │
│  t.createElement(1, "div")                                  │
│  t.appendChild(0, 1)                                        │
│  t.setRoot(1)                                               │
│  t.commitMutations()                                        │
│  t.flush()              ─────────────────────┐              │
│  t.simulateClick(50, 50) ────────────────────┤              │
│  const events = t.drainEvents() ─────────────┤              │
│  handleGpuixEvent(events[0])                 │              │
│  t.flush()                                   │              │
│  t.getAllText() ─────────────────────────────┤              │
└──────────────────────────────────────────────┤──────────────┘
                                               │ napi-rs
┌──────────────────────────────────────────────┤──────────────┐
│  Rust (TestGpuixRenderer)                    │              │
│                                              ▼              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  TestAppContext (gpui test infrastructure)            │   │
│  │    TestPlatform → no GPU, no OS window               │   │
│  │    TestWindow   → draw() is a no-op                  │   │
│  │    TestDispatcher → deterministic scheduler           │   │
│  ├──────────────────────────────────────────────────────┤   │
│  │  GpuixView (same as production)                      │   │
│  │    RetainedTree ← receives mutations from JS         │   │
│  │    build_element() → real GPUI elements              │   │
│  │    apply_styles() → real GPUI style methods          │   │
│  │    event handlers → push to EventCollector           │   │
│  ├──────────────────────────────────────────────────────┤   │
│  │  flush() → draw() + run_until_parked()               │   │
│  │  simulateClick() → VisualTestContext::simulate_click │   │
│  │  drainEvents() → return collected EventPayloads      │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### Shared code between production and test

```
Production:  GpuixRenderer → NodePlatform → winit + wgpu → GPU
Tests:       TestGpuixRenderer → TestPlatform → no-op draw → no GPU

Both use the exact same:
  GpuixView::render()
  build_element() / build_div() / build_text()
  apply_styles()
  emit_event_full()
  sync_focus_handles()
  RetainedTree
```

## Event Callback Abstraction

Current `GpuixView` stores `Option<ThreadsafeFunction<EventPayload>>`. `ThreadsafeFunction` is async (queues on Node.js event loop), breaking synchronous test flow.

**Solution**: Abstract the callback as `Arc<dyn Fn(EventPayload) + Send + Sync>`:

- **Production**: wraps `ThreadsafeFunction.call(NonBlocking)` — async, queued on Node.js event loop
- **Tests**: wraps `Arc<Mutex<Vec<EventPayload>>>.push()` — synchronous collection

Both are Clone + Send + Sync. `build_div` and `emit_event_full` work unchanged — they just call the callback.

## Files to Change

### 1. `packages/native/Cargo.toml`

Add feature flag:

```toml
[features]
default = []
test-support = ["gpui/test-support"]
```

### 2. `packages/native/src/lib.rs`

Add conditional module:

```rust
#[cfg(feature = "test-support")]
mod test_renderer;
```

### 3. `packages/native/src/renderer.rs` (refactor)

- Change `GpuixView.event_callback` type from `Option<ThreadsafeFunction<EventPayload>>` to `Arc<dyn Fn(EventPayload) + Send + Sync>`
- Update `build_div`, `emit_event_full` to use the new callback type
- Production `GpuixRenderer` wraps ThreadsafeFunction in the Arc at construction time
- Consider extracting `GpuixView` to its own module so both renderers can import it

### 4. New file: `packages/native/src/test_renderer.rs`

`TestGpuixRenderer` napi class:

```rust
#[napi]
pub struct TestGpuixRenderer {
    tree: Arc<Mutex<RetainedTree>>,
    events: Arc<Mutex<Vec<EventPayload>>>,
}
```

**Constructor**: creates `TestAppContext::single()`, opens window with `GpuixView` (shared RetainedTree), stores TestAppContext + VisualTestContext in `thread_local!` (they are !Send).

**Mutation API** (same as GpuixRenderer):
- `createElement(id, elementType)`
- `destroyElement(id) → Vec<f64>`
- `appendChild(parentId, childId)`
- `removeChild(parentId, childId)`
- `insertBefore(parentId, childId, beforeId)`
- `setStyle(id, styleJson)`
- `setText(id, content)`
- `setEventListener(id, eventType, hasHandler)`
- `setRoot(id)`
- `commitMutations()`

**Test-specific methods**:
- `flush()` — calls `draw()` + `run_until_parked()` via VisualTestContext. Triggers GpuixView::render() which rebuilds GPUI elements from RetainedTree.
- `simulateClick(x, y)` — calls `VisualTestContext::simulate_click(point, modifiers)`
- `simulateKeyDown(key, modifiers?)` — calls `VisualTestContext::simulate_keystrokes()`
- `simulateMouseMove(x, y)` — calls `simulate_event(MouseMoveEvent { ... })`
- `drainEvents()` — returns and clears collected `Vec<EventPayload>`

**Tree inspection methods** (reads from RetainedTree):
- `getAllText() → Vec<String>` — DFS walk collecting text content
- `findByType(elementType) → Vec<f64>` — element IDs matching type
- `getTreeJson() → String` — full tree serialization for snapshot testing
- `getElement(id) → Option<ElementInfo>` — single element info (type, style, events, children)

### 5. `packages/native/index.d.ts`

Add type declarations for `TestGpuixRenderer` (auto-generated by napi build with test-support feature).

### 6. `packages/react/src/testing.ts`

Replace mock `TestRenderer` with wrapper around `TestGpuixRenderer`:

- `createTestRoot()` creates a `TestGpuixRenderer` and wires it as the `NativeRenderer`
- `simulateClick(elementId)` → look up element position from tree, call `testRenderer.simulateClick(x, y)`
- After each simulate, call `drainEvents()` and process through `handleGpuixEvent()` with `flushSync`
- Tree inspection delegates to napi methods

### 7. `packages/react/src/__tests__/events.test.tsx`

Adapt tests to new API. Main differences:

- Events go through GPUI pipeline (need `flush()` between mutations and assertions)
- `simulateClick` by element ID still works (testing.ts wrapper handles coordinate lookup)
- Same snapshot assertions should pass if the pipeline is correct

## Build Strategy

Two napi builds — same source, different features:

- **Default** (`cargo build`): production `.node` binary, no test infrastructure
- **test-support** (`cargo build --features test-support`): test `.node` binary, includes `TestGpuixRenderer`

Test script in `packages/react/package.json`:

```json
"pretest": "cd ../native && napi build --features test-support",
"test": "vitest run"
```

## Test Flow Example

```typescript
// JS test
const testRenderer = new TestGpuixRenderer()

// React sends mutations
testRenderer.createElement(1, "div")
testRenderer.setStyle(1, '{"display":"flex"}')
testRenderer.setEventListener(1, "click", true)
testRenderer.setRoot(1)
testRenderer.commitMutations()

// GPUI renders (layout + hit areas registered)
testRenderer.flush()

// Simulate click at coordinates
testRenderer.simulateClick(50, 50)

// Collect events (synchronous)
const events = testRenderer.drainEvents()
// events[0] = { elementId: 1, eventType: "click", x: 50, y: 50, ... }

// Process through React
flushSync(() => {
  for (const e of events) handleGpuixEvent(e)
})

// React re-renders, sends new mutations
testRenderer.flush()

// Assert
expect(testRenderer.getAllText()).toEqual(["Count: 1"])
```

## Test Coverage

### Fully covered (same code runs in tests and production)

| Code | Lines |
|---|---|
| `GpuixView::render()`, `sync_focus_handles()` | ~70 |
| `build_element()`, `build_div()`, `build_text()` | ~230 |
| All 12 event handler closures (click, mouseDown×3, mouseUp×3, mouseMove, hover, mouseDownOutside, scroll, keyDown, keyUp, focus, blur) | ~180 |
| `apply_styles()` — 30+ CSS properties | ~130 |
| `emit_event_full()`, `point_to_xy()`, `mouse_button_to_u32()` | ~30 |
| `RetainedTree` — all mutation methods | all |
| `EventPayload`, `EventModifiers`, `From<gpui::Modifiers>` | all |
| `StyleDesc` deserialization, `parse_color_hex()`, `DimensionValue` | all |

### NOT covered (replaced by gpui TestPlatform)

| Code | Why |
|---|---|
| `NodePlatform` (~578 lines) | Test uses gpui's TestPlatform instead |
| `NodeWindow` (~383 lines) | Test uses gpui's TestWindow (no-op draw) |
| `NodeDisplay`, `NodeDispatcher` (~130 lines) | Test uses gpui's TestDisplay/TestDispatcher |
| `GpuixRenderer.init()`, `tick()` (~170 lines) | Platform setup, not rendering logic |

### Summary

The rendering pipeline, event wiring, focus management, and style mapping are **fully covered**. The uncovered code is platform plumbing (winit, wgpu, OS window management) — inherently integration-test territory.

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| `test-support` pulls unexpected deps at pinned commit | Test `cargo build --features test-support` early; wayland/x11 are confirmed no-ops on macOS |
| `TestAppContext` thread_local conflicts with `GpuixRenderer` | Use separate thread_local keys; tests shouldn't mix both renderers |
| `draw()` + `run_until_parked()` don't trigger `GpuixView::render()` | May need explicit `cx.notify()` on the entity after mutations |
| `simulateClick(x, y)` needs real pixel coordinates for hit testing | Tree inspection returns element bounds, or use large elements in tests |
| napi-rs `#[napi]` requires Send, but TestAppContext is !Send | thread_local storage (same pattern as NodePlatform) |

## Phase 2 (Future)

- Query GPUI computed layout (element bounds, positions) for layout assertions
- Screenshot testing with `VisualTestAppContext` (macOS only, real Metal rendering)
- Focus management testing (tab order, focus/blur events through GPUI pipeline)
- Pixel-level visual regression tests

## Zed Source References

All paths relative to `https://github.com/zed-industries/zed` (pinned commit `14f37ed5024b...`).
Local checkout at `opensrc/repos/github.com/zed-industries/zed/`.

### Platform traits (the interfaces we implement)

| Trait | File | Line |
|---|---|---|
| `trait Platform` | `crates/gpui/src/platform.rs` | 113 |
| `trait PlatformWindow` | `crates/gpui/src/platform.rs` | 449 |
| `trait PlatformDisplay` | `crates/gpui/src/platform.rs` | 232 |
| `trait PlatformDispatcher` | `crates/gpui/src/platform.rs` | 569 |
| `trait PlatformTextSystem` | `crates/gpui/src/platform.rs` | 593 |
| `trait PlatformAtlas` | `crates/gpui/src/platform.rs` | 831 |
| `Application::with_platform()` | `crates/gpui/src/app.rs` | 137 |

### Test platform (what we use for tests)

Module entry: `crates/gpui/src/platform/test.rs` — gated behind `#[cfg(any(test, feature = "test-support"))]`

| Type | File | Line | Notes |
|---|---|---|---|
| `TestPlatform` | `crates/gpui/src/platform/test/platform.rs` | 20 | `pub(crate)`, `run()` is `unimplemented!()` — tests drive manually |
| `TestWindow` | `crates/gpui/src/platform/test/window.rs` | 36 | `draw()` is a no-op, `scale_factor()` returns 2.0 |
| `TestWindowState` | `crates/gpui/src/platform/test/window.rs` | 15 | Has `simulate_input()`, `simulate_resize()` methods |
| `TestDisplay` | `crates/gpui/src/platform/test/display.rs` | 5 | Fixed 1920×1080 bounds |
| `TestDispatcher` | `crates/gpui/src/platform/test/dispatcher.rs` | 17 | Deterministic scheduler via `TestScheduler`, `run_until_parked()` |
| `TestAtlas` | `crates/gpui/src/platform/test/window.rs` | 313 | In-memory sprite atlas stub |

### Test contexts (the API we call)

| Type | File | Line | Notes |
|---|---|---|---|
| `TestAppContext` | `crates/gpui/src/app/test_context.rs` | 20 | **pub**, `single()` at L147, `build()` at L117, `add_window_view()` at L257 |
| `VisualTestContext` | `crates/gpui/src/app/test_context.rs` | 667 | **pub**, `simulate_click()` at L769, `simulate_keystrokes()` at L713, `run_until_parked()` at L694 |
| `VisualTestAppContext` | `crates/gpui/src/app/visual_test_context.rs` | 21 | macOS-only, real Metal rendering, `open_offscreen_window()` at L97 |
| `VisualTestPlatform` | `crates/gpui/src/platform/visual_test.rs` | 31 | Wraps real platform with TestDispatcher-backed executors |

### Web/WASM platform (reference for our NodePlatform design)

Separate crate: `crates/gpui_web/`

| File | Key type | Notes |
|---|---|---|
| `crates/gpui_web/src/platform.rs` | `WebPlatform` (L33) | `run()` does not block — uses `wasm_bindgen_futures::spawn_local`. Closest analog to our `NodePlatform::run()` |
| `crates/gpui_web/src/window.rs` | `WebWindow` (L59) | wgpu surface on `<canvas>`, rAF-driven frame loop |
| `crates/gpui_web/src/display.rs` | `WebDisplay` (L5) | Queries `screen.width/height` from `web_sys::Window` |
| `crates/gpui_web/src/dispatcher.rs` | `WebDispatcher` | Uses `PriorityQueueSender/Receiver` + `Atomics.notify` for wasm threading |
| `crates/gpui_web/src/events.rs` | — | DOM event → GPUI `PlatformInput` conversion |
| `crates/gpui_web/src/keyboard.rs` | — | Key code mapping |

### macOS platform (reference for native window/rendering)

Separate crate: `crates/gpui_macos/`

```
crates/gpui_macos/src/
  platform.rs       ← MacPlatform (uses NSApplication run loop)
  window.rs         ← MacWindow (Metal-backed)
  display.rs        ← MacDisplay (CoreGraphics)
  dispatcher.rs     ← MacDispatcher (dispatch queues)
  metal_renderer.rs ← MetalRenderer
  metal_atlas.rs    ← MetalAtlas
  text_system.rs    ← CoreText integration
  display_link.rs   ← CVDisplayLink for vsync
  events.rs, keyboard.rs, pasteboard.rs, ...
```

### Cross-platform wgpu renderer (used by our NodeWindow)

Separate crate: `crates/gpui_wgpu/` — pure renderer, no Platform impl.

| File | Key type | Notes |
|---|---|---|
| `crates/gpui_wgpu/src/wgpu_context.rs` | `WgpuContext` (L7) | `new()` for native, `new_web()` for WASM |
| `crates/gpui_wgpu/src/wgpu_renderer.rs` | `WgpuRenderer` (L94) | Takes `Scene`, renders to wgpu surface |
| `crates/gpui_wgpu/src/wgpu_atlas.rs` | `WgpuAtlas` (L22) | `PlatformAtlas` impl for wgpu |
| `crates/gpui_wgpu/src/cosmic_text_system.rs` | `CosmicTextSystem` (L22) | `PlatformTextSystem` impl, shared between web and our NodePlatform |

### Platform dispatch entry point

| File | Notes |
|---|---|
| `crates/gpui_platform/src/gpui_platform.rs` L30 | `current_platform()` — dispatches to Mac/Linux/Web/Windows based on `#[cfg]` |
