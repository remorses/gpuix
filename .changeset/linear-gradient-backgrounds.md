---
"@gpuix/native": minor
"@gpuix/react": minor
---

Paint `linear-gradient()` backgrounds.

`background` and the new `backgroundImage` take a `linear-gradient()` with
an angle, a side or a corner, up to eight colour stops, percentage positions
and colour hints. The engine fixes the stops up the way CSS Images 3 says and
the quad shader paints them, so a gradient costs one quad like a flat colour.
`backgroundImage` wins over `backgroundColor`, since a box paints one fill.
Stop lengths, radial and conic gradients are not read yet.
