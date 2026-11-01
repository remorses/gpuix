use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    Bounds, Capslock, DevicePixels, DispatchEventResult, GpuSpecs, KeyDownEvent, KeyUpEvent,
    Modifiers, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, PlatformAtlas, PlatformDisplay, PlatformInput, PlatformInputHandler, PlatformWindow,
    Point, PromptButton, PromptLevel, RequestFrameOptions, Scene, ScrollDelta, ScrollWheelEvent,
    Size, TouchPhase, WindowAppearance, WindowBackgroundAppearance, WindowBounds,
    WindowControlArea, point, px, size,
};

use crate::{
    dispatcher::Wake,
    kitty::KittyPresenter,
    tty::{KeyEventKind, MouseEventKind, TerminalEvent, TerminalSize, Tty},
};

/// Frames are demand driven; this only caps how often a dirty window repaints.
const MIN_FRAME_INTERVAL: Duration = Duration::from_micros(16_667);
const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(500);
const DOUBLE_CLICK_DISTANCE: f32 = 4.;
/// Used when the terminal reports no pixel size (`ws_xpixel == 0`).
const FALLBACK_CELL_SIZE: (u32, u32) = (8, 16);

#[derive(Default)]
struct Callbacks {
    request_frame: Option<Box<dyn FnMut(RequestFrameOptions)>>,
    input: Option<Box<dyn FnMut(PlatformInput) -> DispatchEventResult>>,
    active_status_change: Option<Box<dyn FnMut(bool)>>,
    hover_status_change: Option<Box<dyn FnMut(bool)>>,
    resize: Option<Box<dyn FnMut(Size<Pixels>, f32)>>,
    close: Option<Box<dyn FnOnce()>>,
}

struct ClickState {
    button: MouseButton,
    position: Point<Pixels>,
    time: Instant,
    count: usize,
}

struct WindowState {
    size: TerminalSize,
    scale_factor: f32,
    pixel_mouse: bool,
    mouse_position: Point<Pixels>,
    modifiers: Modifiers,
    is_active: bool,
    input_handler: Option<PlatformInputHandler>,
    last_click: Option<ClickState>,
    title: String,
}

pub(crate) struct WindowInner {
    tty: Rc<RefCell<Tty>>,
    display: Rc<dyn PlatformDisplay>,
    quit_requested: Rc<Cell<bool>>,
    wake: Wake,
    state: RefCell<WindowState>,
    callbacks: RefCell<Callbacks>,
    renderer: RefCell<Renderer>,
    background: Cell<WindowBackgroundAppearance>,
    presenter: RefCell<KittyPresenter>,
    frame_requested: Cell<bool>,
    last_frame: Cell<Option<Instant>>,
}

pub(crate) struct TerminalWindow(pub(crate) Rc<WindowInner>);

impl TerminalWindow {
    pub(crate) fn new(
        tty: Rc<RefCell<Tty>>,
        display: Rc<dyn PlatformDisplay>,
        quit_requested: Rc<Cell<bool>>,
        wake: Wake,
        size: TerminalSize,
    ) -> anyhow::Result<Self> {
        let size = with_pixel_fallback(size);
        let scale_factor = scale_factor_for(size);
        let presenter = KittyPresenter::new(&mut tty.borrow_mut())?;
        Ok(Self(Rc::new(WindowInner {
            tty,
            display,
            quit_requested,
            wake,
            state: RefCell::new(WindowState {
                size,
                scale_factor,
                pixel_mouse: true,
                mouse_position: Point::default(),
                modifiers: Modifiers::default(),
                is_active: true,
                input_handler: None,
                last_click: None,
                title: String::new(),
            }),
            callbacks: RefCell::new(Callbacks::default()),
            renderer: RefCell::new(Renderer::new(gpui::size(
                DevicePixels(size.width as i32),
                DevicePixels(size.height as i32),
            ))?),
            background: Cell::new(WindowBackgroundAppearance::Opaque),
            presenter: RefCell::new(presenter),
            frame_requested: Cell::new(true),
            last_frame: Cell::new(None),
        })))
    }
}

