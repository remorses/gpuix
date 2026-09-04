---
"@gpuix/native": patch
"@gpuix/react": patch
---

Animate `height` to `auto`

A motion `height` now takes `"auto"` at either end of the animation. `auto` is
the height the content takes, and only layout knows that number, so the element
measures its content every frame and interpolates against the measurement. An
animation that opens a panel follows content that changes while it runs.

The measurement runs at the width the element really gets, whether that width
comes from a declared length, from `flex`, from a percentage or from a stretched
cross axis. Text wraps the way it will on screen.
