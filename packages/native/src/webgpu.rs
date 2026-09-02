/// Desktop WebGPU napi over wgpu 29. Class layout follows @sylphx/webgpu.
///
/// The device is a wgpu instance GPUIX owns, not the window device, so
/// Linux `paint_surface` cannot sample these textures. Present is a cached
/// `paint_image` snapshot on every OS until the window device is shared.
use napi::bindgen_prelude::*;
use napi_derive::napi;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static NEXT_CANVAS_ID: AtomicU64 = AtomicU64::new(1);
static SUBMIT_EPOCH: AtomicU64 = AtomicU64::new(0);
static CANVASES: Mutex<Option<HashMap<u64, Arc<Mutex<GpuCanvasInner>>>>> = Mutex::new(None);

fn canvases() -> parking_lot::MutexGuard<'static, Option<HashMap<u64, Arc<Mutex<GpuCanvasInner>>>>> {
    CANVASES.lock()
}

pub(crate) fn canvas_snapshot(id: u64) -> Option<Arc<gpui::RenderImage>> {
    let guard = canvases();
    let inner = guard.as_ref()?.get(&id)?.clone();
    drop(guard);
    let result = inner.lock().snapshot_image();
    match result {
        Ok(image) => Some(image),
        Err(error) => {
            log::warn!("GPUCanvas snapshot failed: {error}");
            None
        }
    }
}

fn register_canvas(id: u64, inner: Arc<Mutex<GpuCanvasInner>>) {
    canvases()
        .get_or_insert_with(HashMap::new)
        .insert(id, inner);
}

fn unregister_canvas(id: u64) {
    if let Some(map) = canvases().as_mut() {
        map.remove(&id);
    }
}

#[napi]
pub struct GPU {
    instance: wgpu::Instance,
}

#[napi]
impl GPU {
    #[napi(factory)]
    pub fn create() -> Self {
        Self {
            instance: wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::PRIMARY,
                flags: wgpu::InstanceFlags::default(),
                backend_options: wgpu::BackendOptions::default(),
                memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
                display: None,
            }),
        }
    }

    #[napi]
    pub fn request_adapter(&self, power_preference: Option<String>) -> Result<GPUAdapter> {
        let power_pref = match power_preference.as_deref() {
            Some("low-power") => wgpu::PowerPreference::LowPower,
            Some("high-performance") => wgpu::PowerPreference::HighPerformance,
            _ => wgpu::PowerPreference::HighPerformance,
        };
        let adapter = pollster::block_on(self.instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: power_pref,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|error| Error::from_reason(format!("No GPU adapter: {error}")))?;
        Ok(GPUAdapter { adapter })
    }

    #[napi(js_name = "getPreferredCanvasFormat")]
    pub fn get_preferred_canvas_format(&self) -> String {
        "bgra8unorm".into()
    }
}

#[napi]
pub fn get_preferred_canvas_format() -> String {
    "bgra8unorm".into()
}

#[napi]
pub struct GPUAdapter {
    adapter: wgpu::Adapter,
}

#[napi]
impl GPUAdapter {
    #[napi]
    pub fn request_device(&self) -> Result<GPUDevice> {
        let (device, queue) = pollster::block_on(self.adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("gpuix_webgpu"),
            required_features: wgpu::Features::empty(),
            required_limits: self.adapter.limits(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        }))
        .map_err(|error| Error::from_reason(format!("Failed to request device: {error}")))?;
        device.on_uncaptured_error(Arc::new(|error| {
            log::error!("wgpu validation error: {error}");
        }));
        Ok(GPUDevice::new(Arc::new(device), Arc::new(queue)))
    }

    #[napi(getter)]
    pub fn limits(&self) -> DeviceLimits {
        DeviceLimits::from_wgpu(&self.adapter.limits())
    }

    #[napi(getter)]
    pub fn info(&self) -> AdapterInfo {
        let info = self.adapter.get_info();
        AdapterInfo {
            vendor: info.vendor.to_string(),
            architecture: String::new(),
            device: info.name,
            description: format!("{:?}", info.backend),
        }
    }

    #[napi(getter)]
    pub fn is_fallback_adapter(&self) -> bool {
        self.adapter.get_info().device_type == wgpu::DeviceType::Cpu
    }
}

#[napi(object)]
pub struct AdapterInfo {
    pub vendor: String,
    pub architecture: String,
    pub device: String,
    pub description: String,
}

#[napi(object)]
pub struct DeviceLimits {
    pub max_texture_dimension_1d: u32,
    pub max_texture_dimension_2d: u32,
    pub max_texture_dimension_3d: u32,
    pub max_bind_groups: u32,
    pub max_buffer_size: i64,
    pub max_uniform_buffer_binding_size: i64,
    pub min_uniform_buffer_offset_alignment: u32,
    pub max_compute_workgroups_per_dimension: u32,
}

impl DeviceLimits {
    fn from_wgpu(limits: &wgpu::Limits) -> Self {
        Self {
            max_texture_dimension_1d: limits.max_texture_dimension_1d,
            max_texture_dimension_2d: limits.max_texture_dimension_2d,
            max_texture_dimension_3d: limits.max_texture_dimension_3d,
            max_bind_groups: limits.max_bind_groups,
            max_buffer_size: limits.max_buffer_size as i64,
            max_uniform_buffer_binding_size: limits.max_uniform_buffer_binding_size as i64,
            min_uniform_buffer_offset_alignment: limits.min_uniform_buffer_offset_alignment,
            max_compute_workgroups_per_dimension: limits.max_compute_workgroups_per_dimension,
        }
    }
}

#[napi]
pub struct GPUDevice {
    pub(crate) device: Arc<wgpu::Device>,
    pub(crate) queue_internal: Arc<wgpu::Queue>,
}

impl GPUDevice {
    fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        Self {
            device,
            queue_internal: queue,
        }
    }
}

#[napi]
impl GPUDevice {
    #[napi(getter)]
    pub fn queue(&self) -> GPUQueue {
        GPUQueue {
            queue: self.queue_internal.clone(),
            device: self.device.clone(),
        }
    }

    #[napi(getter)]
    pub fn label(&self) -> Option<String> {
        None
    }

    #[napi(getter)]
    pub fn limits(&self) -> DeviceLimits {
        DeviceLimits::from_wgpu(&self.device.limits())
    }

