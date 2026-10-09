---
'@gpuix/react': patch
---

Park spring `motion.div` frame leases at rest so a settled spring stops calling `setCurrent` and no longer rebuilds GPUI every frame. A retarget re-subscribes `onFrame`. Soft GELATIN crawl hard-settles between 280ms and 420ms. Tween `motion.div` is unchanged.
