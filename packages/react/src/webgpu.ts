/// Spec-shaped WebGPU facade over the @gpuix/native napi classes.
///
/// Three.js WebGPURenderer calls navigator.gpu, canvas.getContext('webgpu'),
/// and device.createXxx with nested descriptors. The napi layer takes split
/// class arguments. This file is the only place that translation lives.
import {
  Gpu as NativeGPU,
  GpuAdapter as NativeGPUAdapter,
  GpuCanvas as NativeGPUCanvas,
  GpuCanvasContext as NativeGPUCanvasContext,
  GpuCommandEncoder as NativeGPUCommandEncoder,
  GpuDevice as NativeGPUDevice,
  GpuSampler as NativeGPUSampler,
  gpuBufferUsage,
  gpuShaderStage,
  gpuTextureUsage,
} from "@gpuix/native"

const bufferUsage = gpuBufferUsage()
const textureUsage = gpuTextureUsage()
const shaderStage = gpuShaderStage()

export const GPUBufferUsage = {
  MAP_READ: bufferUsage.mapRead,
  MAP_WRITE: bufferUsage.mapWrite,
  COPY_SRC: bufferUsage.copySrc,
  COPY_DST: bufferUsage.copyDst,
  INDEX: bufferUsage.index,
  VERTEX: bufferUsage.vertex,
  UNIFORM: bufferUsage.uniform,
  STORAGE: bufferUsage.storage,
  INDIRECT: bufferUsage.indirect,
  QUERY_RESOLVE: bufferUsage.queryResolve,
}

export const GPUTextureUsage = {
  COPY_SRC: textureUsage.copySrc,
  COPY_DST: textureUsage.copyDst,
  TEXTURE_BINDING: textureUsage.textureBinding,
  STORAGE_BINDING: textureUsage.storageBinding,
  RENDER_ATTACHMENT: textureUsage.renderAttachment,
}

export const GPUShaderStage = {
  VERTEX: shaderStage.vertex,
  FRAGMENT: shaderStage.fragment,
  COMPUTE: shaderStage.compute,
}

export const GPUMapMode = {
  READ: 0x0001,
  WRITE: 0x0002,
}

export class GPU {
  #inner = NativeGPU.create()