    #[napi(js_name = "createBuffer")]
    pub fn create_buffer(&self, descriptor: BufferDescriptor) -> GPUBuffer {
        let mapped_at_creation = descriptor.mapped_at_creation.unwrap_or(false);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: descriptor.label.as_deref(),
            size: descriptor.size as u64,
            usage: wgpu::BufferUsages::from_bits_truncate(descriptor.usage),
            mapped_at_creation,
        });
        GPUBuffer {
            buffer: Arc::new(buffer),
            device: self.device.clone(),
            mapped: Mutex::new(if mapped_at_creation {
                Some(MappedRange {
                    offset: 0,
                    end: descriptor.size as u64,
                    write: true,
                    bytes: Arc::new(Mutex::new(vec![0u8; descriptor.size as usize])),
                })
            } else {
                None
            }),
        }
    }

    #[napi(js_name = "createTexture")]
    pub fn create_texture(&self, descriptor: TextureDescriptor) -> Result<GPUTexture> {
        Ok(GPUTexture::from_wgpu(self.create_wgpu_texture(&descriptor)?))
    }

    #[napi(js_name = "createSampler")]
    pub fn create_sampler(&self, descriptor: Option<SamplerDescriptor>) -> GPUSampler {
        let descriptor = descriptor.unwrap_or_default();
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: descriptor.label.as_deref(),
            address_mode_u: parse_address_mode(descriptor.address_mode_u.as_deref()),
            address_mode_v: parse_address_mode(descriptor.address_mode_v.as_deref()),
            address_mode_w: parse_address_mode(descriptor.address_mode_w.as_deref()),
            mag_filter: parse_filter_mode(descriptor.mag_filter.as_deref()),
            min_filter: parse_filter_mode(descriptor.min_filter.as_deref()),
            mipmap_filter: parse_mipmap_filter_mode(descriptor.mipmap_filter.as_deref()),
            lod_min_clamp: descriptor.lod_min_clamp.unwrap_or(0.0) as f32,
            lod_max_clamp: descriptor.lod_max_clamp.unwrap_or(32.0) as f32,
            compare: parse_compare_function(descriptor.compare.as_deref()),
            anisotropy_clamp: descriptor.max_anisotropy.unwrap_or(1),
            border_color: None,
        });
        GPUSampler {
            sampler: Arc::new(sampler),
        }
    }

    #[napi(js_name = "createShaderModule")]
    pub fn create_shader_module(&self, descriptor: ShaderModuleDescriptor) -> GPUShaderModule {
        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: descriptor.label.as_deref(),
            source: wgpu::ShaderSource::Wgsl(descriptor.code.into()),
        });
        GPUShaderModule {
            shader: Arc::new(shader),
        }
    }

    #[napi(js_name = "createBindGroupLayout")]
    pub fn create_bind_group_layout(
        &self,
        descriptor: BindGroupLayoutDescriptor,
    ) -> Result<GPUBindGroupLayout> {
        let entries: Result<Vec<_>> = descriptor
            .entries
            .iter()
            .map(convert_bind_group_layout_entry)
            .collect();
        let entries = entries?;
        let layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: descriptor.label.as_deref(),
            entries: &entries,
        });
        Ok(GPUBindGroupLayout {
            layout: Arc::new(layout),
        })
    }

    #[napi(js_name = "createPipelineLayout")]
    pub fn create_pipeline_layout(
        &self,
        descriptor: PipelineLayoutDescriptor,
        bind_group_layouts: Vec<&GPUBindGroupLayout>,
    ) -> GPUPipelineLayout {
        let layouts: Vec<_> = bind_group_layouts
            .iter()
            .map(|layout| Some(layout.layout.as_ref()))
            .collect();
        let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: descriptor.label.as_deref(),
            bind_group_layouts: &layouts,
            immediate_size: 0,
        });
        GPUPipelineLayout {
            layout: Arc::new(layout),
        }
    }

    #[napi(js_name = "createBindGroup")]
    pub fn create_bind_group(
        &self,
        descriptor: BindGroupDescriptor,
        layout: &GPUBindGroupLayout,
        entries: Vec<BindGroupEntry>,
        buffers: Option<Vec<&GPUBuffer>>,
        textures: Option<Vec<&GPUTextureView>>,
        samplers: Option<Vec<&GPUSampler>>,
    ) -> Result<GPUBindGroup> {
        let mut buffer_index = 0;
        let mut texture_index = 0;
        let mut sampler_index = 0;
        let wgpu_entries: Result<Vec<_>> = entries
            .iter()
            .map(|entry| {
                let resource = match entry.resource_type.as_str() {
                    "buffer" => {
                        let buffers = buffers
                            .as_ref()
                            .ok_or_else(|| Error::from_reason("No buffers for bind group"))?;
                        let buffer = buffers
                            .get(buffer_index)
                            .ok_or_else(|| Error::from_reason("Not enough buffers"))?;
                        buffer_index += 1;
                        wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &buffer.buffer,
                            offset: entry.offset.unwrap_or(0) as u64,
                            size: entry
                                .size
                                .and_then(|size| std::num::NonZeroU64::new(size as u64)),
                        })
                    }
                    "texture" => {
                        let textures = textures
                            .as_ref()
                            .ok_or_else(|| Error::from_reason("No textures for bind group"))?;
                        let texture = textures
                            .get(texture_index)
                            .ok_or_else(|| Error::from_reason("Not enough textures"))?;
                        texture_index += 1;
                        wgpu::BindingResource::TextureView(&texture.view)
                    }
                    "sampler" => {
                        let samplers = samplers
                            .as_ref()
                            .ok_or_else(|| Error::from_reason("No samplers for bind group"))?;
                        let sampler = samplers
                            .get(sampler_index)
                            .ok_or_else(|| Error::from_reason("Not enough samplers"))?;
                        sampler_index += 1;
                        wgpu::BindingResource::Sampler(&sampler.sampler)
                    }
                    other => {
                        return Err(Error::from_reason(format!("Invalid resource_type: {other}")));
                    }
                };
                Ok(wgpu::BindGroupEntry {
                    binding: entry.binding,
                    resource,
                })
            })
            .collect();
        let wgpu_entries = wgpu_entries?;
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: descriptor.label.as_deref(),
            layout: &layout.layout,
            entries: &wgpu_entries,
        });
        Ok(GPUBindGroup {
            bind_group: Arc::new(bind_group),
        })
    }

    #[napi(js_name = "createRenderPipeline")]
    pub fn create_render_pipeline(
        &self,
        descriptor: RenderPipelineDescriptor,
        layout: Option<&GPUPipelineLayout>,
        vertex_module: &GPUShaderModule,
        fragment_module: Option<&GPUShaderModule>,
    ) -> Result<GPURenderPipeline> {
        let vertex_attributes: Vec<Vec<wgpu::VertexAttribute>> = match &descriptor.vertex.buffers {
            Some(buffers) => buffers
                .iter()
                .map(|buffer| {
                    buffer
                        .attributes
                        .iter()
                        .map(|attribute| {
                            Ok(wgpu::VertexAttribute {
                                format: parse_vertex_format(&attribute.format)?,
                                offset: attribute.offset as u64,
                                shader_location: attribute.shader_location,
                            })
                        })
                        .collect::<Result<Vec<_>>>()
                })
                .collect::<Result<Vec<_>>>()?,
            None => Vec::new(),
        };
        let vertex_buffers: Vec<wgpu::VertexBufferLayout> = descriptor
            .vertex
            .buffers
            .as_ref()
            .map(|buffers| {
                buffers
                    .iter()
                    .enumerate()
                    .map(|(index, buffer)| wgpu::VertexBufferLayout {
                        array_stride: buffer.array_stride as u64,
                        step_mode: if buffer.step_mode.as_deref() == Some("instance") {
                            wgpu::VertexStepMode::Instance
                        } else {
                            wgpu::VertexStepMode::Vertex
                        },
                        attributes: &vertex_attributes[index],
                    })
                    .collect()
            })
            .unwrap_or_default();
        let primitive = descriptor
            .primitive
            .as_ref()
            .map(parse_primitive)
            .unwrap_or_default();
        let depth_stencil = descriptor
            .depth_stencil
            .as_ref()
            .map(parse_depth_stencil)
            .transpose()?;
        let frag_targets: Vec<Option<wgpu::ColorTargetState>> = match descriptor.fragment.as_ref() {
            Some(fragment) => fragment
                .targets
                .iter()
                .map(|target| {
                    Ok(Some(wgpu::ColorTargetState {
                        format: parse_texture_format(&target.format)?,
                        blend: target.blend.as_ref().map(parse_blend),
                        write_mask: target
                            .write_mask
                            .map(|mask| {
                                wgpu::ColorWrites::from_bits(mask).unwrap_or(wgpu::ColorWrites::ALL)
                            })
                            .unwrap_or(wgpu::ColorWrites::ALL),
                    }))
                })
                .collect::<Result<Vec<_>>>()?,
            None => Vec::new(),
        };
        let fragment = if let (Some(fragment), Some(module)) =
            (descriptor.fragment.as_ref(), fragment_module)
        {
            Some(wgpu::FragmentState {
                module: &module.shader,
                entry_point: Some(&fragment.entry_point),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &frag_targets,
            })
        } else {
            None
        };
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: descriptor.label.as_deref(),
            layout: layout.map(|layout| layout.layout.as_ref()),
            vertex: wgpu::VertexState {
                module: &vertex_module.shader,
                entry_point: Some(&descriptor.vertex.entry_point),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &vertex_buffers,
            },
            fragment,
            primitive,
            depth_stencil,
            multisample: wgpu::MultisampleState {
                count: descriptor
                    .multisample
                    .as_ref()
                    .and_then(|state| state.count)
                    .unwrap_or(1),
                mask: descriptor
                    .multisample
                    .as_ref()
                    .and_then(|state| state.mask)
                    .map(u64::from)
                    .unwrap_or(!0),
                alpha_to_coverage_enabled: descriptor
                    .multisample
                    .as_ref()
                    .and_then(|state| state.alpha_to_coverage_enabled)
                    .unwrap_or(false),
            },
            multiview_mask: None,
            cache: None,
        });
        Ok(GPURenderPipeline {
            pipeline: Arc::new(pipeline),
        })
    }

    #[napi(js_name = "createComputePipeline")]
    pub fn create_compute_pipeline(
        &self,
        descriptor: ComputePipelineDescriptor,
        layout: Option<&GPUPipelineLayout>,
        module: &GPUShaderModule,
    ) -> GPUComputePipeline {
        let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: descriptor.label.as_deref(),
            layout: layout.map(|layout| layout.layout.as_ref()),
            module: &module.shader,
            entry_point: Some(&descriptor.entry_point),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        GPUComputePipeline {
            pipeline: Arc::new(pipeline),
        }
    }

    #[napi(js_name = "createCommandEncoder")]
    pub fn create_command_encoder(
        &self,
        descriptor: Option<CommandEncoderDescriptor>,
    ) -> GPUCommandEncoder {
        let encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: descriptor.as_ref().and_then(|descriptor| descriptor.label.as_deref()),
        });
        GPUCommandEncoder {
            encoder: Some(encoder),
            device: self.device.clone(),
        }
    }

    #[napi]
    pub fn destroy(&self) {
        // wgpu drops the device when the last Arc is released.
    }

    fn create_wgpu_texture(&self, descriptor: &TextureDescriptor) -> Result<wgpu::Texture> {
        let dimension = match descriptor.dimension.as_deref() {
            Some("1d") => wgpu::TextureDimension::D1,
            Some("3d") => wgpu::TextureDimension::D3,
            _ => wgpu::TextureDimension::D2,
        };
        Ok(self.device.create_texture(&wgpu::TextureDescriptor {
            label: descriptor.label.as_deref(),
            size: wgpu::Extent3d {
                width: descriptor.width.max(1),
                height: descriptor.height.max(1),
                depth_or_array_layers: descriptor.depth.unwrap_or(1).max(1),
            },
            mip_level_count: descriptor.mip_level_count.unwrap_or(1),
            sample_count: descriptor.sample_count.unwrap_or(1),
            dimension,
            format: parse_texture_format(&descriptor.format)?,
            usage: wgpu::TextureUsages::from_bits_truncate(descriptor.usage),
            view_formats: &[],
        }))
    }
}

