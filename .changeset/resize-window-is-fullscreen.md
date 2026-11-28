---
"@gpuix/native": minor
---

Add `resizeWindow(width, height)` and `isFullscreen()` to the desktop renderer,
reachable from React and Solid through `useGpuixRequired()`. They map to GPUI's
`Window::resize` and `Window::is_fullscreen` on macOS, Windows, Linux, and
FreeBSD. `resizeWindow` takes the content size in logical pixels, the same unit
`getWindowSize()` reports, and throws on a non-positive or non-finite value.

Fixes #126