/// Terminals report physical pixels but no DPI. A cell taller than 24 device
/// pixels is a HiDPI screen at any common font size.
/// `GPUI_TERMINAL_SCALE` overrides the guess.
fn scale_factor_for(size: TerminalSize) -> f32 {
    if let Some(scale) = std::env::var("GPUI_TERMINAL_SCALE")
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|scale| scale.is_finite() && *scale > 0.)
    {
        return scale;
    }
    let (_, cell_height) = size.cell_size();
    if cell_height >= 24. { 2. } else { 1. }
}

fn with_pixel_fallback(mut size: TerminalSize) -> TerminalSize {
    size.columns = size.columns.max(1);
    size.rows = size.rows.max(1);
    if size.width == 0 || size.height == 0 {
        size.width = size.columns as u32 * FALLBACK_CELL_SIZE.0;
        size.height = size.rows as u32 * FALLBACK_CELL_SIZE.1;
    }
    size
}

impl WindowInner {
    fn request_frame(&self) {
        if !self.frame_requested.replace(true) {
            self.wake.wake();
        }
    }

    fn logical_size(&self) -> Size<Pixels> {
        let state = self.state.borrow();
        size(
            px(state.size.width as f32 / state.scale_factor),
            px(state.size.height as f32 / state.scale_factor),
        )
    }

    fn dispatch(&self, input: PlatformInput) -> Option<DispatchEventResult> {
        let mut callback = self.callbacks.borrow_mut().input.take()?;
        let result = callback(input);
        self.callbacks.borrow_mut().input.get_or_insert(callback);
        Some(result)
    }

    fn with_input_handler<R>(&self, f: impl FnOnce(&mut PlatformInputHandler) -> R) -> Option<R> {
        let mut handler = self.state.borrow_mut().input_handler.take()?;
        let result = f(&mut handler);
        self.state.borrow_mut().input_handler.get_or_insert(handler);
        Some(result)
    }

    /// When the window wants a frame and the frame interval elapsed, renders
    /// it. Returns when the next frame is due, if one is wanted.
    pub(crate) fn run_frame_if_due(&self) -> Option<Instant> {
        if !self.frame_requested.get() {
            return None;
        }
        let now = Instant::now();
        if let Some(last_frame) = self.last_frame.get()
            && now < last_frame + MIN_FRAME_INTERVAL
        {
            return Some(last_frame + MIN_FRAME_INTERVAL);
        }
        if let Some(deadline) = self.presenter.borrow_mut().wait_until(now) {
            return Some(deadline);
        }
        self.frame_requested.set(false);
        self.last_frame.set(Some(now));
        let mut callback = self.callbacks.borrow_mut().request_frame.take()?;
        callback(RequestFrameOptions {
            require_presentation: false,
            force_render: false,
        });
        self.callbacks
            .borrow_mut()
            .request_frame
            .get_or_insert(callback);
        self.frame_requested
            .get()
            .then_some(now + MIN_FRAME_INTERVAL)
    }

    pub(crate) fn handle_event(&self, event: TerminalEvent) {
        match event {
            TerminalEvent::Key { keystroke, kind } => self.handle_key(keystroke, kind),
            TerminalEvent::Modifiers(modifiers) => {
                self.state.borrow_mut().modifiers = modifiers;
                self.dispatch(PlatformInput::ModifiersChanged(ModifiersChangedEvent {
                    modifiers,
                    capslock: Capslock::default(),
                }));
            }
            TerminalEvent::Mouse {
                kind,
                x,
                y,
                pixels: _,
                modifiers,
            } => self.handle_mouse(kind, x, y, modifiers),
            TerminalEvent::Paste(text) => {
                self.with_input_handler(|handler| handler.replace_text_in_range(None, &text));
            }
            TerminalEvent::Focus(focused) => {
                self.state.borrow_mut().is_active = focused;
                self.notify_status(|callbacks| &mut callbacks.active_status_change, focused);
                self.notify_status(|callbacks| &mut callbacks.hover_status_change, focused);
            }
            TerminalEvent::Resize(size) => self.handle_resize(size),
            TerminalEvent::PixelMouseSupported(supported) => {
                self.state.borrow_mut().pixel_mouse = supported;
            }
            TerminalEvent::GraphicsReply { id, ok } => {
                self.presenter.borrow_mut().handle_reply(id, ok);
            }
            TerminalEvent::DeviceAttributes => {
                self.presenter.borrow_mut().handle_device_attributes()
            }
            TerminalEvent::Quit => self.quit_requested.set(true),
        }
    }