#[napi]
pub struct GPUQueue {
    queue: Arc<wgpu::Queue>,
    device: Arc<wgpu::Device>,
}

#[napi]
impl GPUQueue {
    #[napi]
    pub fn submit(&self, command_buffers: Vec<&mut GPUCommandBuffer>) {
        let buffers: Vec<wgpu::CommandBuffer> = command_buffers
            .into_iter()
            .filter_map(|buffer| buffer.buffer.take())
            .collect();
        self.queue.submit(buffers);
        SUBMIT_EPOCH.fetch_add(1, Ordering::Release);
    }

    #[napi(js_name = "onSubmittedWorkDone")]
    pub fn on_submitted_work_done(&self) -> Result<()> {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|error| {
                Error::from_reason(format!("onSubmittedWorkDone poll failed: {error:?}"))
            })?;
        Ok(())
    }

    #[napi(js_name = "writeBuffer")]
    pub fn write_buffer(
        &self,
        buffer: &GPUBuffer,
        offset: i64,
        data: Buffer,
        data_offset: Option<i64>,
        size: Option<i64>,
    ) {
        let bytes = data.as_ref();
        let start = data_offset.unwrap_or(0).max(0) as usize;
        let end = size
            .map(|size| start.saturating_add(size.max(0) as usize))
            .unwrap_or(bytes.len())
            .min(bytes.len());
        let slice = if start >= bytes.len() {
            &[]
        } else {
            &bytes[start..end]
        };
        self.queue
            .write_buffer(&buffer.buffer, offset as u64, slice);
    }

    #[napi(getter)]
    pub fn label(&self) -> Option<String> {
        None
    }
}

struct MappedRange {
    offset: u64,
    end: u64,
    write: bool,
    bytes: Arc<Mutex<Vec<u8>>>,
}

#[napi]
pub struct GPUBuffer {
    buffer: Arc<wgpu::Buffer>,
    device: Arc<wgpu::Device>,
    mapped: Mutex<Option<MappedRange>>,
}

#[napi]
impl GPUBuffer {
    #[napi(getter)]
    pub fn size(&self) -> f64 {
        self.buffer.size() as f64
    }

    #[napi(getter)]
    pub fn usage(&self) -> u32 {
        self.buffer.usage().bits()
    }

    #[napi(js_name = "mapAsync")]
    pub fn map_async(&self, mode: u32, offset: Option<f64>, size: Option<f64>) -> Result<()> {
        let offset = offset.unwrap_or(0.0) as u64;
        let end = size
            .map(|size| offset + size as u64)
            .unwrap_or(self.buffer.size());
        let mode = if mode & 0x0002 != 0 {
            wgpu::MapMode::Write
        } else {
            wgpu::MapMode::Read
        };
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        self.buffer.slice(offset..end).map_async(mode, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|error| Error::from_reason(format!("mapAsync poll failed: {error:?}")))?;
        receiver
            .recv()
            .map_err(|_| Error::from_reason("mapAsync channel closed"))?
            .map_err(|error| Error::from_reason(format!("mapAsync failed: {error}")))?;
        let bytes = if mode == wgpu::MapMode::Read {
            self.buffer
                .slice(offset..end)
                .get_mapped_range()
                .to_vec()
        } else {
            vec![0u8; (end - offset) as usize]
        };
        *self.mapped.lock() = Some(MappedRange {
            offset,
            end,
            write: mode == wgpu::MapMode::Write,
            bytes: Arc::new(Mutex::new(bytes)),
        });
        Ok(())
    }

    #[napi(js_name = "getMappedRange")]
    pub fn get_mapped_range(
        &self,
        env: Env,
        offset: Option<f64>,
        size: Option<f64>,
    ) -> Result<ArrayBuffer<'_>> {
        let mapped = self.mapped.lock();
        let range = mapped
            .as_ref()
            .ok_or_else(|| Error::from_reason("Buffer is not mapped"))?;
        if offset.unwrap_or(0.0) != 0.0 || size.is_some() {
            return Err(Error::from_reason(
                "getMappedRange offset/size slices are not supported yet",
            ));
        }
        let keep_alive = range.bytes.clone();
        let mut bytes = keep_alive.lock();
        let (data, len) = (bytes.as_mut_ptr(), bytes.len());
        drop(bytes);
        unsafe { ArrayBuffer::from_external(&env, data, len, keep_alive, |_, _keep| {}) }
    }

    #[napi]
    pub fn unmap(&self) {
        if let Some(range) = self.mapped.lock().take() {
            if range.write {
                let bytes = range.bytes.lock();
                if !bytes.is_empty() {
                    self.buffer
                        .slice(range.offset..range.end)
                        .get_mapped_range_mut()
                        .copy_from_slice(&bytes);
                }
            }
            self.buffer.unmap();
        }
    }

    #[napi]
    pub fn destroy(&self) {
        self.buffer.destroy();
    }
}

#[napi]
pub struct GPUTexture {
    texture: Arc<wgpu::Texture>,
}

impl GPUTexture {
    fn from_wgpu(texture: wgpu::Texture) -> Self {
        Self {
            texture: Arc::new(texture),
        }
    }
}

#[napi]
impl GPUTexture {
    #[napi(js_name = "createView")]
    pub fn create_view(&self, descriptor: Option<TextureViewDescriptor>) -> Result<GPUTextureView> {
        let descriptor = descriptor.unwrap_or_default();
        let format = descriptor
            .format
            .as_deref()
            .map(parse_texture_format)
            .transpose()?;
        let view = self.texture.create_view(&wgpu::TextureViewDescriptor {
            label: descriptor.label.as_deref(),
            format,
            dimension: parse_view_dimension(descriptor.dimension.as_deref()),
            aspect: match descriptor.aspect.as_deref() {
                Some("depth-only") => wgpu::TextureAspect::DepthOnly,
                Some("stencil-only") => wgpu::TextureAspect::StencilOnly,
                _ => wgpu::TextureAspect::All,
            },
            base_mip_level: descriptor.base_mip_level.unwrap_or(0),
            mip_level_count: descriptor.mip_level_count,
            base_array_layer: descriptor.base_array_layer.unwrap_or(0),
            array_layer_count: descriptor.array_layer_count,
            usage: None,
        });
        Ok(GPUTextureView {
            view: Arc::new(view),
        })
    }

