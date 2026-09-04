---
"@gpuix/native": patch
---

Split the renderer into modules by subject.

`renderer.rs` held the napi binding, the GPUI view, the frame walk, the virtual
list state and the batch parser in one 3,358 line file. The frame walk now lives
in `renderer/frame.rs`, the retained state of one virtual list in
`renderer/virtual_list.rs`, and the batch parser in `renderer/batch.rs`.

Nothing changed about what any of it does.
