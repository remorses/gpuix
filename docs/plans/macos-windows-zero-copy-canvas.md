---
title: Zero-copy WebGPU canvas on macOS and Windows
description: >
  Plan to present a GPUIX GPUCanvas without a CPU readback. macOS uses
  IOSurface plus BGRA paint_surface. Windows needs a wgpu renderer and
  DXGI shared textures.
---

# Zero-copy WebGPU canvas on macOS and Windows

`<canvas>` today paints through `paint_image`. That is CPU BGRA every
frame. Linux can sample an `Arc<wgpu::Texture>` in-scene after the
gpui-ce compositor port. macOS and Windows cannot.

This plan is only the present path. The napi WebGPU API stays.

```
Three.js  ►  wgpu GPUCanvas texture  ►  shared GPU buffer  ►  GPUI samples it
                                           │
                         macOS: IOSurface / CVPixelBuffer
                         Windows: DXGI NT handle (needs wgpu, not D3D11)
                         Linux: already Arc<wgpu::Texture> on the window device
```

## Where we are

| OS | Window GPU | Canvas present today | Zero-copy door |
|---|---|---|---|
| **Linux** | wgpu 29 / Vulkan | `paint_surface(Arc<wgpu::Texture>)` if same device | Done in `remorses/zed` `gpuix` (`8802c83138`) |
| **macOS** | Metal | `paint_image` CPU snapshot | `paint_surface(CVPixelBuffer)` is YCbCr-only. Aborts on BGRA |
| **Windows** | Direct3D **11** | `paint_image` | No texture composite. wgpu is DX12 |

Linux still fails if `GPU.create()` makes a **second** wgpu instance.
`GPUAdapter.requestDevice` must return `Window::gpu_context()` there.
That is a GPUIX change, not this plan.

## macOS: IOSurface, then same `MTLDevice`

Two steps. Do **A** first. **B** is the real browser path.

### A. IOSurface → `CVPixelBuffer` → `paint_surface`

GPUI already composites `CVPixelBuffer` on Metal. The buffer must be
YCbCr or the renderer **aborts** (`assert_eq!` in
`zed/crates/gpui_apple/src/metal_renderer.rs`).