    /// Callbacks are taken out while they run: they re-enter the window.
    fn notify_status(
        &self,
        slot: impl Fn(&mut Callbacks) -> &mut Option<Box<dyn FnMut(bool)>>,
        value: bool,
    ) {
        let callback = slot(&mut self.callbacks.borrow_mut()).take();
        if let Some(mut callback) = callback {
            callback(value);
            slot(&mut self.callbacks.borrow_mut()).get_or_insert(callback);
        }
    }

    fn handle_key(&self, keystroke: gpui::Keystroke, kind: KeyEventKind) {
        if kind == KeyEventKind::Release {
            self.dispatch(PlatformInput::KeyUp(KeyUpEvent { keystroke }));
            return;
        }
        let result = self.dispatch(PlatformInput::KeyDown(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: kind == KeyEventKind::Repeat,
            prefer_character_input: false,
        }));
        if result.is_some_and(|result| !result.propagate) {
            return;
        }
        let modifiers = keystroke.modifiers;
        if !modifiers.control
            && !modifiers.platform
            && let Some(text) = keystroke.key_char.as_deref()
            && self
                .with_input_handler(|handler| handler.replace_text_in_range(None, text))
                .is_some()
        {
            return;
        }
        // Raw mode turns off the terminal's own SIGINT, so an app that does
        // not bind ctrl-c must still be able to exit with it.
        if modifiers.control && keystroke.key == "c" && !modifiers.alt && !modifiers.shift {
            self.quit_requested.set(true);
        }
    }

    fn handle_mouse(&self, kind: MouseEventKind, x: f32, y: f32, modifiers: Modifiers) {
        let (position, moved) = {
            let mut state = self.state.borrow_mut();
            let (x, y) = if state.pixel_mouse {
                (x, y)
            } else {
                let (cell_width, cell_height) = state.size.cell_size();
                ((x + 0.5) * cell_width, (y + 0.5) * cell_height)
            };
            let position = point(px(x / state.scale_factor), px(y / state.scale_factor));
            let moved = state.mouse_position != position;
            state.mouse_position = position;
            state.modifiers = modifiers;
            (position, moved)
        };
        // A terminal can report a press or a wheel tick at a position it never
        // reported a move to (motion reports are coalesced, or the pointer
        // entered over the grid). GPUI derives hover from moves, so send one.
        if moved && !matches!(kind, MouseEventKind::Move(_)) {
            let pressed_button = match kind {
                MouseEventKind::Release(button) => Some(button),
                _ => None,
            };
            self.dispatch(PlatformInput::MouseMove(MouseMoveEvent {
                position,
                pressed_button,
                modifiers,
            }));
        }
        let input = match kind {
            MouseEventKind::Press(button) => {
                let click_count = self.click_count(button, position);
                PlatformInput::MouseDown(MouseDownEvent {
                    button,
                    position,
                    modifiers,
                    click_count,
                    first_mouse: false,
                })
            }
            MouseEventKind::Release(button) => {
                let click_count = self
                    .state
                    .borrow()
                    .last_click
                    .as_ref()
                    .map_or(1, |click| click.count);
                PlatformInput::MouseUp(MouseUpEvent {
                    button,
                    position,
                    modifiers,
                    click_count,
                })
            }
            MouseEventKind::Move(pressed_button) => PlatformInput::MouseMove(MouseMoveEvent {
                position,
                pressed_button,
                modifiers,
            }),
            MouseEventKind::Wheel { dx, dy } => {
                // One wheel report scrolls one terminal line, like any TUI.
                let state = self.state.borrow();
                let (cell_width, cell_height) = state.size.cell_size();
                let scale_factor = state.scale_factor;
                drop(state);
                PlatformInput::ScrollWheel(ScrollWheelEvent {
                    position,
                    delta: ScrollDelta::Pixels(point(
                        px(-(dx as f32) * cell_width / scale_factor),
                        px(-(dy as f32) * cell_height / scale_factor),
                    )),
                    modifiers,
                    touch_phase: TouchPhase::Moved,
                })
            }
        };
        self.dispatch(input);
    }

    fn click_count(&self, button: MouseButton, position: Point<Pixels>) -> usize {
        let now = Instant::now();
        let mut state = self.state.borrow_mut();
        let count = match &state.last_click {
            Some(click)
                if click.button == button
                    && now.duration_since(click.time) < DOUBLE_CLICK_INTERVAL
                    && (click.position.x - position.x).abs() < px(DOUBLE_CLICK_DISTANCE)
                    && (click.position.y - position.y).abs() < px(DOUBLE_CLICK_DISTANCE) =>
            {
                click.count + 1
            }
            _ => 1,
        };
        state.last_click = Some(ClickState {
            button,
            position,
            time: now,
            count,
        });
        count
    }

    fn handle_resize(&self, size: TerminalSize) {
        let size = with_pixel_fallback(size);
        let scale_factor = {
            let mut state = self.state.borrow_mut();
            if state.size == size {
                return;
            }
            state.size = size;
            state.scale_factor
        };
        let logical_size = self.logical_size();
        let callback = self.callbacks.borrow_mut().resize.take();
        if let Some(mut callback) = callback {
            callback(logical_size, scale_factor);
            self.callbacks.borrow_mut().resize.get_or_insert(callback);
        }
        self.frame_requested.set(true);
    }

    fn draw(&self, scene: &Scene) {
        let (width, height) = {
            let state = self.state.borrow();
            (state.size.width, state.size.height)
        };
        let result = autoreleasepool(|| {
            let mut presenter = self.presenter.borrow_mut();
            let mut frame = presenter.begin_frame(width, height)?;
            self.renderer.borrow_mut().render(
                scene,
                size(DevicePixels(width as i32), DevicePixels(height as i32)),
                frame.pixels(),
            )?;
            presenter.present(&mut self.tty.borrow_mut(), frame)
        });
        if let Err(error) = result {
            log::error!("failed to draw terminal frame: {error:#}");
        }
    }

    pub(crate) fn close(&self) {
        let close = self.callbacks.borrow_mut().close.take();
        if let Some(close) = close {
            close();
        }
    }
}

