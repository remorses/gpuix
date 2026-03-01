# Plan: GPU-backed test renderer with screenshot support

## Goal

Replace the current `TestGpuixRenderer` (which uses `TestPlatform` — no GPU, no rendering) with a single GPU-backed test renderer that uses real rendering. This enables `captureScreenshot(path)` to save the rendered UI as a PNG image during tests.

## Oracle review findings

The original plan proposed building a custom headless wgpu renderer. Oracle review identified critical blockers with that approach:

1. **`WgpuRenderer` is surface-backed** — `new()` requires window/display handles, `draw()` pulls `surface.get_current_texture()`. It's not an offscreen renderer. Making it headless would require forking/extending `gpui_wgpu`.
2. **`PlatformWindow` requires `HasWindowHandle + HasDisplayHandle`** — a truly windowless test window isn't a drop-in.
3. **`TestAppContext::single()` is hardwired to `TestPlatform`** — no public constructor accepts a custom platform.
4. **wgpu `bytes_per_row` alignment** — must be 256-byte aligned for `copy_texture_to_buffer`, `width * 4` is often invalid.

### Recommended approach: Use GPUI's `VisualTestAppContext`

GPUI already has `VisualTestAppContext` which wraps a real macOS platform (`MacPlatform` + Metal/Blade) with `TestDispatcher` for deterministic scheduling. It provides `capture_screenshot()` out of the box.

This means:
- macOS: full screenshot support via Metal (works now, no wgpu changes needed)
- Linux/CI: graceful "not supported" error until we invest in headless wgpu

## Current state

```
TestGpuixRenderer (Rust)
  └── TestAppContext::single()
        └── TestPlatform
              ├── draw() → no-op (does nothing)
              ├── render_to_image() → "not implemented"
              └── text_system → empty glyphs (no text rendering)
```

The test platform doesn't render anything. It runs GPUI's layout and hit-testing (which is why event simulation works), but it never produces pixels.

## Target state

```
TestGpuixRenderer (Rust)
  └── VisualTestAppContext (GPUI's visual test infrastructure)
        ├── VisualTestPlatform wrapping real MacPlatform
        ├── Metal/Blade rendering (real GPU, offscreen window at -10000,-10000)
        ├── Real text system (native macOS text rendering)
        ├── TestDispatcher for deterministic scheduling
        ├── draw(scene) → renders via Metal
        ├── capture_screenshot() → render_to_image() → RgbaImage
        └── captureScreenshot(path) → flush + capture + save PNG
```

One test renderer, always GPU-backed on macOS. Every test gets real rendering.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  JS Test                                                     │
│                                                              │
│  testRoot.render(<Counter />)                                │
│  testRoot.renderer.nativeSimulateClick(10, 10)               │
│  testRoot.renderer.captureScreenshot("/tmp/test.png")        │
└──────────────────────┬───────────────────────────────────────┘
                       │ napi
┌──────────────────────▼───────────────────────────────────────┐
│  TestGpuixRenderer (Rust)                                    │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  GpuixView (same as production)                      │    │
│  │  build_element() → GPUI elements                     │    │
│  │  apply_styles() → GPUI styles                        │    │
│  │  emit_event_full() → event callbacks                 │    │
│  └─────────────────────────────────────────────────────┘    │
│                       │                                      │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  VisualTestAppContext (GPUI)                         │    │
│  │  VisualTestPlatform wrapping MacPlatform             │    │
│  │  TestDispatcher for deterministic scheduling         │    │
│  │  Layout + hit testing + Scene + Metal rendering      │    │
│  └─────────────────────────────────────────────────────┘    │
│                       │                                      │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Metal/Blade renderer (macOS)                        │    │
│  │  Offscreen window at (-10000, -10000)                │    │
│  │  capture_screenshot() → render_to_image() → PNG      │    │
│  └─────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────┘
```

## Key challenge: TestPlatform vs real platform

GPUI's `TestPlatform` is designed for fast, headless tests. It mocks everything:
- `draw()` is a no-op
- text system returns empty metrics
- no GPU context

We need a platform that:
1. Has real GPU rendering (Metal on macOS)
2. Has a real text system for text rendering
3. Works without a visible window (offscreen at -10000,-10000)
4. Supports `simulate_click`, `simulate_keystrokes`, etc.
5. Has deterministic task scheduling (TestDispatcher)

### Approach: Use GPUI's VisualTestAppContext

GPUI already provides `VisualTestAppContext` which does exactly this:

- Wraps real `MacPlatform` with `VisualTestPlatform` (adds `TestDispatcher`)
- Opens windows offscreen at (-10000, -10000) — invisible but fully rendered
- Provides `capture_screenshot(window)` → `render_to_image()` → `RgbaImage`
- Has `simulate_click`, `simulate_keystrokes`, `simulate_mouse_move`, etc.
- Uses `TestDispatcher` for deterministic scheduling and `run_until_parked()`

## Implementation steps

### Step 1: Update `TestGpuixRenderer` to use `VisualTestAppContext`

**File:** `packages/native/src/test_renderer.rs`

Replace the `TestAppContext` setup with `VisualTestAppContext`. The key change is in `TestGpuixRenderer::new()`:

```rust
// Before:
let mut cx = gpui::TestAppContext::single();
let (view, vcx) = cx.add_window_view(|_window, _cx| GpuixView { ... });