[zed#61291](https://github.com/zed-industries/zed/pull/61291) (open)
adds BGRA. **~88 lines. Copy it.**

| File | Diff |
|---|---|
| `crates/gpui_apple/src/metal_renderer.rs` | +76 / −6. `draw_bgra_surface` via `CVMetalTextureCache` |
| `crates/gpui_apple/src/shaders.metal` | +12. `surface_fragment_bgra`, nearest, straight alpha |

Author validated CEF at 1200×1602@2x, 60 fps. CPU vs `paint_image`
dropped from ~25–40% of a core to ~6%.

After that lands on `gpuix`:

1. Allocate an **IOSurface** (BGRA, canvas size × scale)
2. Create an `MTLTexture` with
   `newTextureWithDescriptor:iosurface:plane:`
3. Wrap that Metal texture as `wgpu::Texture` with
   `wgpu_hal::metal::Device::texture_from_raw` +
   `create_texture_from_hal`
4. Three.js renders into that wgpu texture
5. Wrap the same IOSurface:
   `CVPixelBufferCreateWithIOSurface`
6. `window.paint_surface(bounds, pixel_buffer)`

No CPU copy. wgpu and Metal still may be **two devices**. On Apple
Silicon that is unified RAM. It is a GPU alias, not a memcpy.

Pool 2–3 IOSurfaces. Do not allocate per frame.
[syphon-metal](https://github.com/BlueJayLouche/syphon-rs/tree/main/syphon-metal)
does that (`IOSurfacePool`).

### B. Same `MTLDevice` (later)

Wrap GPUI’s Metal device with wgpu-hal so the canvas texture **is** a
Metal texture GPUI can bind.

wgpu already has the hooks:

- `device_from_raw` / `queue_from_raw` /
  `texture_from_raw`
  ([gfx-rs/wgpu#3338](https://github.com/gfx-rs/wgpu/pull/3338),
  [wgpu-hal metal device.rs](https://github.com/gfx-rs/wgpu/blob/trunk/wgpu-hal/src/metal/device.rs))

[zed#60573](https://github.com/zed-industries/zed/pull/60573) (closed)
is **not** this. Its Metal arm is an empty match. Background colour
shows. Do not copy that stub.

Need new Metal `draw_surfaces` for a raw `MTLTexture`, plus
`Window::gpu_context()` on macOS. That is 1–2 weeks after A.

## Windows: wgpu first, then DXGI share

GPUIX Windows is **D3D11**. wgpu 29 is **DX12**. You cannot wrap a
D3D11 device as WebGPU.

Order:

1. Move GPUIX Windows to `gpui_wgpu` (gpui-ce already did this with
   `wgpu-surfaces`). See
   [gpui-ce#121](https://github.com/gpui-ce/gpui-ce/pull/121).
2. Then either:
   - **Same device** (best). `GPU.create()` returns
     `Window::gpu_context()`. Sample `Arc<wgpu::Texture>` like Linux.
   - **DXGI NT handle** if the canvas must stay on another D3D12
     device. `CreateSharedHandle` → `OpenSharedHandle` →
     `wgpu_hal::dx12::Device::texture_from_raw`. Sync with
     `IDXGIKeyedMutex` or an `ID3D12Fence`.

Do not invent a D3D11 keyed-mutex path into the current DirectX
renderer. That fights the wgpu migration.

## Related work (copy these, do not rewrite)

**GPUI / Zed**

- [zed#61291](https://github.com/zed-industries/zed/pull/61291) —
  BGRA `CVPixelBuffer` on Metal. **Use this.**
- [zed#60573](https://github.com/zed-industries/zed/pull/60573) —
  wgpu external compositor. Linux-shaped. Metal no-op. Closed.
- [zed discussion #60572](https://github.com/zed-industries/zed/discussions/60572) —
  design thread for 60573.
- [gpui-ce surface.rs](https://github.com/gpui-ce/gpui-ce/blob/main/crates/gpui/src/elements/surface.rs) —
  `SurfaceSource::Texture` is Linux / Windows+wgpu only. macOS is
  still `CVPixelBuffer`.
- [gpui-ce#39](https://github.com/gpui-ce/gpui-ce/commit/6d043b22e477) /
  [gpui-ce#121](https://github.com/gpui-ce/gpui-ce/pull/121) —
  Linux/Windows texture composite. Already ported to `gpuix`.

**IOSurface + wgpu / Metal (the canvas wrap)**

- [slint servo metal.rs](https://github.com/slint-ui/slint/blob/master/examples/servo/src/webview/rendering_context/metal.rs) —
  IOSurface → `newTextureWithDescriptor:iosurface:plane:` →
  `texture_from_raw` → `create_texture_from_hal`. Closest copy-paste.
- [grafting `raw_gl/metal.rs`](https://docs.rs/grafting/latest/grafting/) —
  same three steps, then optional BGRA→RGBA blit.
- [wgpu-native-texture-interop](https://docs.rs/wgpu-native-texture-interop/latest/wgpu_native_texture_interop/) —
  same Metal import, plus DX12 `OpenSharedHandle`.
- [bevy_cef#56](https://github.com/not-elm/bevy_cef/pull/56) —
  CEF `OnAcceleratedPaint` IOSurface. Import **inside** the frame
  encoder. Extra `queue.submit` from a callback races present.
- [syphon-metal](https://github.com/BlueJayLouche/syphon-rs/tree/main/syphon-metal) —
  `IOSurfacePool`, `create_texture_from_iosurface`,
  `MetalContext::from_wgpu_device`.
- [CefSwift](https://github.com/Rajaniraiyn/CefSwift) —
  OSR → IOSurface → `CALayer`. Layer overlay, not in-scene.
- [encse/cef-test](https://github.com/encse/cef-test) —
  older CEF + Metal HUD POC.
- [Chromium `io_surface.cc`](https://chromium.googlesource.com/chromium/src/+/master/ui/gfx/mac/io_surface.cc) —
  how Chrome allocates IOSurfaces (`IOSurfaceCreate`, pixel format,
  plane layout).

**Windows DXGI**

- [grafting `dx12_shared_texture.rs`](https://docs.rs/grafting/latest/grafting/) —
  `CreateSharedHandle` / `OpenSharedHandle` /
  `texture_from_raw`.
- [IDXGIKeyedMutex::AcquireSync](https://learn.microsoft.com/en-us/windows/win32/api/dxgi/nf-dxgi-idxgikeyedmutex-acquiresync)
- [ID3D12Device::OpenSharedHandle](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-opensharedhandle)
- [wgpu#4067](https://github.com/gfx-rs/wgpu/issues/4067) —
  public import/export of native textures is still “not yet” at the
  wgpu-native C API. HAL `texture_from_raw` is the Rust path.

**Apple docs**

- [IOSurface](https://developer.apple.com/documentation/iosurface/iosurface)
- [CVPixelBuffer](https://developer.apple.com/documentation/corevideo/cvpixelbuffer)
- [MTLDevice](https://developer.apple.com/documentation/metal/mtldevice)

## AppKit / CoreVideo APIs to use with WebGPU

These are the handles, not a second canvas API.

| API | Role |
|---|---|
| `IOSurfaceCreate` | Shared GPU buffer. Width, height, `kIOSurfacePixelFormat` = `'BGRA'` |
| `MTLDevice.newTextureWithDescriptor:iosurface:plane:` | Metal view of that buffer |
| `wgpu_hal::metal::Device::texture_from_raw` | wgpu view of that Metal texture |
| `CVPixelBufferCreateWithIOSurface` | GPUI `paint_surface` input |
| `CVMetalTextureCacheCreateTextureFromImage` | What #61291 already uses to bind the buffer in Metal |
| `IOSurfaceLock` / `Unlock` | Only if CPU ever touches the pages. Skip for GPU-only |

Do **not** put a second `CAMetalLayer` over the GPUI window. That is
not a flex child. Clicks, clip, and z-order break.
[CefSwift](https://github.com/Rajaniraiyn/CefSwift) does that for a
webview overlay. GPUIX canvas must stay in the scene.

## Phases

**0. Linux same-device (GPUIX, small)**  
`requestDevice` returns `window.gpu_context()`. Until then, Linux
must `paint_image` too or wgpu panics on `same_device`.

**1. Cherry-pick [zed#61291](https://github.com/zed-industries/zed/pull/61291) onto `gpuix`**  
Metal accepts BGRA `CVPixelBuffer`. ~88 lines. Test: wrap a solid
BGRA IOSurface and `paint_surface` it. No WebGPU yet.

**2. macOS canvas on IOSurface**  
Copy the Slint / grafting import. Pool surfaces. `<canvas>` calls
`paint_surface` instead of `paint_image`. Keep CPU snapshot as
fallback if wrap fails.

**3. Windows wgpu renderer**  
Follow gpui-ce Windows wgpu. Then same-device composite like Linux.

**4. Optional: macOS same `MTLDevice`**  
Drop the IOSurface hop. Metal samples the wgpu texture directly.

## Tests

1. Native: create BGRA `CVPixelBuffer`, `paint_surface`, screenshot
   is not black. Proves #61291.
2. Native: wgpu clear magenta into an IOSurface-backed texture,
   composite, centre pixel is magenta. No `readPixels` CPU path.
3. `examples/three-webgpu.tsx` cube on macOS. Same PNG assert as
   today, but `canvas_snapshot` must not run (log or counter).
4. Resize the canvas. New IOSurface. Old one dropped.
5. Windows: skip until wgpu renderer. Do not fake D3D11 share.

## Do not

- Wait for [zed#60573](https://github.com/zed-industries/zed/pull/60573).
  Closed. Metal empty.
- Overlay a `CAMetalLayer`.
- Add Dawn or `@napi-rs/canvas`.
- Share across two wgpu instances without IOSurface / DXGI.
- Copy Slint’s Y-flip blit unless GPUI samples upside down. GPUI
  Metal and wgpu Metal agree on origin more often than GL.

## Size

| Phase | Effort |
|---|---|
| 1. #61291 | 1 day |
| 2. IOSurface canvas | 3–5 days |
| 3. Windows wgpu | week+ (platform swap) |
| 4. Same MTLDevice | 1–2 weeks after 2 |
