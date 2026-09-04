---
"@gpuix/native": minor
"@gpuix/react": minor
---

Add `corner-shape` from CSS Borders 4.

`cornerShape` takes `round`, `squircle`, `square`, `bevel`, `scoop`, `notch`
or `superellipse(K)`, one to four values like `borderRadius`. The per-corner
and per-side longhands, their logical names, and the `corner*` shorthands
that pair a radius with a shape are read too. The quad shader draws each
corner as a superellipse, so a squircle costs the same as a circle. Motion
interpolates `cornerShape` in the half-corner space the spec names. Shadows
and image corners still follow the round shape.
