---
"@gpuix/native": minor
"@gpuix/react": minor
---

Add `locator.scrollIntoView()` to the automation client. It scrolls the
element's nearest scroll parent until the element is visible, over the same
renderer `scrollIntoView` a host ref uses, so a test can reach below-the-fold
content before `click()` instead of hand-rolling `mouse.wheel` loops.
