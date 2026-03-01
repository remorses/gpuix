---
title: NodePlatform Improvements Plan — Aligning with Zed's Platform Crates
description: Detailed plan for improving GPUIX's NodePlatform to match the quality and completeness of Zed's gpui_macos, gpui_linux, and gpui_web platform implementations.
---

# NodePlatform Improvements Plan

## Zed Source Reference Files

All Zed source files are downloaded in `opensrc/repos/github.com/zed-industries/zed/`.
Paths below are relative to that root.

### gpui_web (browser platform — closest analog to our non-blocking approach)

| File | Lines | Purpose |
|---|---|---|
| `crates/gpui_web/src/platform.rs` | 341 | WebPlatform — non-blocking run(), cursor style, window appearance |
| `crates/gpui_web/src/window.rs` | 689 | WebWindow — canvas, RAF loop, ResizeObserver, DPR detection |
| `crates/gpui_web/src/dispatcher.rs` | 333 | WebDispatcher — Web Workers, setTimeout, queueMicrotask, Atomics.waitAsync |
| `crates/gpui_web/src/display.rs` | 98 | WebDisplay — reads window.screen() dimensions |
| `crates/gpui_web/src/events.rs` | 615 | Input events — Pointer Events API, keyboard, scroll, drag-drop |
| `crates/gpui_web/src/keyboard.rs` | 19 | Keyboard layout stub |

### gpui_macos (macOS native — the gold standard)

| File | Lines | Purpose |
|---|---|---|
| `crates/gpui_macos/src/platform.rs` | 1431 | MacPlatform — NSApp run loop, cursor style, credentials, thermal |
| `crates/gpui_macos/src/window.rs` | 2751 | MacWindow — NSWindow, ObjC delegates, IME, focus, CVDisplayLink |
| `crates/gpui_macos/src/dispatcher.rs` | 243 | MacDispatcher — GCD queues, priority routing, realtime threads |
| `crates/gpui_macos/src/display.rs` | 167 | MacDisplay — CGDisplay, NSScreen, multiple monitors |
| `crates/gpui_macos/src/events.rs` | 553 | NSEvent → PlatformInput, international keyboard, dead keys |
| `crates/gpui_macos/src/pasteboard.rs` | 342 | NSPasteboard — clipboard with metadata, image support |
| `crates/gpui_macos/src/display_link.rs` | ~80 | CVDisplayLink — vsync-driven frame callbacks |
| `crates/gpui_macos/src/text_system.rs` | ~600 | CoreText + font-kit text rendering |
| `crates/gpui_macos/src/window_appearance.rs` | ~50 | NSAppearance name → WindowAppearance mapping |

### gpui_linux (Wayland — wgpu-based like us)

| File | Lines | Purpose |
|---|---|---|
| `crates/gpui_linux/src/linux/platform.rs` | 1063 | LinuxPlatform — calloop event loop, LinuxClient trait |
| `crates/gpui_linux/src/linux/dispatcher.rs` | 445 | LinuxDispatcher — calloop channels, timer thread, SCHED_FIFO |
| `crates/gpui_linux/src/linux/wayland/client.rs` | 2278 | WaylandClient — all Wayland event dispatch, globals |
| `crates/gpui_linux/src/linux/wayland/window.rs` | 1550 | WaylandWindow — wl_surface, frame pacing, wgpu surface |

### gpui_wgpu (shared renderer — used by web, linux, and us)

| File | Lines | Purpose |
|---|---|---|
| `crates/gpui_wgpu/src/wgpu_context.rs` | 270 | WgpuContext — adapter selection, device creation, surface config |
| `crates/gpui_wgpu/src/wgpu_renderer.rs` | ~800 | WgpuRenderer — draw(), pipeline states, instance buffers |
| `crates/gpui_wgpu/src/cosmic_text_system.rs` | ~500 | CosmicTextSystem — font loading, shaping, rasterization |

### Our current files (packages/native/src/)

