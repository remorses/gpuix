---
"@gpuix/native": minor
---

On Windows and Linux, `getElementBounds`, `getElementsAt`, `getAutomationTree` and `getWindowSize` now answer from the last painted frame instead of asking the UI thread. A read no longer forces a frame, so polling bounds leaves an idle window at 0 frames per second, and the reads keep working while a native file dialog blocks the UI thread. `getAutomationTree` no longer invalidates the window either.

Each of the four reads takes an optional last argument, `forceRender`. Pass `true` to draw a frame first and read its result. It works the same on macOS, Windows and Linux, and the automation client uses it for its own reads.

`readWindowSize` keeps the last size it read for a renderer when a later read fails, instead of returning 800x600, so `observeWindowSize` no longer reports a false resize.

Fixes #125
