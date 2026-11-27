---
"@gpuix/react": patch
---

CSS shorthand strings in `padding` / `margin` style fields no longer blank the tree with an opaque `setStyle parse error: invalid type: string, expected f64`. The adapter now rejects them before the batch, with an error naming the property and value and pointing at the per-side fields, and converts unambiguous single pixel lengths (`paddingTop: "8px"`) to numbers.

Fixes the error half of #145; parsing full shorthands in the Rust style deserializer remains open for a native-side fix.