| File | Lines | Purpose |
|---|---|---|
| `platform/node_platform.rs` | 829 | NodePlatform — non-blocking run(), tick(), winit event translation |
| `platform/node_window.rs` | 427 | NodeWindow — winit window, WgpuRenderer, shared state |
| `platform/node_dispatcher.rs` | 178 | NodeDispatcher — 4 bg threads, main queue, delayed heap |
| `platform/node_display.rs` | 61 | NodeDisplay — hardcoded 1920x1080 |
| `renderer.rs` | 854 | GpuixRenderer napi, GpuixView, build_element, apply_styles |

---

## Changes by Priority

---

### 1. Frame Pacing (HIGH)

**Problem:** Our `tick()` is called every `setImmediate` (~1ms). Every call pumps winit events AND fires `request_frame(force_render)`. When `needs_redraw` is false, we still call `request_frame(force_render: false)` which asks GPUI to render a frame — wasteful.

**How Zed does it:**

- **macOS** (`display_link.rs`): CVDisplayLink fires at display refresh rate → DispatchSource coalesces → main queue fires `request_frame_callback` only at vsync intervals.
- **Linux** (`wayland/window.rs:558`): `wl_surface.frame` callback fires when compositor is ready. `window.frame()` calls `request_frame(Default::default())` — only at vblank.
- **Web** (`window.rs:285`): `requestAnimationFrame` fires at ~60fps. The RAF closure calls `request_frame(RequestFrameOptions { require_presentation: true, force_render: false })`.

**What to change:**

- **File:** `node_platform.rs` — `tick()` method (line 94)
- Throttle frame callback cadence rather than suppressing callbacks entirely. GPUI does useful work in frame callbacks even when not dirty (animations, timers, `next_frame` tasks). Fully suppressing callbacks can starve these internal GPUI paths.
- Approach: from the JS side, use `setTimeout(loop, 16)` instead of `setImmediate` to approximate ~60fps (matching `requestAnimationFrame` behavior). Still always call `request_frame`, but only set `force_render: true` when `commitMutations()` flagged `needs_redraw`.
- Keep `require_presentation: true` so GPUI always presents the frame (even if not dirty, this is how gpui_web does it).

**Warning:** Do NOT fully suppress frame callbacks when idle — GPUI internally uses frame callbacks for `next_frame()` futures, animation interpolation, and timer-driven updates. The Zed web platform always fires RAF regardless of dirty state.

**Reference:** `gpui_web/src/window.rs:285-318` — the RAF loop always passes `force_render: false` and `require_presentation: true`, every frame, unconditionally.

**Estimated effort:** Small — ~10 lines changed in `node_platform.rs`, ~5 lines in JS event loop.

---

### 2. Display Bounds + DPI/Scale-Factor Handling (HIGH)

**Problem:** `NodeDisplay` returns hardcoded 1920x1080. Additionally, `WindowEvent::ScaleFactorChanged` is not handled in `tick()`, so scale and bounds go stale if the window moves between monitors.

**How Zed does it:**

- **macOS** (`display.rs:106`): `CGDisplayBounds()` for full size, `NSScreen.visibleFrame` for usable area (excludes Dock/menu bar).
- **Linux** (`wayland/display.rs`): Reads from `wl_output` geometry/mode events. Stores bounds per output.
- **Web** (`display.rs:55`): `window.screen().width/height()` for full screen, `window.innerWidth/innerHeight()` for viewport.
- **Web** (`window.rs:331-355`): DPR change watching via `matchMedia("(resolution: Xdppx)")`.

**What to change:**

- **File:** `node_display.rs` — replace hardcoded bounds
  - Accept display size as constructor parameter
  - In `open_window()`: read `winit_window.current_monitor()` to get actual monitor size and pass to `NodeDisplay`
  - Unify `NodePlatform.active_display` and per-window `NodeDisplay` — currently they're separate instances that can diverge (`node_platform.rs:55` vs `node_window.rs:179`)

- **File:** `node_platform.rs` — handle `WindowEvent::ScaleFactorChanged` in `tick()`
  - Update `state.scale_factor` when DPI changes
  - Fire the resize callback with new logical size and scale factor
  - Update `renderer.update_drawable_size()` with new physical size

