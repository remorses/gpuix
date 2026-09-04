---
"@gpuix/native": minor
---

Read every colour with the same CSS parser the rest of the engine uses.

GPUIX held two CSS parsers. Colour went through csscolorparser, and every other
value went through lightningcss. The two agreed on most colours and disagreed at
the edges, which is where the specifications matter most. Colour now goes through
lightningcss as well, and csscolorparser is gone.

Three colour syntaxes work that did not before:

- `color-mix()`, which Tailwind writes for every opacity modifier such as
  `bg-red-500/50`
- `light-dark()`, which reads the appearance of the window
- `color()`, such as `color(display-p3 1 0 0)`

`currentColor` now resolves wherever it sits, including inside `light-dark()`.
Before, only a value that was exactly `currentColor` resolved, and a nested one
made the whole declaration invalid.

Four syntaxes no longer parse, because no CSS specification defines any of them:

- `hsv()` and `hsva()`
- `hwba()`
- hex with no leading `#`, such as `ff0000ff`

A declaration that uses one of these is invalid, so the property keeps the value
it would have had with no declaration at all. Write `hwb()` instead of `hwba()`,
and add the `#` to a bare hex colour. There is no CSS replacement for `hsv()`,
so convert the value to `hsl()` or `hwb()`.

Alpha on `rgb()`, `hsl()` and `hwb()` now rounds to 8 bits, because that is how
lightningcss holds an sRGB colour. The wider colour spaces keep the exact value.