    #[napi(getter)]
    pub fn width(&self) -> u32 {
        self.texture.width()
    }

    #[napi(getter)]
    pub fn height(&self) -> u32 {
        self.texture.height()
    }

    #[napi]
    pub fn destroy(&self) {
        self.texture.destroy();
    }
}

#[napi(object)]
#[derive(Default)]
pub struct TextureViewDescriptor {
    pub label: Option<String>,
    pub format: Option<String>,
    pub dimension: Option<String>,
    pub aspect: Option<String>,
    pub base_mip_level: Option<u32>,
    pub mip_level_count: Option<u32>,
    pub base_array_layer: Option<u32>,
    pub array_layer_count: Option<u32>,
}

#[napi]
pub struct GPUTextureView {
    view: Arc<wgpu::TextureView>,
}

#[napi]
pub struct GPUSampler {
    sampler: Arc<wgpu::Sampler>,
}

#[napi]
pub struct GPUShaderModule {
    shader: Arc<wgpu::ShaderModule>,
}

#[napi]
pub struct GPUBindGroupLayout {
    layout: Arc<wgpu::BindGroupLayout>,
}

#[napi]
pub struct GPUPipelineLayout {
    layout: Arc<wgpu::PipelineLayout>,
}

#[napi]
pub struct GPUBindGroup {
    bind_group: Arc<wgpu::BindGroup>,
}

#[napi]
pub struct GPURenderPipeline {
    pipeline: Arc<wgpu::RenderPipeline>,
}

#[napi]
impl GPURenderPipeline {
    #[napi(js_name = "getBindGroupLayout")]
    pub fn get_bind_group_layout(&self, index: u32) -> GPUBindGroupLayout {
        GPUBindGroupLayout {
            layout: Arc::new(self.pipeline.get_bind_group_layout(index)),
        }
    }
}

#[napi]
pub struct GPUComputePipeline {
    pipeline: Arc<wgpu::ComputePipeline>,
}

#[napi]
impl GPUComputePipeline {
    #[napi(js_name = "getBindGroupLayout")]
    pub fn get_bind_group_layout(&self, index: u32) -> GPUBindGroupLayout {
        GPUBindGroupLayout {
            layout: Arc::new(self.pipeline.get_bind_group_layout(index)),
        }
    }
}

#[napi]
pub struct GPUCommandEncoder {
    encoder: Option<wgpu::CommandEncoder>,
    #[allow(dead_code)]
    device: Arc<wgpu::Device>,
}

#[napi]
impl GPUCommandEncoder {
    #[napi(js_name = "beginRenderPass")]
    pub fn begin_render_pass(
        &mut self,
        descriptor: RenderPassDescriptor,
        color_views: Vec<&GPUTextureView>,
        color_resolve_views: Option<Vec<Option<&GPUTextureView>>>,
        depth_stencil_view: Option<&GPUTextureView>,
    ) -> Result<GPURenderPassEncoder> {
        if color_resolve_views
            .as_ref()
            .is_some_and(|views| views.iter().any(Option::is_some))
        {
            return Err(Error::from_reason(
                "MSAA resolveTarget is not supported yet. Use antialias: false.",
            ));
        }
        let encoder = self
            .encoder
            .as_mut()
            .ok_or_else(|| Error::from_reason("Command encoder already finished"))?;
        if color_views.len() != descriptor.color_attachments.len() {
            return Err(Error::from_reason(
                "colorViews length must match colorAttachments",
            ));
        }
        let color_attachments: Vec<Option<wgpu::RenderPassColorAttachment>> = descriptor
            .color_attachments
            .iter()
            .enumerate()
            .map(|(index, attachment)| {
                let view = &color_views[index];
                let load = match attachment.load_op.as_str() {
                    "clear" => wgpu::LoadOp::Clear(
                        attachment
                            .clear_value
                            .as_ref()
                            .map(|color| wgpu::Color {
                                r: color.r,
                                g: color.g,
                                b: color.b,
                                a: color.a,
                            })
                            .unwrap_or(wgpu::Color::BLACK),
                    ),
                    _ => wgpu::LoadOp::Load,
                };
                let store = if attachment.store_op == "discard" {
                    wgpu::StoreOp::Discard
                } else {
                    wgpu::StoreOp::Store
                };
                Some(wgpu::RenderPassColorAttachment {
                    view: &view.view,
                    resolve_target: None,
                    ops: wgpu::Operations { load, store },
                    depth_slice: None,
                })
            })
            .collect();
        let depth_stencil_attachment = descriptor.depth_stencil_attachment.as_ref().and_then(|attachment| {
            let view = depth_stencil_view?;
            Some(wgpu::RenderPassDepthStencilAttachment {
                view: &view.view,
                depth_ops: Some(wgpu::Operations {
                    load: if attachment.depth_load_op.as_deref() == Some("clear") {
                        wgpu::LoadOp::Clear(attachment.depth_clear_value.unwrap_or(1.0) as f32)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: if attachment.depth_store_op.as_deref() == Some("discard") {
                        wgpu::StoreOp::Discard
                    } else {
                        wgpu::StoreOp::Store
                    },
                }),
                stencil_ops: None,
            })
        });
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: descriptor.label.as_deref(),
            color_attachments: &color_attachments,
            depth_stencil_attachment,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        Ok(GPURenderPassEncoder {
            pass: Some(pass.forget_lifetime()),
        })
    }

    #[napi(js_name = "beginComputePass")]
    pub fn begin_compute_pass(
        &mut self,
        descriptor: Option<ComputePassDescriptor>,
    ) -> Result<GPUComputePassEncoder> {
        let encoder = self
            .encoder
            .as_mut()
            .ok_or_else(|| Error::from_reason("Command encoder already finished"))?;
        let pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: descriptor.as_ref().and_then(|descriptor| descriptor.label.as_deref()),
            timestamp_writes: None,
        });
        Ok(GPUComputePassEncoder {
            pass: Some(pass.forget_lifetime()),
        })
    }

    #[napi(js_name = "copyBufferToBuffer")]
    pub fn copy_buffer_to_buffer(
        &mut self,
        source: &GPUBuffer,
        source_offset: i64,
        destination: &GPUBuffer,
        destination_offset: i64,
        size: i64,
    ) -> Result<()> {
        let encoder = self
            .encoder
            .as_mut()
            .ok_or_else(|| Error::from_reason("Command encoder already finished"))?;
        encoder.copy_buffer_to_buffer(
            &source.buffer,
            source_offset as u64,
            &destination.buffer,
            destination_offset as u64,
            size as u64,
        );
        Ok(())
    }

    #[napi]
    pub fn finish(&mut self) -> Result<GPUCommandBuffer> {
        let encoder = self
            .encoder
            .take()
            .ok_or_else(|| Error::from_reason("Command encoder already finished"))?;
        Ok(GPUCommandBuffer {
            buffer: Some(encoder.finish()),
        })
    }
}

#[napi]
pub struct GPUCommandBuffer {
    buffer: Option<wgpu::CommandBuffer>,
}

#[napi]
pub struct GPURenderPassEncoder {
    pass: Option<wgpu::RenderPass<'static>>,
}

#[napi]
impl GPURenderPassEncoder {
    #[napi(js_name = "setPipeline")]
    pub fn set_pipeline(&mut self, pipeline: &GPURenderPipeline) -> Result<()> {
        self.pass_mut()?.set_pipeline(&pipeline.pipeline);
        Ok(())
    }

    #[napi(js_name = "setBindGroup")]
    pub fn set_bind_group(
        &mut self,
        index: u32,
        bind_group: &GPUBindGroup,
        dynamic_offsets: Option<Vec<u32>>,
    ) -> Result<()> {
        let offsets = dynamic_offsets.unwrap_or_default();
        self.pass_mut()?
            .set_bind_group(index, bind_group.bind_group.as_ref(), &offsets);
        Ok(())
    }

    #[napi(js_name = "setVertexBuffer")]
    pub fn set_vertex_buffer(
        &mut self,
        slot: u32,
        buffer: &GPUBuffer,
        offset: Option<f64>,
        size: Option<f64>,
    ) -> Result<()> {
        let slice = buffer_slice(&buffer.buffer, offset, size);
        self.pass_mut()?.set_vertex_buffer(slot, slice);
        Ok(())
    }

