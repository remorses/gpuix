/// NodeWindow — implements gpui::PlatformWindow for the Node.js environment.
///
/// Creates a wgpu rendering surface from a winit::Window handle.
/// The winit window is passed in (owned by this struct to keep it alive).
/// The winit EventLoop is NOT stored here — it lives in GpuixRenderer
/// and is pumped during tick().
///
/// Reference: gpui_web/src/window.rs (689 lines)

use gpui::{
    AnyWindowHandle, Bounds, Capslock, Decorations, DevicePixels, DispatchEventResult, GpuSpecs,
    Modifiers, Pixels, PlatformAtlas, PlatformDisplay, PlatformInput, PlatformInputHandler,
    PlatformWindow, Point, PromptButton, PromptLevel, RequestFrameOptions, ResizeEdge, Scene,
    Size, WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowControlArea,
    WindowControls, WindowDecorations, WindowParams, px,
};
use gpui_wgpu::{WgpuContext, WgpuRenderer, WgpuSurfaceConfig};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use crate::platform::node_display::NodeDisplay;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Default)]
pub struct NodeWindowCallbacks {
    pub request_frame: Option<Box<dyn FnMut(RequestFrameOptions)>>,
    pub input: Option<Box<dyn FnMut(PlatformInput) -> DispatchEventResult>>,
    pub active_status_change: Option<Box<dyn FnMut(bool)>>,
    pub hover_status_change: Option<Box<dyn FnMut(bool)>>,
    pub resize: Option<Box<dyn FnMut(Size<Pixels>, f32)>>,
    pub moved: Option<Box<dyn FnMut()>>,
    pub should_close: Option<Box<dyn FnMut() -> bool>>,
    pub close: Option<Box<dyn FnOnce()>>,
    pub appearance_changed: Option<Box<dyn FnMut()>>,
    pub hit_test_window_control: Option<Box<dyn FnMut() -> Option<WindowControlArea>>>,
}

/// Shared mutable state for the window. Wrapped in Rc so both NodeWindow
/// and the external tick handler can access callbacks/state.
pub struct NodeWindowState {
    pub renderer: RefCell<WgpuRenderer>,
    pub callbacks: RefCell<NodeWindowCallbacks>,
    pub bounds: RefCell<Bounds<Pixels>>,
    pub scale_factor: Cell<f32>,
    pub mouse_position: Cell<Point<Pixels>>,
    pub modifiers: Cell<Modifiers>,
    pub capslock: Cell<Capslock>,
    pub input_handler: RefCell<Option<PlatformInputHandler>>,
    pub is_active: Cell<bool>,
    pub is_hovered: Cell<bool>,
    pub is_fullscreen: Cell<bool>,
    pub title: RefCell<String>,
    pub sprite_atlas: Arc<dyn PlatformAtlas>,
}

pub struct NodeWindow {
    /// The winit window — MUST stay alive for WgpuRenderer surface validity
    #[allow(dead_code)]
    winit_window: winit::window::Window,
    /// Shared state accessed by both PlatformWindow methods and external tick
    state: Rc<NodeWindowState>,
    display: Rc<dyn PlatformDisplay>,
    #[allow(dead_code)]
    handle: AnyWindowHandle,
}

impl NodeWindow {
    /// Create a new NodeWindow from an existing winit window.
    /// The winit_window is moved into this struct to keep it alive.
    pub fn new(
        handle: AnyWindowHandle,
        _params: WindowParams,
        winit_window: winit::window::Window,
        wgpu_context: &mut Option<WgpuContext>,
    ) -> anyhow::Result<(Self, Rc<NodeWindowState>)> {
        let scale_factor = winit_window.scale_factor() as f32;
        let inner_size = winit_window.inner_size();

        let device_size = Size {
            width: DevicePixels(inner_size.width as i32),
            height: DevicePixels(inner_size.height as i32),
        };

        let renderer_config = WgpuSurfaceConfig {
            size: device_size,
            transparent: false,
        };

        // Pre-create wgpu context with Metal backend if not already created.
        // gpui_wgpu's WgpuContext::instance() hardcodes VULKAN|GL (no Metal),
        // because it was designed for Linux/WASM. On macOS we need Metal.
        if wgpu_context.is_none() {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                flags: wgpu::InstanceFlags::default(),
                backend_options: wgpu::BackendOptions::default(),
                memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            });