// After:
// Get the real macOS platform
let mac_platform = gpui::MacPlatform::new();
let mut cx = gpui::VisualTestAppContext::new(Rc::new(mac_platform));

// Open an offscreen window with GpuixView as the root
let window = cx.open_offscreen_window_default(|window, app| {
    app.new(|cx| GpuixView { ... })
})?;

let view = window.root(&cx)?;
```

The `VisualTestAppContext` opens the window at (-10000, -10000) — invisible but fully rendered by Metal. It uses `TestDispatcher` so `run_until_parked()` works identically.

**Important difference:** `VisualTestAppContext` doesn't return a `&mut VisualTestContext` like `TestAppContext::add_window_view` does. Instead, event simulation methods are on `VisualTestAppContext` itself and take a `window: AnyWindowHandle` parameter. We need to store the `AnyWindowHandle` alongside the context.

Thread-local storage changes:

```rust
thread_local! {
    // Before: VisualTestContext pointer
    // After: VisualTestAppContext (owns the app) + AnyWindowHandle
    static TEST_CX: RefCell<Option<gpui::VisualTestAppContext>> = RefCell::new(None);
    static TEST_WINDOW: RefCell<Option<gpui::AnyWindowHandle>> = RefCell::new(None);
    static TEST_VIEW: RefCell<Option<gpui::Entity<GpuixView>>> = RefCell::new(None);
}
```

Event simulation methods change from `vcx.simulate_click(pos, mods)` to `cx.simulate_click(window, pos, mods)`.

### Step 2: Update all simulation methods for new API

The `VisualTestAppContext` simulation methods take `window: AnyWindowHandle` as first parameter (unlike `VisualTestContext` which has implicit window). Update all methods:

```rust
// Before (VisualTestContext):
vcx.simulate_click(position, modifiers);
vcx.simulate_keystrokes(keystrokes);
vcx.simulate_mouse_move(position, button, modifiers);
vcx.run_until_parked();

// After (VisualTestAppContext):
cx.simulate_click(window, position, modifiers);
cx.simulate_keystrokes(window, keystrokes);
cx.simulate_mouse_move(window, position, button, modifiers);
cx.run_until_parked();
```

The `flush()` method changes from `vcx.update(...)` to `cx.update_window(window, ...)`.

### Step 3: Add `capture_screenshot` napi method

**File:** `packages/native/src/test_renderer.rs`

```rust
/// Capture a screenshot of the current rendered state and save as PNG.
/// macOS only — requires Metal GPU rendering.
#[napi]
pub fn capture_screenshot(&self, path: String) -> Result<()> {
    let cx = get_cx()?;       // VisualTestAppContext from thread_local
    let window = get_window()?; // AnyWindowHandle from thread_local
    let view = get_view()?;

    // Flush to ensure layout and rendering are current
    cx.update_window(window, |_, _window, cx| {
        view.update(cx, |_, cx| { cx.notify(); });
    }).map_err(|e| Error::from_reason(e.to_string()))?;
    cx.run_until_parked();

    // Capture via GPUI's render_to_image (Metal texture → RgbaImage)
    let image = cx.capture_screenshot(window)
        .map_err(|e| Error::from_reason(format!("Screenshot failed: {}", e)))?;

    // Save as PNG
    image.save(&path)
        .map_err(|e| Error::from_reason(format!("Failed to save: {}", e)))?;

    Ok(())
}
```

### Step 4: Add JS wrapper to `TestRenderer`

**File:** `packages/react/src/testing.ts`

```typescript
/** Capture a screenshot of the current rendered UI and save as PNG.
 *  macOS only — requires Metal GPU rendering. */
