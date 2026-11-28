---
'@gpuix/react': patch
---

Add a video player example with sound, built on
[Mediabunny](https://mediabunny.dev). It plays a local file or a URL with play,
pause, skip, mute, a scrubbable seek bar, and keyboard shortcuts:

```bash
cd examples && bun --hot video-player.tsx ~/Movies/clip.mp4
```

```
file / URL ─► Mediabunny ─┬─► VideoDecoder ─► BGRA ─► <img>.setImagePixels
                          └─► AudioDecoder ─► AudioBuffer ─► AudioContext (master clock)
```

Mediabunny decodes through the standard WebCodecs `VideoDecoder` and
`AudioDecoder`, like in a browser. Bun has no WebCodecs, so the example imports
`@napi-rs/webcodecs/polyfill` first. Each decoded frame is copied out as BGRA
and uploaded to a plain `<img>` with `setImagePixels`. Audio plays through
`node-web-audio-api`, and its `AudioContext` clock times the frames, so pause,
seek, and skip move sound and picture together. No new GPUIX API is involved.

On an M-series Mac a 1080p60 file plays all 360 frames, and on a 720p trailer
the median frame lands 4 ms behind the sound. The README shows the minimal
decode and audio scheduling loops under **Live images from a buffer**.

This is a first step for #92. Frames still take a CPU copy: GPUIX has no
zero-copy GPU video surface yet.
