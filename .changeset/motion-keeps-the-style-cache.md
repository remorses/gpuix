---
"@gpuix/native": patch
"@gpuix/react": patch
---

Keep the resolved-style cache for an element that animates.

A motion frame drives eight numbers: `width`, `height`, `top`, `right`,
`bottom`, `left`, `borderRadius` and `opacity`. It used to drive them by
copying the element's whole style, writing the numbers into the copy and
resolving that copy. Every declaration the element made was reparsed on every
frame of the animation to change one value.

None of the eight reads a custom property, `currentColor` or the font size, so
each one now lands on the element after the cached resolution does. An animated
element resolves its style once, the same as any other element.

Custom elements resolve a style themselves, so a motion frame still reaches
them folded into one.
