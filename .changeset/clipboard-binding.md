---
"@gpuix/native": minor
"@gpuix/react": minor
---

Let apps use the OS clipboard. The renderer gains `writeToClipboard(text)` and
`readFromClipboard()`, and `@gpuix/react` exports `copyText(text)`. Both calls
are synchronous: desktop GPUI platforms read and write the platform clipboard
directly. A read returns `null` when the clipboard holds no text at all, and an
empty string is a real entry, so `copyText('')` clears the clipboard.
