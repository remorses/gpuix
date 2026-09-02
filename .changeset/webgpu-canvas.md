---
'@gpuix/native': minor
'@gpuix/react': minor
---

Add a desktop WebGPU API and a `<canvas source>` element so apps can draw with wgpu and composite into the GPUI window.

`createGPUCanvas()` / `installWebGpu()` expose `navigator.gpu`, `GPUDevice`, and `canvas.getContext('webgpu')` for Three.js `WebGPURenderer`. Present copies pixels through `paint_image` on every OS. Untextured materials work. MSAA, cube maps, and `writeTexture` are not implemented. Keep the `GPUCanvas` object alive and call `destroy()` when done.