impl raw_window_handle::HasWindowHandle for TerminalWindow {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        Err(raw_window_handle::HandleError::NotSupported)
    }
}

impl raw_window_handle::HasDisplayHandle for TerminalWindow {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Err(raw_window_handle::HandleError::NotSupported)
    }
}

impl PlatformWindow for TerminalWindow {
    fn bounds(&self) -> Bounds<Pixels> {
        Bounds::new(Point::default(), self.0.logical_size())
    }

    fn is_maximized(&self) -> bool {
        true
    }

    fn window_bounds(&self) -> WindowBounds {
        WindowBounds::Windowed(self.bounds())
    }

    fn content_size(&self) -> Size<Pixels> {
        self.0.logical_size()
    }

    // The terminal owns its size.
    fn resize(&mut self, _size: Size<Pixels>) {}

    fn scale_factor(&self) -> f32 {
        self.0.state.borrow().scale_factor
    }

    fn appearance(&self) -> WindowAppearance {
        WindowAppearance::Dark
    }

    fn display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        Some(self.0.display.clone())
    }

    fn mouse_position(&self) -> Point<Pixels> {
        self.0.state.borrow().mouse_position
    }

    fn modifiers(&self) -> Modifiers {
        self.0.state.borrow().modifiers
    }

    fn capslock(&self) -> Capslock {
        Capslock::default()
    }

    fn set_input_handler(&mut self, input_handler: PlatformInputHandler) {
        self.0.state.borrow_mut().input_handler = Some(input_handler);
    }

    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        self.0.state.borrow_mut().input_handler.take()
    }

    fn prompt(
        &self,
        _level: PromptLevel,
        _msg: &str,
        _detail: Option<&str>,
        _answers: &[PromptButton],
    ) -> Option<futures::channel::oneshot::Receiver<usize>> {
        // GPUI falls back to prompts it renders itself.
        None
    }

    fn activate(&self) {}

    fn is_active(&self) -> bool {
        self.0.state.borrow().is_active
    }

    fn is_hovered(&self) -> bool {
        self.0.state.borrow().is_active
    }

    fn background_appearance(&self) -> WindowBackgroundAppearance {
        self.0.background.get()
    }

    fn set_title(&mut self, title: &str) {
        let mut state = self.0.state.borrow_mut();
        if state.title == title {
            return;
        }
        state.title = title.to_owned();
        // OSC 2 sets the terminal window title. Control bytes would end it early.
        let title: String = title
            .chars()
            .filter(|character| !character.is_control())
            .collect();
        if let Err(error) = self
            .0
            .tty
            .borrow_mut()
            .write_all(format!("\x1b]2;{title}\x1b\\").as_bytes())
        {
            log::error!("failed to set the terminal title: {error:#}");
        }
    }

    fn get_title(&self) -> String {
        self.0.state.borrow().title.clone()
    }

    /// `Transparent` and `Blurred` leave unpainted pixels see-through, so the
    /// terminal's own background shows behind the app. A terminal configured
    /// with background opacity and blur (kitty `background_blur`, Ghostty
    /// `background-blur`) then shows the blurred wallpaper. No escape code lets
    /// an app turn that blur on, so `Blurred` cannot do more than `Transparent`.
    fn set_background_appearance(&self, background_appearance: WindowBackgroundAppearance) {
        self.0.background.set(background_appearance);
        self.0
            .renderer
            .borrow_mut()
            .set_transparent(background_appearance != WindowBackgroundAppearance::Opaque);
        self.0.frame_requested.set(true);
    }

    fn minimize(&self) {}

    fn zoom(&self) {}

    fn toggle_fullscreen(&self) {}

    fn is_fullscreen(&self) -> bool {
        true
    }

    fn frame_waker(&self) -> Option<Rc<dyn Fn()>> {
        // Weak: the waker lives in the window's invalidator, which the frame
        // callback owns, so a strong reference would leak the window.
        let inner = Rc::downgrade(&self.0);
        Some(Rc::new(move || {
            if let Some(inner) = inner.upgrade() {
                inner.request_frame();
            }
        }))
    }

    fn schedule_frame(&self) {
        self.0.request_frame();
    }

    fn on_request_frame(&self, callback: Box<dyn FnMut(RequestFrameOptions)>) {
        self.0.callbacks.borrow_mut().request_frame = Some(callback);
    }

    fn on_input(&self, callback: Box<dyn FnMut(PlatformInput) -> DispatchEventResult>) {
        self.0.callbacks.borrow_mut().input = Some(callback);
    }

    fn on_active_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.0.callbacks.borrow_mut().active_status_change = Some(callback);
    }

    fn on_hover_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.0.callbacks.borrow_mut().hover_status_change = Some(callback);
    }

    fn on_resize(&self, callback: Box<dyn FnMut(Size<Pixels>, f32)>) {
        self.0.callbacks.borrow_mut().resize = Some(callback);
    }

    fn on_moved(&self, _callback: Box<dyn FnMut()>) {}

    fn on_should_close(&self, _callback: Box<dyn FnMut() -> bool>) {}

    fn on_hit_test_window_control(&self, _callback: Box<dyn FnMut() -> Option<WindowControlArea>>) {
    }

    fn on_close(&self, callback: Box<dyn FnOnce()>) {
        self.0.callbacks.borrow_mut().close = Some(callback);
    }

    fn on_appearance_changed(&self, _callback: Box<dyn FnMut()>) {}

    fn draw(&self, scene: &Scene) {
        self.0.draw(scene);
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.0.renderer.borrow().sprite_atlas()
    }

    fn is_subpixel_rendering_supported(&self) -> bool {
        false
    }

    fn update_ime_position(&self, _bounds: Bounds<Pixels>) {}

    fn gpu_specs(&self) -> Option<GpuSpecs> {
        None
    }
}

