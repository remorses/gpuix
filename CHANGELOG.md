# Changelog

## 2026-03-01 16:35 UTC

- Expand visual screenshot coverage with additional end-to-end tests for `click`, `keyDown`, and `mouseEnter`-driven hover state changes
- Add shared screenshot assertion helper in `events.test.tsx` to enforce non-empty PNG output and before/after image differences

## 2026-03-01 16:20 UTC

- Fix `build_text` to render child text elements recursively instead of dropping nested text nodes
- Improve screenshot reliability by forcing `window.refresh()` before `capture_screenshot()` in the native test renderer
- Strengthen screenshot integration test to assert visual output changes (compare PNG bytes before vs after interaction)
- Update screenshot test fixture to use a high-contrast background toggle so black-frame regressions are obvious

## 2026-03-01 15:40 UTC

- Switch TestGpuixRenderer from `TestAppContext` (no GPU) to `VisualTestAppContext` (real Metal rendering on macOS)
- Add `gpui_macos` dependency for `MacPlatform` — provides real Metal GPU rendering in test windows
- Replace raw `VisualTestContext` pointer with `VisualTestAppContext` + `AnyWindowHandle` in thread_local storage
- Add `capture_screenshot(path)` napi method — renders via Metal, reads back pixels, saves as PNG
- Add `captureScreenshot(path)` JS wrapper to `TestRenderer`
- Add screenshot integration test (renders counter, clicks, captures before/after PNGs)
- Gate `test_renderer` module on `#[cfg(all(feature = "test-support", target_os = "macos"))]`
- All 19 tests pass (18 existing event/tree tests + 1 new screenshot test)

## 2026-03-01 15:24 UTC

- Fix missing text in macOS visual screenshots by enabling `gpui_macos/font-kit` under `test-support`
- Keep `VisualTestAppContext` on real `MacTextSystem` instead of fallback `NoopTextSystem`, restoring glyph rasterization in `capture_screenshot()`
- Validate with an example-like counter render: text labels (`0/1`, `+`, `-`, `Reset`) now appear correctly in captured PNGs

## 2026-03-01 12:50 UTC

- Add plan for GPU-backed test renderer with screenshot support (`docs/visual-screenshot-plan.md`)
- Plan uses GPUI's `VisualTestAppContext` + Metal rendering on macOS (Oracle-reviewed, original headless wgpu approach rejected due to `WgpuRenderer` being surface-bound)

## 2026-03-01 12:25 UTC

- Add changelog requirement to AGENTS.md
- Document auto-generated napi-rs files in AGENTS.md (`index.d.ts`, `index.js`, `*.node`)

## 2026-03-01 12:00 UTC

- Add `simulate_key_down(keystroke, is_held?)` and `simulate_key_up(keystroke)` to Rust TestGpuixRenderer for fine-grained key event testing
- Extend `simulate_mouse_move(x, y, pressed_button?)` to accept optional pressed button for drag simulation
- Add `nativeSimulateKeyDown`, `nativeSimulateKeyUp` JS wrappers to TestRenderer
- Update `nativeSimulateMouseMove` to pass pressed button through to native
- Restore dropped tests: keyUp state update, keyDown+keyUp sequence, mouse button mapping (left/right/middle), drag pressedButton
- Tighten weak assertions: scroll checks exact deltaX/deltaY/touchPhase, mouseMove checks exact x/y
- Fix stale "mock-only mode" comment in testing.ts

## 2026-03-01 11:45 UTC

- Migrate all event tests from JS-only simulation to native GPUI end-to-end simulation
- Add `simulate_mouse_down(x, y, button)` and `simulate_mouse_up(x, y, button)` to Rust TestGpuixRenderer
- Add `nativeSimulateMouseDown` and `nativeSimulateMouseUp` JS wrappers to TestRenderer
- Remove all 10 JS-only simulation methods from TestRenderer (`simulateEvent`, `simulateClick`, `simulateKeyDown`, `simulateKeyUp`, `simulateMouseEnter`, `simulateMouseLeave`, `simulateMouseDown`, `simulateMouseUp`, `simulateMouseMove`, `simulateScroll`)
- Rewrite all tests to use coordinate-based native GPUI simulation with explicit element sizes
- Change key names from `"arrowDown"`/`"arrowUp"` to GPUI names `"down"`/`"up"`