captureScreenshot(path: string): void {
  if (!this.native) {
    throw new Error("Native renderer not available for captureScreenshot")
  }
  this.native.flush()
  this.native.captureScreenshot(path)
}
```

TypeScript declarations in `index.d.ts` are auto-generated by `bun run build`.

### Step 5: Add screenshot test

**File:** `packages/react/src/__tests__/events.test.tsx`

```typescript
import fs from "fs"

it("should capture screenshot after interaction", () => {
  function Counter() {
    const [count, setCount] = useState(0)
    return (
      <div
        style={{
          width: 200,
          height: 50,
          backgroundColor: "#1e1e2e",
        }}
        onClick={() => setCount((c) => c + 1)}
      >
        <text style={{ color: "#cdd6f4", fontSize: 14 }}>
          {`Count: ${count}`}
        </text>
      </div>
    )
  }

  testRoot.render(<Counter />)

  // Capture initial state
  testRoot.renderer.captureScreenshot("/tmp/gpuix-counter-0.png")

  // Click and capture again
  testRoot.renderer.nativeSimulateClick(10, 10)
  testRoot.renderer.captureScreenshot("/tmp/gpuix-counter-1.png")

  // Verify files exist and have non-zero size
  expect(fs.existsSync("/tmp/gpuix-counter-0.png")).toBe(true)
  expect(fs.existsSync("/tmp/gpuix-counter-1.png")).toBe(true)
  expect(fs.statSync("/tmp/gpuix-counter-0.png").size).toBeGreaterThan(0)
  expect(fs.statSync("/tmp/gpuix-counter-1.png").size).toBeGreaterThan(0)
})
```

## Risks and open questions

### 1. MacPlatform availability

`VisualTestAppContext` requires `MacPlatform` which is only available on macOS. On Linux/Windows:
- Option A: graceful error — `captureScreenshot` returns "not supported on this platform"
- Option B: fall back to `TestPlatform` (no screenshots, but event tests still work)
- Option B is better — event simulation should work everywhere, screenshots are macOS-only

This means we may want to keep both context types and choose at runtime, OR build a single context that conditionally supports screenshots.

### 2. MacPlatform initialization in Node.js

`MacPlatform::new()` initializes Cocoa/AppKit internals. This normally happens on the main thread. In Node.js, the napi calls happen on the main thread (via libuv), which should be fine. But there may be conflicts with Node.js's own event loop or Objective-C runtime initialization. This needs testing.

### 3. Test speed impact

Metal/MacPlatform initialization adds overhead (~200-500ms per test suite). This is a one-time cost. Current tests run 18 tests in ~90ms, so total might go to ~300-600ms. Still fast.

### 4. CI environments

- **macOS CI** (GitHub Actions): has Metal support, should work
- **Linux CI**: no MacPlatform, needs fallback to TestPlatform
- Could use `#[cfg(target_os = "macos")]` to gate the visual path

### 5. Golden image nondeterminism

Screenshots may differ across:
- GPU hardware (Intel vs Apple Silicon vs discrete)
- macOS versions (text rendering changes)
- Font availability (system fonts vary)
- Anti-aliasing settings

For now, we only verify screenshots are non-empty PNGs (no pixel-perfect comparison). Visual regression testing with golden images is a future concern.

### 6. Main-thread constraints on macOS

macOS requires UI operations on the main thread. Since napi calls from Node.js already happen on the main thread, this should be fine. But if tests ever run in parallel threads, this breaks.

### 7. VisualTestAppContext API differences

`VisualTestAppContext` simulation methods take `window: AnyWindowHandle` explicitly, unlike `VisualTestContext` (from `TestAppContext::add_window_view`) which has the window implicit. All simulation methods in `test_renderer.rs` need updating to pass the stored window handle.

## Files to modify

| File | Action |
|------|--------|
| `packages/native/src/test_renderer.rs` | **Modify** — switch to VisualTestAppContext, add captureScreenshot |
| `packages/native/Cargo.toml` | **Modify** — add `image` crate dependency if not already present |
| `packages/react/src/testing.ts` | **Modify** — add captureScreenshot wrapper |
| `packages/react/src/__tests__/events.test.tsx` | **Modify** — add screenshot test |

No new files needed — we reuse GPUI's existing `VisualTestAppContext` and `VisualTestPlatform`.

## Verification

- `cargo check` — compiles clean
- `bun run build` — napi binary builds
- `bun test` — all existing 18 tests still pass (event simulation, tree structure)
- New screenshot test produces valid PNG files
- PNG files show rendered UI with text, colors, and layout