**Reference:** `gpui_web/src/display.rs:55-80` — simplest display model. `gpui_web/src/window.rs:331-355` — DPR change detection.

**Estimated effort:** Medium — ~50 lines across display and platform.

---

### 3. Cursor Style (HIGH)

**Problem:** `set_cursor_style()` is a no-op. Users can't change cursor on hover (pointer for buttons, text for inputs, etc.).

**How Zed does it:**

- **macOS** (`platform.rs:972-1021`): Maps every `CursorStyle` to `NSCursor` methods. Special cases for hidden cursor and private diagonal resize cursors.
- **Linux** (`wayland.rs`): Maps `CursorStyle` to `wp_cursor_shape::Shape` wayland protocol values.
- **Web** (`platform.rs:269-302`): Maps `CursorStyle` to CSS cursor strings, sets via `body.style.cursor`.

**What to change:**

- **File:** `node_platform.rs` — `set_cursor_style()` method (line 678)
- Store a reference to the winit window in `NodeWindowState` (or in platform via the shared state Rc)
- Map `gpui::CursorStyle` variants to `winit::window::CursorIcon` enum
- Call `winit_window.set_cursor(CursorIcon::...)` or `winit_window.set_cursor_visible(false)` for hidden
- When leaving `CursorStyle::None`, call `set_cursor_visible(true)` to restore visibility

**Reference:** `gpui_web/src/platform.rs:269-302` — cleanest mapping since it's a flat enum-to-string conversion. Our winit version will be similar but enum-to-enum.

**Mapping (from gpui_macos/platform.rs + winit docs):**

```
CursorStyle::Arrow          → CursorIcon::Default
CursorStyle::IBeam          → CursorIcon::Text
CursorStyle::Crosshair      → CursorIcon::Crosshair
CursorStyle::ClosedHand     → CursorIcon::Grabbing
CursorStyle::OpenHand       → CursorIcon::Grab
CursorStyle::PointingHand   → CursorIcon::Pointer
CursorStyle::ResizeLeft      → CursorIcon::WResize
CursorStyle::ResizeRight     → CursorIcon::EResize
CursorStyle::ResizeUp        → CursorIcon::NResize
CursorStyle::ResizeDown      → CursorIcon::SResize
CursorStyle::ResizeLeftRight → CursorIcon::EwResize
CursorStyle::ResizeUpDown    → CursorIcon::NsResize
CursorStyle::None            → set_cursor_visible(false)
// etc.
```

**Estimated effort:** Small — ~40 lines. Need to store winit window reference in platform.

---

### 4. Window Controls + is_maximized (HIGH)

**Problem:** `minimize()`, `zoom()`, `toggle_fullscreen()` are no-ops or don't propagate to the OS window. `is_maximized()` is hardcoded `false` (`node_window.rs:236`).

**How Zed does it:**

- **macOS** (`window.rs`): Direct `NSWindow` calls — `miniaturize:`, `zoom:`, `toggleFullScreen:`.
- **Linux** (`wayland/window.rs`): XDG toplevel requests — `set_minimized()`, `set_maximized()`, `set_fullscreen()`.
- **Web** (`window.rs`): `canvas.request_fullscreen()` / `document.exit_fullscreen()`.

**What to change:**