    #[napi(js_name = "setIndexBuffer")]
    pub fn set_index_buffer(
        &mut self,
        buffer: &GPUBuffer,
        index_format: String,
        offset: Option<f64>,
        size: Option<f64>,
    ) -> Result<()> {
        let format = match index_format.as_str() {
            "uint16" => wgpu::IndexFormat::Uint16,
            _ => wgpu::IndexFormat::Uint32,
        };
        let slice = buffer_slice(&buffer.buffer, offset, size);
        self.pass_mut()?.set_index_buffer(slice, format);
        Ok(())
    }

    #[napi]
    pub fn draw(
        &mut self,
        vertex_count: u32,
        instance_count: Option<u32>,
        first_vertex: Option<u32>,
        first_instance: Option<u32>,
    ) -> Result<()> {
        let first_vertex = first_vertex.unwrap_or(0);
        let first_instance = first_instance.unwrap_or(0);
        self.pass_mut()?.draw(
            first_vertex..first_vertex + vertex_count,
            first_instance..first_instance + instance_count.unwrap_or(1),
        );
        Ok(())
    }

    #[napi(js_name = "drawIndexed")]
    pub fn draw_indexed(
        &mut self,
        index_count: u32,
        instance_count: Option<u32>,
        first_index: Option<u32>,
        base_vertex: Option<i32>,
        first_instance: Option<u32>,
    ) -> Result<()> {
        let first_index = first_index.unwrap_or(0);
        let first_instance = first_instance.unwrap_or(0);
        self.pass_mut()?.draw_indexed(
            first_index..first_index + index_count,
            base_vertex.unwrap_or(0),
            first_instance..first_instance + instance_count.unwrap_or(1),
        );
        Ok(())
    }

    #[napi(js_name = "setViewport")]
    pub fn set_viewport(
        &mut self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        min_depth: Option<f64>,
        max_depth: Option<f64>,
    ) -> Result<()> {
        self.pass_mut()?.set_viewport(
            x as f32,
            y as f32,
            width as f32,
            height as f32,
            min_depth.unwrap_or(0.0) as f32,
            max_depth.unwrap_or(1.0) as f32,
        );
        Ok(())
    }

    #[napi(js_name = "setScissorRect")]
    pub fn set_scissor_rect(&mut self, x: u32, y: u32, width: u32, height: u32) -> Result<()> {
        self.pass_mut()?.set_scissor_rect(x, y, width, height);
        Ok(())
    }

    #[napi]
    pub fn end(&mut self) {
        self.drop_pass();
    }

    fn pass_mut(&mut self) -> Result<&mut wgpu::RenderPass<'static>> {
        self.pass
            .as_mut()
            .ok_or_else(|| Error::from_reason("Render pass already ended"))
    }

    fn drop_pass(&mut self) {
        self.pass.take();
    }
}

#[napi]
pub struct GPUComputePassEncoder {
    pass: Option<wgpu::ComputePass<'static>>,
}

#[napi]
impl GPUComputePassEncoder {
    #[napi(js_name = "setPipeline")]
    pub fn set_pipeline(&mut self, pipeline: &GPUComputePipeline) -> Result<()> {
        self.pass_mut()?.set_pipeline(&pipeline.pipeline);
        Ok(())
    }

    #[napi(js_name = "setBindGroup")]
    pub fn set_bind_group(
        &mut self,
        index: u32,
        bind_group: &GPUBindGroup,
        dynamic_offsets: Option<Vec<u32>>,
    ) -> Result<()> {
        let offsets = dynamic_offsets.unwrap_or_default();
        self.pass_mut()?
            .set_bind_group(index, bind_group.bind_group.as_ref(), &offsets);
        Ok(())
    }

    #[napi(js_name = "dispatchWorkgroups")]
    pub fn dispatch_workgroups(
        &mut self,
        x: u32,
        y: Option<u32>,
        z: Option<u32>,
    ) -> Result<()> {
        self.pass_mut()?
            .dispatch_workgroups(x, y.unwrap_or(1), z.unwrap_or(1));
        Ok(())
    }

    #[napi]
    pub fn end(&mut self) {
        self.drop_pass();
    }

    fn pass_mut(&mut self) -> Result<&mut wgpu::ComputePass<'static>> {
        self.pass
            .as_mut()
            .ok_or_else(|| Error::from_reason("Compute pass already ended"))
    }

    fn drop_pass(&mut self) {
        self.pass.take();
    }
}

struct GpuCanvasInner {
    #[allow(dead_code)]
    id: u64,
    width: u32,
    height: u32,
    device: Option<Arc<wgpu::Device>>,
    queue: Option<Arc<wgpu::Queue>>,
    format: wgpu::TextureFormat,
    texture: Option<Arc<wgpu::Texture>>,
    cached_image: Option<Arc<gpui::RenderImage>>,
    snapshot_generation: u64,
}

impl GpuCanvasInner {
    fn configure(&mut self, device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>, format: wgpu::TextureFormat) {
        self.device = Some(device.clone());
        self.queue = Some(queue);
        self.format = format;
        self.rebuild_textures(&device);
    }

    fn set_size(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        if let Some(device) = self.device.clone() {
            self.rebuild_textures(&device);
        }
    }

    fn rebuild_textures(&mut self, device: &wgpu::Device) {
        let width = self.width.max(1);
        let height = self.height.max(1);
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING;
        self.texture = Some(Arc::new(device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpuix_canvas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage,
            view_formats: &[],
        })));
        self.cached_image = None;
        self.snapshot_generation = 0;
    }

    fn current_texture(&self) -> Option<Arc<wgpu::Texture>> {
        self.texture.clone()
    }

    fn snapshot_image(&mut self) -> Result<Arc<gpui::RenderImage>> {
        let epoch = SUBMIT_EPOCH.load(Ordering::Acquire);
        if self.snapshot_generation == epoch {
            if let Some(image) = &self.cached_image {
                return Ok(image.clone());
            }
        }
        let device = self
            .device
            .clone()
            .ok_or_else(|| Error::from_reason("GPUCanvas is not configured"))?;
        let queue = self
            .queue
            .clone()
            .ok_or_else(|| Error::from_reason("GPUCanvas is not configured"))?;
        let texture = self
            .current_texture()
            .ok_or_else(|| Error::from_reason("GPUCanvas has no texture"))?;
        let width = texture.width();
        let height = texture.height();
        let bytes_per_row = (width as usize * 4).next_multiple_of(256);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpuix_canvas_readback"),
            size: bytes_per_row as u64 * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gpuix_canvas_copy"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row as u32),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| ());
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|error| Error::from_reason(format!("GPU readback poll failed: {error:?}")))?;
        let mapped = buffer.slice(..).get_mapped_range();
        let mut pixels = image::RgbaImage::new(width, height);
        let row_bytes = width as usize * 4;
        let swap_to_bgra = !matches!(
            self.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );
        for y in 0..height as usize {
            let src = &mapped[y * bytes_per_row..][..row_bytes];
            let dest = &mut pixels.as_mut()[y * row_bytes..][..row_bytes];
            dest.copy_from_slice(src);
            if swap_to_bgra {
                for pixel in dest.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
            }
        }
        drop(mapped);
        buffer.unmap();
        let image = Arc::new(gpui::RenderImage::new(vec![image::Frame::new(pixels)]));
        self.cached_image = Some(image.clone());
        self.snapshot_generation = epoch;
        Ok(image)
    }

    fn read_pixels(&mut self) -> Result<Vec<u8>> {
        let image = self.snapshot_image()?;
        let bytes = image
            .as_bytes(0)
            .ok_or_else(|| Error::from_reason("GPUCanvas snapshot has no pixels"))?;
        let mut rgba = bytes.to_vec();
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        Ok(rgba)
    }
}

#[napi]
pub struct GPUCanvas {
    id: u64,
    inner: Arc<Mutex<GpuCanvasInner>>,
}