            let window_handle = winit_window
                .window_handle()
                .map_err(|e| anyhow::anyhow!("Failed to get window handle: {e}"))?;
            let display_handle = winit_window
                .display_handle()
                .map_err(|e| anyhow::anyhow!("Failed to get display handle: {e}"))?;

            let target = wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: display_handle.as_raw(),
                raw_window_handle: window_handle.as_raw(),
            };

            let surface = unsafe {
                instance
                    .create_surface_unsafe(target)
                    .map_err(|e| anyhow::anyhow!("Failed to create surface: {e}"))?
            };

            let context = WgpuContext::new(instance, &surface)?;
            *wgpu_context = Some(context);
        }

        // Create wgpu renderer from the winit window.
        // SAFETY: winit_window is stored in self and lives as long as NodeWindow.
        let renderer = WgpuRenderer::new(wgpu_context, &winit_window, renderer_config)?;

        let logical_width = inner_size.width as f32 / scale_factor;
        let logical_height = inner_size.height as f32 / scale_factor;

        let bounds = Bounds {
            origin: Point::default(),
            size: Size {
                width: px(logical_width),
                height: px(logical_height),
            },
        };

        let display: Rc<dyn PlatformDisplay> = Rc::new(NodeDisplay::new());
        let sprite_atlas = renderer.sprite_atlas().clone();

        let state = Rc::new(NodeWindowState {
            renderer: RefCell::new(renderer),
            callbacks: RefCell::new(NodeWindowCallbacks::default()),
            bounds: RefCell::new(bounds),
            scale_factor: Cell::new(scale_factor),
            mouse_position: Cell::new(Point::default()),
            modifiers: Cell::new(Modifiers::default()),
            capslock: Cell::new(Capslock::default()),
            input_handler: RefCell::new(None),
            is_active: Cell::new(true),
            is_hovered: Cell::new(false),
            is_fullscreen: Cell::new(false),
            title: RefCell::new(String::new()),
            sprite_atlas,
        });

        let state_clone = state.clone();

        let window = Self {
            winit_window,
            state,
            display,
            handle,
        };

        // Return both the window and a clone of the shared state
        // The caller stores the state clone for tick() access
        Ok((window, state_clone))
    }
}

impl raw_window_handle::HasWindowHandle for NodeWindow {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        self.winit_window.window_handle()
    }
}

impl raw_window_handle::HasDisplayHandle for NodeWindow {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        self.winit_window.display_handle()
    }
}

impl PlatformWindow for NodeWindow {
    fn bounds(&self) -> Bounds<Pixels> {
        *self.state.bounds.borrow()
    }

    fn is_maximized(&self) -> bool {
        false
    }

    fn window_bounds(&self) -> WindowBounds {
        WindowBounds::Windowed(self.bounds())
    }

    fn content_size(&self) -> Size<Pixels> {
        self.state.bounds.borrow().size
    }

    fn resize(&mut self, size: Size<Pixels>) {
        let scale = self.state.scale_factor.get();
        let device_width = (f32::from(size.width) * scale) as i32;
        let device_height = (f32::from(size.height) * scale) as i32;

        self.state.renderer.borrow_mut().update_drawable_size(Size {
            width: DevicePixels(device_width),
            height: DevicePixels(device_height),
        });

        *self.state.bounds.borrow_mut() = Bounds {
            origin: Point::default(),
            size,
        };
    }

    fn scale_factor(&self) -> f32 {
        self.state.scale_factor.get()
    }

    fn appearance(&self) -> WindowAppearance {
        WindowAppearance::Dark
    }

