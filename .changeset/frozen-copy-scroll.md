---
"@gpuix/native": patch
---

Keep the scroll offset of a scrolled screen in its frozen view
transition copy. A frame could build between the capture and the start
call, after the update already removed the old screen from the tree.
That frame dropped the scroll handle of the screen, so its frozen copy
painted from a fresh handle at offset zero, and the list flashed back
to the top for the length of the transition. The cleanup now also keeps
the state of every id inside a pending capture.

The same flash had a second cause. React removes an old subtree in two
steps: the commit unlinks it, and a later batch destroys each element.
For a swap nested under a component, the destroy lands after the start
call, so the old ids still sat in the element map, detached. The remap
took them for live elements, gave the clone fresh ids, and with them a
fresh scroll handle at offset zero. The remap now only treats an id as
live when it still sits under the root.