#[napi]
impl GPUCanvas {
    #[napi(constructor)]
    pub fn new(width: u32, height: u32) -> Self {
        let id = NEXT_CANVAS_ID.fetch_add(1, Ordering::Relaxed);
        let inner = Arc::new(Mutex::new(GpuCanvasInner {
            id,
            width: width.max(1),
            height: height.max(1),
            device: None,
            queue: None,
            format: wgpu::TextureFormat::Bgra8Unorm,
            texture: None,
            cached_image: None,
            snapshot_generation: 0,
        }));
        register_canvas(id, inner.clone());
        Self { id, inner }
    }

    #[napi(getter)]
    pub fn id(&self) -> f64 {
        self.id as f64
    }

    #[napi(getter)]
    pub fn width(&self) -> u32 {
        self.inner.lock().width
    }

    #[napi(setter)]
    pub fn set_width(&self, width: u32) {
        let mut inner = self.inner.lock();
        let height = inner.height;
        inner.set_size(width, height);
    }

    #[napi(getter)]
    pub fn height(&self) -> u32 {
        self.inner.lock().height
    }

    #[napi(setter)]
    pub fn set_height(&self, height: u32) {
        let mut inner = self.inner.lock();
        let width = inner.width;
        inner.set_size(width, height);
    }

    #[napi(js_name = "readPixels")]
    pub fn read_pixels(&self) -> Result<Buffer> {
        Ok(Buffer::from(self.inner.lock().read_pixels()?))
    }

    #[napi]
    pub fn destroy(&self) {
        unregister_canvas(self.id);
        let mut inner = self.inner.lock();
        inner.texture = None;
        inner.cached_image = None;
        inner.device = None;
        inner.queue = None;
    }

    #[napi(js_name = "getContext")]
    pub fn get_context(&self, context_id: String) -> Result<GPUCanvasContext> {
        if context_id != "webgpu" {
            return Err(Error::from_reason(format!(
                "Only getContext(\"webgpu\") is supported, got {context_id}"
            )));
        }
        Ok(GPUCanvasContext {
            inner: self.inner.clone(),
        })
    }
}

impl Drop for GPUCanvas {
    fn drop(&mut self) {
        unregister_canvas(self.id);
    }
}

#[napi]
pub struct GPUCanvasContext {
    inner: Arc<Mutex<GpuCanvasInner>>,
}

#[napi]
impl GPUCanvasContext {
    #[napi]
    pub fn configure(&self, configuration: GPUCanvasConfiguration, device: &GPUDevice) -> Result<()> {
        let format = parse_texture_format(configuration.format.as_deref().unwrap_or("bgra8unorm"))?;
        if !matches!(
            format,
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
                | wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            return Err(Error::from_reason(format!(
                "GPUCanvas only supports 8-bit RGBA/BGRA formats, got {format:?}"
            )));
        }
        self.inner.lock().configure(
            device.device.clone(),
            device.queue_internal.clone(),
            format,
        );
        Ok(())
    }

    #[napi(js_name = "getCurrentTexture")]
    pub fn get_current_texture(&self) -> Result<GPUTexture> {
        let texture = self
            .inner
            .lock()
            .current_texture()
            .ok_or_else(|| Error::from_reason("GPUCanvas is not configured"))?;
        Ok(GPUTexture { texture })
    }

    #[napi]
    pub fn unconfigure(&self) {
        let mut inner = self.inner.lock();
        inner.texture = None;
        inner.cached_image = None;
        inner.device = None;
        inner.queue = None;
    }
}

#[napi(object)]
pub struct GPUCanvasConfiguration {
    pub format: Option<String>,
    pub usage: Option<u32>,
    pub alpha_mode: Option<String>,
}

#[napi]
pub fn gpu_buffer_usage() -> BufferUsage {
    BufferUsage {
        map_read: wgpu::BufferUsages::MAP_READ.bits(),
        map_write: wgpu::BufferUsages::MAP_WRITE.bits(),
        copy_src: wgpu::BufferUsages::COPY_SRC.bits(),
        copy_dst: wgpu::BufferUsages::COPY_DST.bits(),
        index: wgpu::BufferUsages::INDEX.bits(),
        vertex: wgpu::BufferUsages::VERTEX.bits(),
        uniform: wgpu::BufferUsages::UNIFORM.bits(),
        storage: wgpu::BufferUsages::STORAGE.bits(),
        indirect: wgpu::BufferUsages::INDIRECT.bits(),
        query_resolve: wgpu::BufferUsages::QUERY_RESOLVE.bits(),
    }
}

#[napi]
pub fn gpu_texture_usage() -> TextureUsage {
    TextureUsage {
        copy_src: wgpu::TextureUsages::COPY_SRC.bits(),
        copy_dst: wgpu::TextureUsages::COPY_DST.bits(),
        texture_binding: wgpu::TextureUsages::TEXTURE_BINDING.bits(),
        storage_binding: wgpu::TextureUsages::STORAGE_BINDING.bits(),
        render_attachment: wgpu::TextureUsages::RENDER_ATTACHMENT.bits(),
    }
}

#[napi]
pub fn gpu_shader_stage() -> ShaderStage {
    ShaderStage {
        vertex: wgpu::ShaderStages::VERTEX.bits(),
        fragment: wgpu::ShaderStages::FRAGMENT.bits(),
        compute: wgpu::ShaderStages::COMPUTE.bits(),
    }
}

#[napi(object)]
pub struct BufferUsage {
    pub map_read: u32,
    pub map_write: u32,
    pub copy_src: u32,
    pub copy_dst: u32,
    pub index: u32,
    pub vertex: u32,
    pub uniform: u32,
    pub storage: u32,
    pub indirect: u32,
    pub query_resolve: u32,
}

#[napi(object)]
pub struct TextureUsage {
    pub copy_src: u32,
    pub copy_dst: u32,
    pub texture_binding: u32,
    pub storage_binding: u32,
    pub render_attachment: u32,
}

#[napi(object)]
pub struct ShaderStage {
    pub vertex: u32,
    pub fragment: u32,
    pub compute: u32,
}

#[napi(object)]
pub struct BufferDescriptor {
    pub label: Option<String>,
    pub size: i64,
    pub usage: u32,
    #[napi(js_name = "mappedAtCreation")]
    pub mapped_at_creation: Option<bool>,
}

#[napi(object)]
pub struct TextureDescriptor {
    pub label: Option<String>,
    pub width: u32,
    pub height: u32,
    pub depth: Option<u32>,
    pub format: String,
    pub usage: u32,
    pub dimension: Option<String>,
    pub mip_level_count: Option<u32>,
    pub sample_count: Option<u32>,
}

#[napi(object)]
#[derive(Default)]
pub struct SamplerDescriptor {
    pub label: Option<String>,
    pub address_mode_u: Option<String>,
    pub address_mode_v: Option<String>,
    pub address_mode_w: Option<String>,
    pub mag_filter: Option<String>,
    pub min_filter: Option<String>,
    pub mipmap_filter: Option<String>,
    pub lod_min_clamp: Option<f64>,
    pub lod_max_clamp: Option<f64>,
    pub compare: Option<String>,
    pub max_anisotropy: Option<u16>,
}

#[napi(object)]
pub struct ShaderModuleDescriptor {
    pub label: Option<String>,
    pub code: String,
}

#[napi(object)]
pub struct PipelineLayoutDescriptor {
    pub label: Option<String>,
}

#[napi(object)]
pub struct ComputePipelineDescriptor {
    pub label: Option<String>,
    #[napi(js_name = "entryPoint")]
    pub entry_point: String,
}

#[napi(object)]
pub struct CommandEncoderDescriptor {
    pub label: Option<String>,
}

#[napi(object)]
pub struct BindGroupDescriptor {
    pub label: Option<String>,
}

#[napi(object)]
pub struct BindGroupLayoutDescriptor {
    pub label: Option<String>,
    pub entries: Vec<BindGroupLayoutEntry>,
}

#[napi(object)]
pub struct BindGroupLayoutEntry {
    pub binding: u32,
    pub visibility: u32,
    pub buffer: Option<BufferBindingLayout>,
    pub sampler: Option<SamplerBindingLayout>,
    pub texture: Option<TextureBindingLayout>,
    #[napi(js_name = "storageTexture")]
    pub storage_texture: Option<StorageTextureBindingLayout>,
}

#[napi(object)]
pub struct BufferBindingLayout {
    #[napi(js_name = "type")]
    pub ty: Option<String>,
    #[napi(js_name = "hasDynamicOffset")]
    pub has_dynamic_offset: Option<bool>,
    #[napi(js_name = "minBindingSize")]
    pub min_binding_size: Option<i64>,
}

