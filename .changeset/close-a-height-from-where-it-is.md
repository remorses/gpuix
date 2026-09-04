---
"@gpuix/native": patch
"@gpuix/react": patch
---

Collapse a `height: auto` animation from the height on screen.

Opening a panel animated to the height the content takes, but closing it snapped
shut in one frame. Every frame of the closing animation measured zero.

A motion height now carries a number of pixels and a share of the height the
content takes. `"auto"` is the whole share, a length is none of it, and a frame
between the two is part of each, so `"auto"` and a length are the same kind of
value. A collapse starts from the height that is on screen, and pressing the
button again part way through turns back from the frame it reached instead of
jumping.
