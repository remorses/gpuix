---
"@gpuix/native": minor
"@gpuix/react": patch
---

Resolve each element style once instead of on every frame. GPUI rebuilds its element tree on every frame, so the renderer used to run all 52 style branches again for styles that had not changed since the last update from React. The resolved style is now kept on the retained element and dropped when the style changes.

Apply the `visibility` style. It reached the native side but nothing read it, so `visibility: "hidden"` did nothing.

Keep the element style when React hides an element. `hideInstance` replaced the whole style with `visibility: "hidden"`, which dropped the layout box and every other style on the element. The `hover` and `active` styles are dropped while an element is hidden, so neither can paint an element React asked to hide.
