# Changelog

## 2026-03-01 18:52 UTC

- Add new `<anchored>` custom element with GPUI `anchored()` positioning props (`x`/`y`, `position`, `anchor`, `snapToWindow`, `snapMargin`) and optional deferred overlay rendering (`deferred`, `priority`)
- Extend custom element render context to pass built child elements so custom primitives can wrap and position nested React content
- Register `anchored` in the default custom element registry and expose it in React intrinsic types/component map
- Add end-to-end anchored deferred dialog overlay test (open, inside click stays open, outside click closes)

## 2026-03-01 18:47 UTC

- Add dialog overlay screenshot regression test that captures before/after PNGs and asserts visual output changes when opening the dialog

## 2026-03-01 18:45 UTC

- Add absolute positioning support in native style mapping (`position`, `top`, `right`, `bottom`, `left`) so React styles place elements out of flow like dialogs/tooltips
- Add end-to-end dialog overlay test: click button opens tooltip-like dialog content, inside click keeps it open, outside click closes via `onMouseDownOutside`

## 2026-03-01 18:35 UTC

- Add polymorphic custom element trait infrastructure (`CustomElement`, `CustomElementFactory`, `CustomElementRegistry`)
- Implement `<input>` as first custom element with value/placeholder/readOnly props and keyboard event handling
- Add `custom_props` field to `RetainedElement` for storing non-style/non-event props on custom elements
- Add `setCustomProp`/`getCustomProp` napi methods on both `GpuixRenderer` and `TestGpuixRenderer`
- Add custom prop forwarding in React reconciler (`host-config.ts`) — automatically syncs non-reserved props for non-div/text elements
- Add `InputProps` type and `input` to JSX IntrinsicElements
- Add 6 end-to-end tests: input rendering, keyboard typing (controlled component), backspace, screenshot before/after, tree structure
- Fix jsx-dev-runtime.js to export `jsxDEV` for React 19 compatibility with vitest (was breaking all tests)
- All 27 tests pass (6 new input + 21 existing events)

## 2026-03-01 17:42 UTC

- Fix custom element lifecycle cleanup by pruning/destroying stale trait instances when IDs disappear from the retained tree
- Fix stale custom prop state by resetting missing known props to `null` each frame via `supported_props()` synchronization
- Apply retained `style` to custom elements through `CustomRenderContext` so `<input style={...}>` affects native layout/hit-testing
- Filter custom element event wiring to declared `supported_events()` only
- Harden React custom prop forwarding with safe JSON serialization fallback (`null` on unsupported/circular values)
- Expand input end-to-end coverage with `readOnly` removal regression test and style-based click hit-test assertion

## 2026-03-01 17:15 UTC

- Rewrite README to reflect current mutation-based architecture (was describing old JSON tree approach)
- Replace "description-based renderer" language with "mutation-based protocol over napi-rs FFI"
- Add architecture diagram showing individual napi calls (createElement, appendChild, setStyle, commitMutations)
- Add Mutation API section documenting the full NativeRenderer interface
- Add Event Flow section with pipeline diagram (GPUI → Rust closure → ThreadsafeFunction → JS event registry → React handler)
- Add detailed events table with payload fields for each event type
- Add Testing section covering TestGpuixRenderer (GPU-backed Metal tests, screenshot capture, native event simulation)
- Update status checklist: mark keyboard events, focus/blur, scroll, click-outside, and test renderer as completed
- Update usage example to use createRenderer() instead of raw GpuixRenderer constructor

## 2026-03-01 16:48 UTC

- Center screenshot probe cards in the visual renderer tests so captured frames represent realistic composition instead of top-left anchored blocks
- Improve screenshot test visuals with richer card styling (rounded surfaces, palette contrast, readable text hierarchy)
- Keep visual assertions unchanged (before/after PNG difference) while moving click/hover simulation coordinates to centered card hit zones

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
