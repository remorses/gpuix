---
"@gpuix/native": minor
---

Let apps use the OS clipboard with `writeToClipboard(text)` and
`readFromClipboard()` on the renderer, reachable from React and Solid through
`useGpuixRequired()`. Both calls are synchronous: desktop GPUI platforms read
and write the platform clipboard directly. A read returns `null` when the
clipboard holds no text at all, and an empty string is a real entry, so
`writeToClipboard('')` clears the clipboard.