    fn display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        Some(self.display.clone())
    }

    fn mouse_position(&self) -> Point<Pixels> {
        self.state.mouse_position.get()
    }

    fn modifiers(&self) -> Modifiers {
        self.state.modifiers.get()
    }

    fn capslock(&self) -> Capslock {
        self.state.capslock.get()
    }

    fn set_input_handler(&mut self, input_handler: PlatformInputHandler) {
        *self.state.input_handler.borrow_mut() = Some(input_handler);
    }

    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        self.state.input_handler.borrow_mut().take()
    }

    fn prompt(
        &self,
        _level: PromptLevel,
        _msg: &str,
        _detail: Option<&str>,
        _answers: &[PromptButton],
    ) -> Option<futures::channel::oneshot::Receiver<usize>> {
        None
    }

    fn activate(&self) {
        self.state.is_active.set(true);
    }

    fn is_active(&self) -> bool {
        self.state.is_active.get()
    }

    fn is_hovered(&self) -> bool {
        self.state.is_hovered.get()
    }

    fn background_appearance(&self) -> WindowBackgroundAppearance {
        WindowBackgroundAppearance::Opaque
    }

    fn set_title(&mut self, title: &str) {
        *self.state.title.borrow_mut() = title.to_owned();
        self.winit_window.set_title(title);
    }

    fn set_background_appearance(&self, _background: WindowBackgroundAppearance) {}

    fn minimize(&self) {}

    fn zoom(&self) {}

    fn toggle_fullscreen(&self) {
        self.state.is_fullscreen.set(!self.state.is_fullscreen.get());
    }

    fn is_fullscreen(&self) -> bool {
        self.state.is_fullscreen.get()
    }

    fn on_request_frame(&self, callback: Box<dyn FnMut(RequestFrameOptions)>) {
        self.state.callbacks.borrow_mut().request_frame = Some(callback);
    }

    fn on_input(&self, callback: Box<dyn FnMut(PlatformInput) -> DispatchEventResult>) {
        self.state.callbacks.borrow_mut().input = Some(callback);
    }

    fn on_active_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.state.callbacks.borrow_mut().active_status_change = Some(callback);
    }

    fn on_hover_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.state.callbacks.borrow_mut().hover_status_change = Some(callback);
    }

    fn on_resize(&self, callback: Box<dyn FnMut(Size<Pixels>, f32)>) {
        self.state.callbacks.borrow_mut().resize = Some(callback);
    }

    fn on_moved(&self, callback: Box<dyn FnMut()>) {
        self.state.callbacks.borrow_mut().moved = Some(callback);
    }

    fn on_should_close(&self, callback: Box<dyn FnMut() -> bool>) {
        self.state.callbacks.borrow_mut().should_close = Some(callback);
    }

    fn on_close(&self, callback: Box<dyn FnOnce()>) {
        self.state.callbacks.borrow_mut().close = Some(callback);
    }

    fn on_hit_test_window_control(&self, callback: Box<dyn FnMut() -> Option<WindowControlArea>>) {
        self.state.callbacks.borrow_mut().hit_test_window_control = Some(callback);
    }

    fn on_appearance_changed(&self, callback: Box<dyn FnMut()>) {
        self.state.callbacks.borrow_mut().appearance_changed = Some(callback);
    }

    fn draw(&self, scene: &Scene) {
        self.state.renderer.borrow_mut().draw(scene);
    }

    fn completed_frame(&self) {
        // wgpu presents automatically
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.state.sprite_atlas.clone()
    }

    fn is_subpixel_rendering_supported(&self) -> bool {
        self.state.renderer.borrow().supports_dual_source_blending()
    }

    fn gpu_specs(&self) -> Option<GpuSpecs> {
        Some(self.state.renderer.borrow().gpu_specs())
    }

    fn update_ime_position(&self, _bounds: Bounds<Pixels>) {}

    fn request_decorations(&self, _decorations: WindowDecorations) {}

    fn show_window_menu(&self, _position: Point<Pixels>) {}

    fn start_window_move(&self) {}

    fn start_window_resize(&self, _edge: ResizeEdge) {}

    fn window_decorations(&self) -> Decorations {
        Decorations::Server
    }

    fn set_app_id(&mut self, _app_id: &str) {}

    fn window_controls(&self) -> WindowControls {
        WindowControls {
            fullscreen: true,
            maximize: true,
            minimize: true,
            window_menu: false,
        }
    }

    fn set_client_inset(&self, _inset: Pixels) {}
}
