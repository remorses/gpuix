---
"@gpuix/native": patch
"@gpuix/react": patch
---

Read every CSS `cursor` keyword and show the I-beam over selectable text.

`cursor` only knew `pointer` and `default`. It now maps `text`, `grab`,
`not-allowed`, the resize directions and the other keywords to their native
cursors. Selectable text shows the I-beam when no ancestor sets a cursor,
which is what `cursor: auto` does on the web.
