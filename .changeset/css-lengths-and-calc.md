---
"@gpuix/native": minor
"@gpuix/react": minor
---

Read a `lineHeight` string the way CSS reads it, and accept `calc()` in any
length.

A JS number keeps the old meaning, so `lineHeight: 20` is still 20 px. A string
follows CSS: `"2.5"` is a multiple of the font size, `"250%"` is the same
multiple, and `"24px"` is 24 px. Anything at or below zero declares nothing.

Every length also takes `calc()`, `min()`, `max()` and `clamp()`, folded by
lightningcss while the value parses. `rem` becomes pixels first, against the
window rem size, so `calc(1rem + 4px)` reaches a single number. This is what
makes the Tailwind spacing scale work, because every step in it is
`calc(var(--spacing) * n)`.
