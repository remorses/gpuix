---
"@gpuix/native": patch
---

Clip a rounded scroll box at its corner arcs, and end the scroll range at the far edge of the content.

A box that hides or scrolls its overflow used to clip its children to a
rectangle, so a child could poke out over a rounded corner by a few
pixels. Such a box now clips on the GPU with the same rounded shape it
paints. The scroll range used to count the start-side padding twice,
which let a padded scroll box scroll sideways past its content and show
a bar it did not need. The range now ends at the far edge of the
children plus the end-side padding, as in CSS, and an absolutely
positioned child ends the range at its own edge.