#[napi(object)]
pub struct SamplerBindingLayout {
    #[napi(js_name = "type")]
    pub ty: Option<String>,
}

#[napi(object)]
pub struct TextureBindingLayout {
    #[napi(js_name = "sampleType")]
    pub sample_type: Option<String>,
    #[napi(js_name = "viewDimension")]
    pub view_dimension: Option<String>,
    pub multisampled: Option<bool>,
}

#[napi(object)]
pub struct StorageTextureBindingLayout {
    pub access: Option<String>,
    pub format: String,
    #[napi(js_name = "viewDimension")]
    pub view_dimension: Option<String>,
}

#[napi(object)]
pub struct BindGroupEntry {
    pub binding: u32,
    pub resource_type: String,
    pub offset: Option<i64>,
    pub size: Option<i64>,
}

#[napi(object)]
pub struct RenderPipelineDescriptor {
    pub label: Option<String>,
    pub vertex: VertexState,
    pub primitive: Option<PrimitiveState>,
    #[napi(js_name = "depthStencil")]
    pub depth_stencil: Option<DepthStencilState>,
    pub multisample: Option<MultisampleState>,
    pub fragment: Option<FragmentState>,
}

#[napi(object)]
pub struct VertexState {
    #[napi(js_name = "entryPoint")]
    pub entry_point: String,
    pub buffers: Option<Vec<VertexBufferLayout>>,
}

#[napi(object)]
pub struct VertexBufferLayout {
    #[napi(js_name = "arrayStride")]
    pub array_stride: i64,
    #[napi(js_name = "stepMode")]
    pub step_mode: Option<String>,
    pub attributes: Vec<VertexAttribute>,
}

#[napi(object)]
pub struct VertexAttribute {
    pub format: String,
    pub offset: i64,
    #[napi(js_name = "shaderLocation")]
    pub shader_location: u32,
}

#[napi(object)]
pub struct PrimitiveState {
    pub topology: Option<String>,
    #[napi(js_name = "frontFace")]
    pub front_face: Option<String>,
    #[napi(js_name = "cullMode")]
    pub cull_mode: Option<String>,
}

#[napi(object)]
pub struct DepthStencilState {
    pub format: String,
    #[napi(js_name = "depthWriteEnabled")]
    pub depth_write_enabled: Option<bool>,
    #[napi(js_name = "depthCompare")]
    pub depth_compare: Option<String>,
}

#[napi(object)]
pub struct MultisampleState {
    pub count: Option<u32>,
    pub mask: Option<u32>,
    #[napi(js_name = "alphaToCoverageEnabled")]
    pub alpha_to_coverage_enabled: Option<bool>,
}

#[napi(object)]
pub struct FragmentState {
    #[napi(js_name = "entryPoint")]
    pub entry_point: String,
    pub targets: Vec<ColorTargetState>,
}

#[napi(object)]
pub struct ColorTargetState {
    pub format: String,
    pub blend: Option<BlendState>,
    #[napi(js_name = "writeMask")]
    pub write_mask: Option<u32>,
}

#[napi(object)]
pub struct BlendState {
    pub color: BlendComponent,
    pub alpha: BlendComponent,
}

#[napi(object)]
pub struct BlendComponent {
    #[napi(js_name = "srcFactor")]
    pub src_factor: String,
    #[napi(js_name = "dstFactor")]
    pub dst_factor: String,
    pub operation: String,
}

#[napi(object)]
pub struct RenderPassDescriptor {
    pub label: Option<String>,
    pub color_attachments: Vec<RenderPassColorAttachment>,
    pub depth_stencil_attachment: Option<RenderPassDepthStencilAttachment>,
}

#[napi(object)]
pub struct RenderPassColorAttachment {
    pub clear_value: Option<GpuColor>,
    pub load_op: String,
    pub store_op: String,
}

#[napi(object)]
pub struct GpuColor {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

#[napi(object)]
pub struct RenderPassDepthStencilAttachment {
    pub depth_clear_value: Option<f64>,
    pub depth_load_op: Option<String>,
    pub depth_store_op: Option<String>,
}

#[napi(object)]
pub struct ComputePassDescriptor {
    pub label: Option<String>,
}

fn buffer_slice(buffer: &wgpu::Buffer, offset: Option<f64>, size: Option<f64>) -> wgpu::BufferSlice<'_> {
    match (offset, size) {
        (Some(offset), Some(size)) => buffer.slice(offset as u64..(offset as u64 + size as u64)),
        (Some(offset), None) => buffer.slice(offset as u64..),
        _ => buffer.slice(..),
    }
}

fn parse_texture_format(format: &str) -> Result<wgpu::TextureFormat> {
    Ok(match format {
        "r8unorm" => wgpu::TextureFormat::R8Unorm,
        "rg8unorm" => wgpu::TextureFormat::Rg8Unorm,
        "rgba8unorm" => wgpu::TextureFormat::Rgba8Unorm,
        "rgba8unorm-srgb" => wgpu::TextureFormat::Rgba8UnormSrgb,
        "bgra8unorm" => wgpu::TextureFormat::Bgra8Unorm,
        "bgra8unorm-srgb" => wgpu::TextureFormat::Bgra8UnormSrgb,
        "rgba8uint" => wgpu::TextureFormat::Rgba8Uint,
        "rgba8sint" => wgpu::TextureFormat::Rgba8Sint,
        "rgba16float" => wgpu::TextureFormat::Rgba16Float,
        "rgba32float" => wgpu::TextureFormat::Rgba32Float,
        "depth24plus" => wgpu::TextureFormat::Depth24Plus,
        "depth24plus-stencil8" => wgpu::TextureFormat::Depth24PlusStencil8,
        "depth32float" => wgpu::TextureFormat::Depth32Float,
        _ => {
            return Err(Error::from_reason(format!(
                "Unsupported texture format: {format}"
            )));
        }
    })
}

fn parse_vertex_format(format: &str) -> Result<wgpu::VertexFormat> {
    Ok(match format {
        "uint8x2" => wgpu::VertexFormat::Uint8x2,
        "uint8x4" => wgpu::VertexFormat::Uint8x4,
        "sint8x2" => wgpu::VertexFormat::Sint8x2,
        "sint8x4" => wgpu::VertexFormat::Sint8x4,
        "unorm8x2" => wgpu::VertexFormat::Unorm8x2,
        "unorm8x4" => wgpu::VertexFormat::Unorm8x4,
        "snorm8x2" => wgpu::VertexFormat::Snorm8x2,
        "snorm8x4" => wgpu::VertexFormat::Snorm8x4,
        "uint16x2" => wgpu::VertexFormat::Uint16x2,
        "uint16x4" => wgpu::VertexFormat::Uint16x4,
        "sint16x2" => wgpu::VertexFormat::Sint16x2,
        "sint16x4" => wgpu::VertexFormat::Sint16x4,
        "unorm16x2" => wgpu::VertexFormat::Unorm16x2,
        "unorm16x4" => wgpu::VertexFormat::Unorm16x4,
        "snorm16x2" => wgpu::VertexFormat::Snorm16x2,
        "snorm16x4" => wgpu::VertexFormat::Snorm16x4,
        "float16x2" => wgpu::VertexFormat::Float16x2,
        "float16x4" => wgpu::VertexFormat::Float16x4,
        "float32" => wgpu::VertexFormat::Float32,
        "float32x2" => wgpu::VertexFormat::Float32x2,
        "float32x3" => wgpu::VertexFormat::Float32x3,
        "float32x4" => wgpu::VertexFormat::Float32x4,
        "uint32" => wgpu::VertexFormat::Uint32,
        "uint32x2" => wgpu::VertexFormat::Uint32x2,
        "uint32x3" => wgpu::VertexFormat::Uint32x3,
        "uint32x4" => wgpu::VertexFormat::Uint32x4,
        "sint32" => wgpu::VertexFormat::Sint32,
        "sint32x2" => wgpu::VertexFormat::Sint32x2,
        "sint32x3" => wgpu::VertexFormat::Sint32x3,
        "sint32x4" => wgpu::VertexFormat::Sint32x4,
        _ => {
            return Err(Error::from_reason(format!(
                "Unsupported vertex format: {format}"
            )));
        }
    })
}