/// Renders a scene offscreen into CPU-readable RGBA pixels.
#[cfg(target_os = "macos")]
struct Renderer {
    renderer: gpui_macos::metal_renderer::MetalRenderer,
    transparent: bool,
}

#[cfg(target_os = "macos")]
impl Renderer {
    fn new(_size: Size<DevicePixels>) -> anyhow::Result<Self> {
        let instance_buffer_pool = Arc::new(parking_lot::Mutex::new(
            gpui_macos::metal_renderer::InstanceBufferPool::default(),
        ));
        // Kitty takes RGBA. Rendering in that format avoids a CPU swizzle.
        Ok(Self {
            renderer: gpui_macos::metal_renderer::MetalRenderer::new_headless(
                instance_buffer_pool,
                metal::MTLPixelFormat::RGBA8Unorm,
            ),
            transparent: false,
        })
    }

    /// A headless Metal renderer clears to opaque black until told otherwise.
    fn set_transparent(&mut self, transparent: bool) {
        self.transparent = transparent;
        self.renderer.update_transparency(transparent);
    }

    fn render(
        &mut self,
        scene: &Scene,
        size: Size<DevicePixels>,
        pixels: &mut [u8],
    ) -> anyhow::Result<()> {
        self.renderer.render_scene_to_pixels(scene, size, pixels)?;
        if self.transparent {
            unpremultiply(pixels);
        }
        Ok(())
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.renderer.sprite_atlas().clone()
    }
}