  requestAdapter(options?: { powerPreference?: string }) {
    return Promise.resolve(new GPUAdapter(this.#inner.requestAdapter(options?.powerPreference)))
  }

  getPreferredCanvasFormat() {
    return this.#inner.getPreferredCanvasFormat()
  }
}

const emptyFeatures = {
  has(_name: string) {
    return false
  },
}

export class GPUAdapter {
  readonly features = emptyFeatures

  constructor(private inner: NativeGPUAdapter) {}

  requestDevice(_descriptor?: object) {
    return Promise.resolve(wrapDevice(this.inner.requestDevice()))
  }

  get info() {
    return this.inner.info
  }

  get isFallbackAdapter() {
    return this.inner.isFallbackAdapter
  }
}

function wrapDevice(inner: NativeGPUDevice) {
  const device = inner as NativeGPUDevice & {
    createCommandEncoder: NativeGPUDevice["createCommandEncoder"]
    createRenderPipeline: NativeGPUDevice["createRenderPipeline"]
    createComputePipeline: NativeGPUDevice["createComputePipeline"]
    createBindGroup: NativeGPUDevice["createBindGroup"]
    createPipelineLayout: NativeGPUDevice["createPipelineLayout"]
    createTexture: NativeGPUDevice["createTexture"]
    lost: Promise<{ reason: string; message: string }>
  }
  // Three.js treats a resolved `device.lost` as a dead GPU.
  device.lost = new Promise(() => {})
  Object.defineProperty(device, "features", {
    value: emptyFeatures,
    enumerable: true,
  })

  const createTexture = inner.createTexture.bind(inner)
  device.createTexture = ((descriptor: {
    label?: string
    size?: number | number[] | { width: number; height?: number; depthOrArrayLayers?: number }
    width?: number
    height?: number
    depth?: number
    format: string
    usage: number
    dimension?: string
    mipLevelCount?: number
    sampleCount?: number
  }) => {
    const size = descriptor.size
    const width = Array.isArray(size)
      ? size[0]
      : typeof size === "number"
        ? size
        : (size?.width ?? descriptor.width ?? 1)
    const height = Array.isArray(size)
      ? (size[1] ?? 1)
      : typeof size === "object"
        ? (size.height ?? 1)
        : (descriptor.height ?? 1)
    const depth = Array.isArray(size)
      ? size[2]
      : typeof size === "object"
        ? size.depthOrArrayLayers
        : descriptor.depth
    return createTexture({
      label: descriptor.label,
      width,
      height,
      depth,
      format: descriptor.format,
      usage: descriptor.usage,
      dimension: descriptor.dimension,
      mipLevelCount: descriptor.mipLevelCount,
      sampleCount: descriptor.sampleCount,
    })
  }) as NativeGPUDevice["createTexture"]

  const createPipelineLayout = inner.createPipelineLayout.bind(inner)
  device.createPipelineLayout = ((descriptor: {
    label?: string
    bindGroupLayouts: Parameters<NativeGPUDevice["createPipelineLayout"]>[1]
  }) => {
    return createPipelineLayout({ label: descriptor.label }, descriptor.bindGroupLayouts)
  }) as NativeGPUDevice["createPipelineLayout"]

  const createBindGroup = inner.createBindGroup.bind(inner)
  device.createBindGroup = ((descriptor: {
    label?: string
    layout: Parameters<NativeGPUDevice["createBindGroup"]>[1]
    entries: Array<{
      binding: number
      resource: { buffer?: unknown; offset?: number; size?: number } | unknown
    }>
  }) => {
    const buffers: unknown[] = []
    const textures: unknown[] = []
    const samplers: unknown[] = []
    const entries = descriptor.entries.map((entry) => {
      const resource = entry.resource as { buffer?: unknown; offset?: number; size?: number }
      if (resource && typeof resource === "object" && "buffer" in resource && resource.buffer) {
        buffers.push(resource.buffer)
        return {
          binding: entry.binding,
          resourceType: "buffer",
          offset: resource.offset,
          size: resource.size,
        }
      }
      if (resource instanceof NativeGPUSampler) {
        samplers.push(resource)
        return { binding: entry.binding, resourceType: "sampler" }
      }
      textures.push(resource)
      return { binding: entry.binding, resourceType: "texture" }
    })
    return createBindGroup(
      { label: descriptor.label },
      descriptor.layout,
      entries,
      buffers as never,
      textures as never,
      samplers as never,
    )
  }) as NativeGPUDevice["createBindGroup"]

  const createRenderPipeline = inner.createRenderPipeline.bind(inner)
  device.createRenderPipeline = ((descriptor: {
    label?: string
    layout?: Parameters<NativeGPUDevice["createRenderPipeline"]>[1] | "auto"
    vertex: {
      module: Parameters<NativeGPUDevice["createRenderPipeline"]>[2]
      entryPoint?: string
      buffers?: unknown
    }
    fragment?: {
      module: Parameters<NativeGPUDevice["createRenderPipeline"]>[3]
      entryPoint?: string
      targets: unknown
    }
    primitive?: unknown
    depthStencil?: unknown
    multisample?: unknown
  }) => {
    const layout = descriptor.layout === "auto" ? undefined : descriptor.layout
    return createRenderPipeline(
      {
        label: descriptor.label,
        vertex: {
          entryPoint: descriptor.vertex.entryPoint ?? "main",
          buffers: descriptor.vertex.buffers as never,
        },
        fragment: descriptor.fragment
          ? {
              entryPoint: descriptor.fragment.entryPoint ?? "main",
              targets: descriptor.fragment.targets as never,
            }
          : undefined,
        primitive: descriptor.primitive as never,
        depthStencil: descriptor.depthStencil as never,
        multisample: descriptor.multisample as never,
      },
      layout,
      descriptor.vertex.module,
      descriptor.fragment?.module,
    )
  }) as NativeGPUDevice["createRenderPipeline"]

  const createComputePipeline = inner.createComputePipeline.bind(inner)
  device.createComputePipeline = ((descriptor: {
    label?: string
    layout?: Parameters<NativeGPUDevice["createComputePipeline"]>[1]
    compute: {
      module: Parameters<NativeGPUDevice["createComputePipeline"]>[2]
      entryPoint?: string
    }
  }) => {
    return createComputePipeline(
      {
        label: descriptor.label,
        entryPoint: descriptor.compute.entryPoint ?? "main",
      },
      descriptor.layout,
      descriptor.compute.module,
    )
  }) as unknown as NativeGPUDevice["createComputePipeline"]

  const createCommandEncoder = inner.createCommandEncoder.bind(inner)
  device.createCommandEncoder = ((descriptor?: { label?: string }) => {
    return wrapEncoder(createCommandEncoder(descriptor))
  }) as NativeGPUDevice["createCommandEncoder"]

  const createBuffer = inner.createBuffer.bind(inner)
  device.createBuffer = ((descriptor: Parameters<NativeGPUDevice["createBuffer"]>[0]) => {
    const buffer = createBuffer(descriptor)
    const mapAsync = buffer.mapAsync.bind(buffer)
    buffer.mapAsync = ((...args: Parameters<typeof mapAsync>) =>
      Promise.resolve(mapAsync(...args))) as typeof buffer.mapAsync
    return buffer
  }) as NativeGPUDevice["createBuffer"]

  const queue = inner.queue as NativeGPUDevice["queue"] & {
    onSubmittedWorkDone?: () => Promise<void>
  }
  const onSubmittedWorkDone = queue.onSubmittedWorkDone?.bind(queue)
  queue.onSubmittedWorkDone = () => Promise.resolve().then(() => onSubmittedWorkDone?.())
  Object.defineProperty(device, "queue", {
    value: queue,
    enumerable: true,
  })

  return device
}

function wrapEncoder(encoder: NativeGPUCommandEncoder) {
  const originalBeginRenderPass = encoder.beginRenderPass.bind(encoder)
  encoder.beginRenderPass = ((descriptor: {
    label?: string
    colorAttachments: Array<{
      view: unknown
      resolveTarget?: unknown
      loadOp: string
      storeOp: string
      clearValue?: { r: number; g: number; b: number; a: number }
    }>
    depthStencilAttachment?: {
      view: unknown
      depthClearValue?: number
      depthLoadOp?: string
      depthStoreOp?: string
    }
  }) => {
    return originalBeginRenderPass(
      {
        label: descriptor.label,
        colorAttachments: descriptor.colorAttachments.map((attachment) => ({
          loadOp: attachment.loadOp,
          storeOp: attachment.storeOp,
          clearValue: attachment.clearValue,
        })),
        depthStencilAttachment: descriptor.depthStencilAttachment
          ? {
              depthClearValue: descriptor.depthStencilAttachment.depthClearValue,
              depthLoadOp: descriptor.depthStencilAttachment.depthLoadOp,
              depthStoreOp: descriptor.depthStencilAttachment.depthStoreOp,
            }
          : undefined,
      },
      descriptor.colorAttachments.map((attachment) => attachment.view) as never,
      descriptor.colorAttachments.map((attachment) => attachment.resolveTarget ?? null) as never,
      descriptor.depthStencilAttachment?.view as never,
    )
  }) as NativeGPUCommandEncoder["beginRenderPass"]
  return encoder
}

export class GPUCanvas {
  #inner: NativeGPUCanvas
  #context: ReturnType<typeof wrapCanvasContext> | null = null
  style: Record<string, string> = {}

  constructor(width = 1, height = 1) {
    this.#inner = new NativeGPUCanvas(width, height)
  }

  get id() {
    return this.#inner.id
  }

  get width() {
    return this.#inner.width
  }

  set width(value: number) {
    this.#inner.width = value
  }

  get height() {
    return this.#inner.height
  }

  set height(value: number) {
    this.#inner.height = value
  }

  get clientWidth() {
    return this.#inner.width
  }

  get clientHeight() {
    return this.#inner.height
  }

  addEventListener(_type: string, _listener?: () => void) {}

  removeEventListener(_type: string, _listener?: () => void) {}

  readPixels() {
    return this.#inner.readPixels()
  }

  destroy() {
    this.#inner.destroy()
  }

  getContext(contextId: string) {
    if (contextId !== "webgpu") {
      throw new Error(`Only getContext("webgpu") is supported, got ${contextId}`)
    }
    this.#context ??= wrapCanvasContext(this.#inner.getContext(contextId))
    return this.#context
  }
}

function wrapCanvasContext(context: NativeGPUCanvasContext) {
  const originalConfigure = context.configure.bind(context)
  context.configure = ((configuration: {
    device: object
    format?: string
    usage?: number
    alphaMode?: string
  }) => {
    originalConfigure(
      {
        format: configuration.format,
        usage: configuration.usage,
        alphaMode: configuration.alphaMode,
      },
      configuration.device as NativeGPUDevice,
    )
  }) as NativeGPUCanvasContext["configure"]
  return context
}

export function createGPUCanvas(width: number, height: number) {
  return new GPUCanvas(width, height)
}

export const gpu = new GPU()

export function installWebGpu(target: { navigator?: { gpu?: GPU } } = globalThis as never) {
  const global = target as {
    navigator?: { gpu?: GPU; userAgent?: string }
    GPUBufferUsage?: typeof GPUBufferUsage
    GPUTextureUsage?: typeof GPUTextureUsage
    GPUShaderStage?: typeof GPUShaderStage
    GPUMapMode?: typeof GPUMapMode
  }
  const navigator = (global.navigator ??= {})
  navigator.gpu = gpu
  navigator.userAgent ??= "gpuix"
  global.GPUBufferUsage = GPUBufferUsage
  global.GPUTextureUsage = GPUTextureUsage
  global.GPUShaderStage = GPUShaderStage
  global.GPUMapMode = GPUMapMode
  return gpu
}
