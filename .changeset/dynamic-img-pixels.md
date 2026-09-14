---
'@gpuix/native': minor
'@gpuix/react': minor
---

Add desktop `updateImage(elementId, width, height, bgra)` and `clearImage(elementId)` for existing `<img>` nodes. Pixels are copied from a tightly packed BGRA Uint8Array, rendered through ordinary GPUI img, and released on clear, src change, resize, or destruction. Same-size updates preserve native image identity. Sampling remains linear. Uploads are limited to 4096 pixels per axis (64 MiB) before copying; GPU failures fall back to src and are not retried until a new version is submitted. Upstream atlas search also found [zed-industries/zed#54659](https://github.com/zed-industries/zed/issues/54659) (allocation failures) and [#57516](https://github.com/zed-industries/zed/pull/57516) (tile lifetime); these remain upstream work, not fork changes in this PR.

Bump GPUI to merged [remorses/zed#4](https://github.com/remorses/zed/pull/4) (`81c99f816b4a5f69d3c014774068034c24d1d7af`) for `RenderImage::from_bgra` and `Window::update_image`. Upstream searches for `from_bgra` and `update_image` found no matching issues/PRs; upstream nearest-neighbor [zed-industries/zed#57393](https://github.com/zed-industries/zed/pull/57393) remains open. Ordinary GPUI Img has no sampling builder at this revision, so that feature is deferred rather than reimplementing its painting in GPUIX.
