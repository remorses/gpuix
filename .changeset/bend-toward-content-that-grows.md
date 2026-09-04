---
"@gpuix/native": patch
"@gpuix/react": patch
---

Keep a `height` animation smooth when the content changes while it runs.

A `motion` height with `auto` at an end resolves against the content every
frame. Content that grew part way through moved the height with it, so a box
opening toward two rows jumped when two more arrived. The animation now keeps
the height that was on screen and bends the rest of the curve toward the new
content, ending when it would have ended.

At rest, `auto` still follows the content at once.
