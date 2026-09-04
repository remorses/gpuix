---
"@gpuix/native": patch
---

Solve a gradient easing per fragment in the shader.

The easing between two colour stops painted as the GPUI colour hint that
crosses one half at the same place, which agrees with the easing only at both
ends and at the half-way point. The bumped GPUI carries the easing control
points to the GPU, and the shader solves the curve at every fragment, so the
whole curve paints exactly.
