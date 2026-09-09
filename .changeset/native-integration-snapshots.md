---
'@gpuix/native': minor
'@gpuix/react': minor
---

Add desktop `getNativeWindowHandle()` and non-flushing `getElementPaintState(id)` snapshots, with React types and test renderer forwarding. Handles are borrowed, tagged Buffers; paint geometry includes logical bounds, rectangular clipping and scale. Neither query manages child lifetimes or dispatches downstream FFI onto GPUI's UI thread.

GPU-backed test windows may return native handles; replaced test renderers cannot query another renderer's handle. These observational queries do not add style behavior.

No Zed submodule change or dynamic surface implementation. The existing pin already supplies `HasWindowHandle`, `HasDisplayHandle`, and paint-time bounds. Upstream research: [#24327](https://github.com/zed-industries/zed/pull/24327) merged the window traits, [#50768](https://github.com/zed-industries/zed/pull/50768) merged X11 support, and [#62775](https://github.com/zed-industries/zed/pull/62775) merged headless `NotSupported` handling. The pinned `InteractiveElement::on_painted` is available locally; upstream issue/PR and code searches for that exact symbol returned no matches. [remorses/zed#8](https://github.com/remorses/zed/pull/8) remains open/conflicting and is not required or imported.
