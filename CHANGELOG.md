# Changelog

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
