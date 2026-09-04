---
"@gpuix/native": minor
"@gpuix/react": minor
---

Add `filter`, `backdrop-filter`, `mask-image`, `mix-blend-mode`, `background-blend-mode` and `overscroll-behavior`.

An element with any of the first four paints itself and its children into a
texture of its own, and the GPU paints that texture over the frame with the
effect. `blur()` is a two-pass Gaussian on a shrunk texture, so a wide blur
costs the same as a narrow one. The other filter functions fold into one
colour matrix. `mask-image` takes a gradient and keeps each pixel by its
alpha. Over a `backdrop-filter: blur()` the alpha scales the blur radius
instead, so an eased mask gives a progressive blur like the iOS 26
navigation bar. CSS would fade a sharp copy over the blur there. All sixteen blend modes of Compositing and Blending 1 plus
`plus-lighter` are painted. `background-image` now paints over
`background-color` instead of replacing it. A scroll box keeps a wheel event
it can scroll with, and `overscroll-behavior: contain` keeps it at the end
of the box too. The Metal, DirectX and wgpu renderers all paint the
effects. Only WebGL, which has no storage buffers, paints the content of
an effect layer with no effect.