/// The event loop has no Cocoa run loop draining autoreleased Metal objects
/// (command buffers, encoders, pass descriptors), so each frame drains its own.
#[cfg(target_os = "macos")]
fn autoreleasepool<T>(body: impl FnOnce() -> T) -> T {
    objc::rc::autoreleasepool(body)
}

#[cfg(not(target_os = "macos"))]
fn autoreleasepool<T>(body: impl FnOnce() -> T) -> T {
    body()
}

#[cfg(not(target_os = "macos"))]
struct Renderer {
    renderer: gpui_wgpu::WgpuRenderer,
    _context: gpui_wgpu::WgpuContext,
    transparent: bool,
}

#[cfg(not(target_os = "macos"))]
impl Renderer {
    fn new(size: Size<DevicePixels>) -> anyhow::Result<Self> {
        let context = gpui_wgpu::WgpuContext::new_offscreen()?;
        Ok(Self {
            renderer: gpui_wgpu::WgpuRenderer::new_offscreen(&context, size)?,
            _context: context,
            transparent: false,
        })
    }

    fn set_transparent(&mut self, transparent: bool) {
        self.transparent = transparent;
    }

    fn render(
        &mut self,
        scene: &Scene,
        size: Size<DevicePixels>,
        pixels: &mut [u8],
    ) -> anyhow::Result<()> {
        self.renderer.render_to_pixels(scene, size, pixels)?;
        // The offscreen wgpu target always clears to transparent, so an opaque
        // window has to drop the alpha itself. Its colour is already the scene
        // composited over black, which is what an opaque native window shows.
        if self.transparent {
            unpremultiply(pixels);
        } else {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel[3] = u8::MAX;
            }
        }
        Ok(())
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.renderer.sprite_atlas().clone()
    }
}

/// GPUI blends onto a transparent clear, so translucent pixels come back with
/// premultiplied colour. The kitty graphics protocol's `f=32` is straight RGBA,
/// so without this a 50% white panel would reach the terminal as 50% gray.
fn unpremultiply(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 || alpha == 255 {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::unpremultiply;

    #[test]
    fn unpremultiply_restores_straight_alpha() {
        // 50% white and 25% red, premultiplied, plus an opaque and an empty pixel.
        let mut pixels = vec![
            128, 128, 128, 128, //
            64, 0, 0, 64, //
            10, 20, 30, 255, //
            0, 0, 0, 0,
        ];
        unpremultiply(&mut pixels);
        assert_eq!(
            pixels,
            vec![
                255, 255, 255, 128, //
                255, 0, 0, 64, //
                10, 20, 30, 255, //
                0, 0, 0, 0,
            ]
        );
    }

    #[test]
    fn unpremultiply_clamps_colour_brighter_than_its_alpha() {
        // GPUI's Metal pipeline adds destination alpha, so a channel can exceed it.
        let mut pixels = vec![200, 0, 0, 100];
        unpremultiply(&mut pixels);
        assert_eq!(pixels, vec![255, 0, 0, 100]);
    }
}
