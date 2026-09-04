---
"@gpuix/native": patch
"@gpuix/react": patch
---

Blur a masked backdrop with a true variable Gaussian blur, like the
variable blur filter of iOS. Each pixel gets a Gaussian blur whose sigma
is the mask value at that pixel times the full sigma. The old path
blurred the backdrop at three fixed widths and mixed them, so the width
between the levels was only an approximation. The new path runs two
blur passes that read the mask, so the width follows the gradient
exactly across its whole ramp. The mask read clamps to the bounds of
the layer, so a strip below the window top keeps its full blur at its
top edge instead of mixing in the sharp rows above it.