fn parse_view_dimension(dimension: Option<&str>) -> Option<wgpu::TextureViewDimension> {
    Some(match dimension? {
        "1d" => wgpu::TextureViewDimension::D1,
        "2d" => wgpu::TextureViewDimension::D2,
        "2d-array" => wgpu::TextureViewDimension::D2Array,
        "cube" => wgpu::TextureViewDimension::Cube,
        "cube-array" => wgpu::TextureViewDimension::CubeArray,
        "3d" => wgpu::TextureViewDimension::D3,
        _ => return None,
    })
}

fn parse_address_mode(mode: Option<&str>) -> wgpu::AddressMode {
    match mode {
        Some("repeat") => wgpu::AddressMode::Repeat,
        Some("mirror-repeat") => wgpu::AddressMode::MirrorRepeat,
        _ => wgpu::AddressMode::ClampToEdge,
    }
}

fn parse_filter_mode(mode: Option<&str>) -> wgpu::FilterMode {
    match mode {
        Some("linear") => wgpu::FilterMode::Linear,
        _ => wgpu::FilterMode::Nearest,
    }
}

fn parse_mipmap_filter_mode(mode: Option<&str>) -> wgpu::MipmapFilterMode {
    match mode {
        Some("linear") => wgpu::MipmapFilterMode::Linear,
        _ => wgpu::MipmapFilterMode::Nearest,
    }
}

fn parse_compare_function(func: Option<&str>) -> Option<wgpu::CompareFunction> {
    match func {
        Some("never") => Some(wgpu::CompareFunction::Never),
        Some("less") => Some(wgpu::CompareFunction::Less),
        Some("equal") => Some(wgpu::CompareFunction::Equal),
        Some("less-equal") => Some(wgpu::CompareFunction::LessEqual),
        Some("greater") => Some(wgpu::CompareFunction::Greater),
        Some("not-equal") => Some(wgpu::CompareFunction::NotEqual),
        Some("greater-equal") => Some(wgpu::CompareFunction::GreaterEqual),
        Some("always") => Some(wgpu::CompareFunction::Always),
        _ => None,
    }
}

fn parse_blend_factor(factor: &str) -> wgpu::BlendFactor {
    match factor {
        "zero" => wgpu::BlendFactor::Zero,
        "src" => wgpu::BlendFactor::Src,
        "one-minus-src" => wgpu::BlendFactor::OneMinusSrc,
        "src-alpha" => wgpu::BlendFactor::SrcAlpha,
        "one-minus-src-alpha" => wgpu::BlendFactor::OneMinusSrcAlpha,
        "dst" => wgpu::BlendFactor::Dst,
        "one-minus-dst" => wgpu::BlendFactor::OneMinusDst,
        "dst-alpha" => wgpu::BlendFactor::DstAlpha,
        "one-minus-dst-alpha" => wgpu::BlendFactor::OneMinusDstAlpha,
        "src-alpha-saturated" => wgpu::BlendFactor::SrcAlphaSaturated,
        "constant" => wgpu::BlendFactor::Constant,
        "one-minus-constant" => wgpu::BlendFactor::OneMinusConstant,
        _ => wgpu::BlendFactor::One,
    }
}

fn parse_blend_operation(operation: &str) -> wgpu::BlendOperation {
    match operation {
        "subtract" => wgpu::BlendOperation::Subtract,
        "reverse-subtract" => wgpu::BlendOperation::ReverseSubtract,
        "min" => wgpu::BlendOperation::Min,
        "max" => wgpu::BlendOperation::Max,
        _ => wgpu::BlendOperation::Add,
    }
}

fn parse_blend(blend: &BlendState) -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: parse_blend_factor(&blend.color.src_factor),
            dst_factor: parse_blend_factor(&blend.color.dst_factor),
            operation: parse_blend_operation(&blend.color.operation),
        },
        alpha: wgpu::BlendComponent {
            src_factor: parse_blend_factor(&blend.alpha.src_factor),
            dst_factor: parse_blend_factor(&blend.alpha.dst_factor),
            operation: parse_blend_operation(&blend.alpha.operation),
        },
    }
}

fn parse_primitive(primitive: &PrimitiveState) -> wgpu::PrimitiveState {
    wgpu::PrimitiveState {
        topology: match primitive.topology.as_deref() {
            Some("point-list") => wgpu::PrimitiveTopology::PointList,
            Some("line-list") => wgpu::PrimitiveTopology::LineList,
            Some("line-strip") => wgpu::PrimitiveTopology::LineStrip,
            Some("triangle-strip") => wgpu::PrimitiveTopology::TriangleStrip,
            _ => wgpu::PrimitiveTopology::TriangleList,
        },
        front_face: if primitive.front_face.as_deref() == Some("cw") {
            wgpu::FrontFace::Cw
        } else {
            wgpu::FrontFace::Ccw
        },
        cull_mode: match primitive.cull_mode.as_deref() {
            Some("front") => Some(wgpu::Face::Front),
            Some("back") => Some(wgpu::Face::Back),
            _ => None,
        },
        ..Default::default()
    }
}

fn parse_depth_stencil(state: &DepthStencilState) -> Result<wgpu::DepthStencilState> {
    Ok(wgpu::DepthStencilState {
        format: parse_texture_format(&state.format)?,
        depth_write_enabled: Some(state.depth_write_enabled.unwrap_or(true)),
        depth_compare: Some(
            parse_compare_function(state.depth_compare.as_deref())
                .unwrap_or(wgpu::CompareFunction::Less),
        ),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    })
}

fn convert_bind_group_layout_entry(entry: &BindGroupLayoutEntry) -> Result<wgpu::BindGroupLayoutEntry> {
    let visibility = wgpu::ShaderStages::from_bits_truncate(entry.visibility);
    let ty = if let Some(buffer) = &entry.buffer {
        wgpu::BindingType::Buffer {
            ty: match buffer.ty.as_deref() {
                Some("storage") => wgpu::BufferBindingType::Storage { read_only: false },
                Some("read-only-storage") => wgpu::BufferBindingType::Storage { read_only: true },
                _ => wgpu::BufferBindingType::Uniform,
            },
            has_dynamic_offset: buffer.has_dynamic_offset.unwrap_or(false),
            min_binding_size: buffer
                .min_binding_size
                .and_then(|size| std::num::NonZeroU64::new(size as u64)),
        }
    } else if let Some(sampler) = &entry.sampler {
        wgpu::BindingType::Sampler(match sampler.ty.as_deref() {
            Some("non-filtering") => wgpu::SamplerBindingType::NonFiltering,
            Some("comparison") => wgpu::SamplerBindingType::Comparison,
            _ => wgpu::SamplerBindingType::Filtering,
        })
    } else if let Some(texture) = &entry.texture {
        wgpu::BindingType::Texture {
            sample_type: match texture.sample_type.as_deref() {
                Some("unfilterable-float") => wgpu::TextureSampleType::Float { filterable: false },
                Some("depth") => wgpu::TextureSampleType::Depth,
                Some("sint") => wgpu::TextureSampleType::Sint,
                Some("uint") => wgpu::TextureSampleType::Uint,
                _ => wgpu::TextureSampleType::Float { filterable: true },
            },
            view_dimension: parse_view_dimension(texture.view_dimension.as_deref())
                .unwrap_or(wgpu::TextureViewDimension::D2),
            multisampled: texture.multisampled.unwrap_or(false),
        }
    } else if let Some(storage) = &entry.storage_texture {
        wgpu::BindingType::StorageTexture {
            access: match storage.access.as_deref() {
                Some("read-only") => wgpu::StorageTextureAccess::ReadOnly,
                Some("read-write") => wgpu::StorageTextureAccess::ReadWrite,
                _ => wgpu::StorageTextureAccess::WriteOnly,
            },
            format: parse_texture_format(&storage.format)?,
            view_dimension: parse_view_dimension(storage.view_dimension.as_deref())
                .unwrap_or(wgpu::TextureViewDimension::D2),
        }
    } else {
        return Err(Error::from_reason(
            "Bind group layout entry needs buffer, sampler, texture, or storageTexture",
        ));
    };
    Ok(wgpu::BindGroupLayoutEntry {
        binding: entry.binding,
        visibility: visibility,
        ty,
        count: None,
    })
}