- **File:** `node_window.rs`
  - Store winit window reference in `NodeWindowState` (currently only in `NodeWindow` which doesn't share with the state Rc)
  - `minimize()` (line 329): call `winit_window.set_minimized(true)`
  - `zoom()` (line 331): call `winit_window.set_maximized(!winit_window.is_maximized())`
  - `toggle_fullscreen()` (line 333): call `winit_window.set_fullscreen(Some(Fullscreen::Borderless(None)))` or `set_fullscreen(None)`
  - `is_maximized()` (line 236): call `winit_window.is_maximized()` instead of returning `false`

**Reference:** `gpui_web/src/window.rs:580-605` — toggle_fullscreen via requestFullscreen.

**Estimated effort:** Small — ~25 lines. Biggest change is making winit_window accessible from NodeWindowState.

---

### 5. Clipboard (MEDIUM)

**Problem:** `read_from_clipboard()` returns None, `write_to_clipboard()` is a no-op. Text copy/paste doesn't work.

**How Zed does it:**

- **macOS** (`pasteboard.rs`): NSPasteboard with custom metadata hash, image support for PNG/JPEG/TIFF/WebP/GIF.
- **Linux** (`wayland/clipboard.rs`): Wayland data-device protocol with MIME types.
- **Web**: Not implemented (async clipboard API not wired yet).

**What to change:**

- **File:** `node_platform.rs` — `read_from_clipboard()` (line 692), `write_to_clipboard()` (line 696)
- Add `arboard` crate as dependency (cross-platform clipboard, works on macOS/Linux/Windows)
- Store `arboard::Clipboard` instance in `NodePlatform`
- Read: `clipboard.get_text()` → wrap in `ClipboardItem::new_string()`
- Write: extract string from `ClipboardItem`, call `clipboard.set_text()`
- Image clipboard is lower priority but `arboard` supports it too
- **Scope:** Text-only MVP. GPUI's `ClipboardItem` supports richer payloads (metadata hash, images) — those are deferred.
- **Also:** `read_from_find_pasteboard` / `write_to_find_pasteboard` (lines 698, 702) remain no-ops for now — the find pasteboard is macOS-specific.

**Reference:** `gpui_macos/src/pasteboard.rs` for the full metadata-aware implementation. For MVP, just text is sufficient.

**Estimated effort:** Medium — ~50 lines plus `arboard` dependency in Cargo.toml.

---

### 6. Window Appearance Detection (MEDIUM)

**Problem:** `window_appearance()` always returns `WindowAppearance::Dark` at both platform level (`node_platform.rs:599`) AND window level (`node_window.rs:268`). The `appearance_changed` callback is never fired.

**How Zed does it:**

- **macOS** (`platform.rs:599`): Reads `NSAppearance.currentDrawingAppearance.name` and matches against known names (`NSAppearanceNameDarkAqua`, etc.).
- **macOS** (`window_appearance.rs`): Maps NSAppearance name strings to `WindowAppearance` enum.
- **Linux** (`xdg_desktop_portal.rs`): Reads appearance via XDG Desktop Portal D-Bus interface.
- **Web** (`platform.rs:161-173`): Uses `window.matchMedia("(prefers-color-scheme: dark)")`.

**What to change:**

- **File:** `node_platform.rs` — `window_appearance()` (line 599)
  - On macOS: use `objc2_app_kit::NSApplication::effectiveAppearance()` and match name — we already import objc2_app_kit.
- **File:** `node_window.rs` — `appearance()` (line 268)
  - Should return actual appearance, not hardcoded `Dark`.
- **File:** `node_platform.rs` or `node_window.rs` — wire up appearance change callback
  - Listen for `NSWorkspace.didChangeScreenParametersNotification` or poll during tick to detect changes.
  - Call `state.callbacks.appearance_changed` when it changes.

**Reference:** `gpui_macos/src/window_appearance.rs` — maps NSAppearance names to `WindowAppearance`.

**Estimated effort:** Medium — ~35 lines for macOS. Linux detection is harder (needs D-Bus).

---

### 7. on_moved Callback + Window Moved Events (MEDIUM)

**Problem:** The `on_moved` callback slot exists in `NodeWindowCallbacks` (`node_window.rs:71`) and is registered (`node_window.rs:361`) but `tick()` never detects or emits window-moved events. winit sends `WindowEvent::Moved` which we currently ignore.

**How Zed does it:**

- **macOS** (`window.rs`): `windowDidMove:` ObjC delegate method fires the `moved` callback.
- **Linux** (`wayland/window.rs`): configure events include new position.

**What to change:**

- **File:** `node_platform.rs` — add `WindowEvent::Moved` to the match in `tick()` (around line 385)
- Fire `state.callbacks.moved` when the window position changes

**Estimated effort:** Small — ~10 lines.

---

### 8. set_window_title Propagation (MEDIUM)

**Problem:** `GpuixRenderer::set_window_title()` is a no-op (line 268). `NodeWindow::set_title()` works but only if called directly.

**What to change:**

- **File:** `renderer.rs` — `set_window_title()` (line 268)
- Store the winit window reference or pass title through to `GpuixView.window_title`, which is already used in `window.set_window_title()` during render.
- Or: store title in `NodeWindowState` and have `tick()` apply it.

**Estimated effort:** Small — ~10 lines.

---

### 9. Dispatcher Priority Support (MEDIUM)

**Problem:** `dispatch()` ignores the `Priority` parameter — all background work goes to the same channel. Zed routes different priorities to different queues.

**How Zed does it:**

- **macOS** (`dispatcher.rs:71-88`): Maps `Priority::High/Medium/Low` to `DispatchQueueGlobalPriority::High/Default/Low`. Panics on `RealtimeAudio` (must use `spawn_realtime()`).
- **Linux** (`dispatcher.rs`): Uses `PriorityQueueSender/Receiver` — items are dequeued in priority order.
- **Web** (`dispatcher.rs:224-239`): Routes to Web Workers when threads available, falls back to main thread in single-threaded mode. Uses `queueMicrotask` for `RealtimeAudio`.

**What to change:**

- **File:** `node_dispatcher.rs`
- Replace single `mpsc::channel` with a priority-aware channel. Options:
  - Use `gpui::PriorityQueueSender/PriorityQueueReceiver` if available in our GPUI version (note: these are cfg-gated to `windows/linux/wasm` in current GPUI — NOT exported on macOS. Need to check our pinned commit.)
  - Fallback: use separate channels per priority level (High gets its own, Medium+Low share one)
- `dispatch()` should route based on `Priority` parameter
- Workers should drain high-priority items before low-priority ones

**Reference:** `gpui_linux/src/linux/dispatcher.rs` — uses GPUI's built-in `PriorityQueueSender/Receiver` for background work.

**Estimated effort:** Medium — ~60 lines. Depends on whether GPUI's priority queue types are exported for our target.

---

### 10. spawn_realtime (MEDIUM)

**Problem:** `spawn_realtime()` calls the function immediately on the current thread instead of spawning a real-time priority thread.

**How Zed does it:**

- **macOS** (`dispatcher.rs:107`): Spawns a raw `std::thread`, calls `set_audio_thread_priority()` which uses Mach kernel `thread_policy_set` with `THREAD_TIME_CONSTRAINT_POLICY` (75% guaranteed, 85% max duty cycle at audio frame rate).
- **Linux** (`dispatcher.rs:201`): Spawns a thread, sets `SCHED_FIFO` priority 65 via `pthread_setschedparam`.
- **Web** (`dispatcher.rs:270`): On main thread: `queue_microtask()`. On worker: posts `MainThreadItem::RealtimeFunction`.

**What to change:**

- **File:** `node_dispatcher.rs` — `spawn_realtime()` (line 170)
- Spawn a new thread for the function
- On macOS: set Mach thread time-constraint policy (copy from `gpui_macos/src/dispatcher.rs:115-160`)
- On Linux: set `SCHED_FIFO` priority via `pthread_setschedparam`
- Fallback (if priority setting fails): just run on a regular thread

**Reference:** `gpui_macos/src/dispatcher.rs:107-160` — full Mach thread priority implementation.

**Estimated effort:** Medium — ~80 lines with platform-specific cfg blocks.

---

### 11. Callback Reentrancy Safety (MEDIUM)

**Problem:** Some callbacks in `tick()` are invoked while `RefCell` borrows may still be active. For example, the `request_frame` callback path at `node_platform.rs:390` calls through while `state.callbacks` borrow could conflict with a reentrant path. This can cause runtime panics.

**How Zed does it:**

- **macOS** (`window.rs`): Consistent **take-and-restore** pattern for every callback invocation:
  ```rust
  let mut lock = state.lock();
  if let Some(mut callback) = lock.some_callback.take() {
      drop(lock);
      callback(...);
      state.lock().some_callback.get_or_insert(callback);
  }
  ```
  This avoids holding the lock/borrow during the callback, preventing deadlocks and borrow panics.

**What to change:**

- **File:** `node_platform.rs` — all callback invocations in `tick()`
- Audit every `state.callbacks.borrow_mut()` → callback invocation path
- Replace with take-call-restore pattern: take the callback out of the RefCell, drop the borrow, call the callback, then put it back
- This is especially important for `request_frame`, `input`, `resize`, `active_status_change`, and `close` callbacks

**Reference:** `gpui_macos/src/window.rs` — search for `.take()` / `.get_or_insert()` pattern throughout.

**Estimated effort:** Medium — ~40 lines of refactoring across tick() event handlers.

---

### 12. Input Handler / IME Support (LOW — future)

**Problem:** `set_input_handler()` stores the handler but nothing calls it. Text input beyond basic keyboard events doesn't work.

**How Zed does it:**

- **macOS** (`window.rs:2265-2458`): Full `NSTextInputClient` protocol — `insertText:`, `setMarkedText:`, `unmarkText:`, etc. All delegate to `PlatformInputHandler`.
- **Linux** (`wayland/client.rs:1479-1557`): `zwp_text_input_v3` protocol — commit string, preedit string, done events.
- **Web** (`events.rs`): No IME wiring yet.

**What to change:**

- **File:** `node_platform.rs` — keyboard event handling in `tick()` (line 244)
- For basic text input: when a `KeyDown` event produces a printable character, call `input_handler.replace_text_in_range()` with the character.
- For full IME: integrate with winit's `Ime` events (`Ime::Commit`, `Ime::Preedit`, `Ime::Enabled`, `Ime::Disabled`). winit v0.30+ supports IME events.
- Call `winit_window.set_ime_allowed(true)` during window creation.

**Reference:** `gpui_macos/src/window.rs:1712-1864` — the full `handle_key_event()` function with IME integration. Most complex piece of the macOS platform.

**Estimated effort:** Large — IME is the hardest platform integration. Basic text input is medium (~50 lines). Full IME with marked text/composition is ~200 lines.

---

### 13. Multiple Windows (LOW — future)

**Problem:** Platform stores a single `event_loop` and `window_state`. Can't open multiple windows.

**How Zed does it:**

- **macOS**: Each `MacWindow` is independent. All share the same `NSApp` run loop.
- **Linux**: `WaylandClientState.windows: HashMap<ObjectId, WaylandWindowStatePtr>` — maps surface IDs to window state. All events dispatched to the correct window.
- **Web**: Currently single window (browser tab = one window).

**What to change:**

- **File:** `node_platform.rs` — `open_window()` and `tick()`
- Replace `window_state: RefCell<Option<...>>` with `windows: RefCell<HashMap<AnyWindowHandle, Rc<NodeWindowState>>>`
- In `tick()`, dispatch winit events to the correct window based on `window_id`
- Share the single `event_loop` across all windows

**Reference:** `gpui_linux/src/linux/wayland/client.rs` — `windows` HashMap pattern.

**Estimated effort:** Medium — ~100 lines refactor. Mainly changing single-window assumptions to HashMap lookups.

---

### 14. File Drag and Drop (LOW — future)

**Problem:** Not implemented. winit supports drag-and-drop events but they're ignored in `tick()`'s default match branch (`node_platform.rs:385`).

**How Zed does it:**

- **macOS** (`window.rs`): NSWindow drag-and-drop delegate — `draggingEntered:`, `performDragOperation:`.
- **Linux** (`wayland/client.rs:1956-2111`): Reads `file://` URIs from Wayland data-device pipe on background thread.
- **Web** (`events.rs:266-313`): DOM `dragover`/`drop`/`dragleave` events.

GPUI's `FileDropEvent` has four phases: `Entered { paths }`, `Pending { position }`, `Submit { position }`, `Exited`.

**What to change:**

- **File:** `node_platform.rs` — handle these winit events in `tick()`:
  - `WindowEvent::HoveredFile(path)` → `PlatformInput::FileDrop(FileDropEvent::Entered { paths: vec![path] })`
  - Subsequent `HoveredFile` while dragging → `PlatformInput::FileDrop(FileDropEvent::Pending { position })`
  - `WindowEvent::DroppedFile(path)` → `PlatformInput::FileDrop(FileDropEvent::Submit { position })`
  - `WindowEvent::HoveredFileCancelled` → `PlatformInput::FileDrop(FileDropEvent::Exited)`

**Reference:** `gpui_web/src/events.rs:266-313` — simplest implementation with all four phases.

**Estimated effort:** Medium — ~50 lines.

---

### 15. Thermal State (LOW)

**Problem:** Always returns `ThermalState::Nominal`.

**How Zed does it:**

- **macOS** (`platform.rs:890-902`): Reads `NSProcessInfo.thermalState`. Subscribes to `NSProcessInfoThermalStateDidChangeNotification`.

**What to change:**

- **File:** `node_platform.rs` — `thermal_state()` (line 686)
- On macOS: read via `objc2_foundation::NSProcessInfo` — `thermalState()` returns 0-3.

**Estimated effort:** Small — ~15 lines, macOS only.

---

### 16. Window Decoration/Move/Resize APIs (LOW — future)

**Problem:** `request_decorations()`, `show_window_menu()`, `start_window_move()`, `start_window_resize()` are all no-ops (`node_window.rs:403-409`).

**How Zed does it:**

- **Linux** (`wayland/window.rs`): `xdg_toplevel._move()`, `xdg_toplevel.resize()`, decoration negotiation via `zxdg_toplevel_decoration_v1`.
- **macOS**: Handled by NSWindow decorations natively.

**What to change:**

- **File:** `node_window.rs`
- `start_window_move()`: call `winit_window.drag_window()`
- `start_window_resize(edge)`: call `winit_window.drag_resize_window(edge)` (winit v0.30+)
- `request_decorations()`: call `winit_window.set_decorations(bool)` based on `WindowDecorations::Server` vs `Client`

**Estimated effort:** Small — ~20 lines.

---

## Implementation Order

Recommended sequence based on impact and difficulty:

```
Phase 1 — Quick wins (correctness + UX)
  1. Display bounds + DPI (#2)     — 50 lines, fixes window centering + scale changes
  2. Cursor style (#3)             — 40 lines, essential for interactive UIs
  3. Frame pacing (#1)             — 15 lines, reduces CPU/GPU waste
  4. Window controls (#4)          — 25 lines, minimize/maximize/fullscreen + is_maximized

Phase 2 — Feature completeness
  5. Clipboard (#5)                — 50 lines + arboard dep
  6. Window appearance (#6)        — 35 lines, dark/light mode detection + callback
  7. on_moved callback (#7)        — 10 lines
  8. set_window_title (#8)         — 10 lines
  9. Callback reentrancy (#11)     — 40 lines, prevents RefCell panics
  10. Dispatcher priorities (#9)   — 60 lines
  11. spawn_realtime (#10)         — 80 lines

Phase 3 — Advanced
  12. Text input / IME (#12)       — 50-200 lines
  13. Multiple windows (#13)       — 100 lines refactor
  14. Drag and drop (#14)          — 50 lines (all 4 phases)
  15. Window decoration APIs (#16) — 20 lines
  16. Thermal state (#15)          — 15 lines
```

---

## Testing Strategy

Each change should be validated by:

1. **Build check:** `cargo build` in `packages/native/` compiles without errors
2. **Example test:** Run `examples/counter.tsx` and verify the feature works visually
3. **Integration tests:** Platform features (cursor, display, window controls, drag/drop) need integration tests on real `NodePlatform` — `TestGpuixRenderer` is headless GPUI and does NOT exercise winit/platform paths
4. **Screenshot validation:** Use `screencapture` + image-understanding agent for visual verification

For frame pacing specifically, measure CPU usage before/after with `top` or `Activity Monitor` to confirm reduction.

**Note:** `TestGpuixRenderer` (`test_renderer.rs`) tests the GPUI rendering pipeline (GpuixView, build_element, apply_styles, events) but runs headless without NodePlatform or winit. It validates element building and event dispatch but cannot test platform integration features.
