---
"@gpuix/native": patch
---

A `scrollTo` that lands before the first frame of its element now
sticks. A mount effect runs in the commit that creates the element, so
an app can restore a saved scroll offset on a screen that never
painted, the way scroll restoration works on the web. When the element
has no scroll state yet, the engine holds the offset, and the frame
that creates the state starts it there. On macOS the commit paints
before the effect runs, so the state already exists there. The held
offset covers the backends that queue their scroll commands.
