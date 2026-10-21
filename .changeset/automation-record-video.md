---
'@gpuix/native': minor
'@gpuix/react': minor
'@gpuix/solid': minor
---

Add `app.recordVideo({ path, fps })` to the automation client. It pauses the
native clock and captures one screenshot per frame at exact timestamps, so a
recording is smooth at any frame rate even when a frame is slow to render.
Drive the app between `video.frame()` or `video.wait(ms)` calls, then
`video.stop()` encodes `.mp4`, `.webm` or `.gif` with ffmpeg and resumes the
clock. Available for `connectTest()` and `launch()`; browser pages still use
the controlling browser tool.
