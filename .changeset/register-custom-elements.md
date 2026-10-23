---
"@gpuix/native": minor
"@gpuix/react": minor
"@gpuix/solid": minor
---

Crates that link `gpuix-native` can add their own native elements with `register_custom_element`. The `CustomElement` and `CustomElementFactory` traits, `CustomRenderContext`, `custom_surface`, `emit_event_full` and `GpuixView` are now public, and `gpuix_native::gpui` re-exports the GPUI the renderer uses. Element names must contain a `-`; `@gpuix/solid` now accepts those tags instead of throwing.

Any function prop named `on` + uppercase is now an event handler, in React and Solid: `onGreet` listens for `greet`. Before, only a fixed list of names worked and other handlers were dropped silently. A value that is not a function stays a normal prop, so `onTop={true}` still reaches the element. Events carry a new `detail` field that takes any JSON.

Fixes #110
