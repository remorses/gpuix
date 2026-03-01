/// NodePlatform — implements gpui::Platform for the Node.js environment.
///
/// Key difference from MacPlatform: run() returns immediately instead of blocking.
/// JS drives the frame loop by calling tick() via setImmediate.
///
/// Reference: gpui_web/src/platform.rs (341 lines)
use crate::platform::node_dispatcher::NodeDispatcher;
use crate::platform::node_display::NodeDisplay;
use anyhow::Result;
use futures::channel::oneshot;
use gpui::{
    Action, AnyWindowHandle, BackgroundExecutor, ClipboardItem, CursorStyle, DummyKeyboardMapper,
    ForegroundExecutor, Keymap, Menu, MenuItem, PathPromptOptions, Platform, PlatformDisplay,
    PlatformKeyboardLayout, PlatformKeyboardMapper, PlatformTextSystem, PlatformWindow, Task,
    ThermalState, WindowAppearance, WindowParams,
};
use gpui_wgpu::WgpuContext;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

/// Keyboard layout stub for Node.js — we don't have OS keyboard layout info.
struct NodeKeyboardLayout;

impl PlatformKeyboardLayout for NodeKeyboardLayout {
    fn id(&self) -> &str {
        "us"
    }

    fn name(&self) -> &str {
        "US"
    }
}

#[derive(Default)]
struct NodePlatformCallbacks {
    quit: Option<Box<dyn FnMut()>>,
    reopen: Option<Box<dyn FnMut()>>,
    app_menu_action: Option<Box<dyn FnMut(&dyn Action)>>,
    will_open_app_menu: Option<Box<dyn FnMut()>>,
    validate_app_menu_command: Option<Box<dyn FnMut(&dyn Action) -> bool>>,
    keyboard_layout_change: Option<Box<dyn FnMut()>>,
}

pub struct NodePlatform {
    dispatcher: Arc<NodeDispatcher>,
    background_executor: BackgroundExecutor,
    foreground_executor: ForegroundExecutor,
    text_system: Arc<dyn PlatformTextSystem>,
    active_window: RefCell<Option<AnyWindowHandle>>,
    active_display: Rc<dyn PlatformDisplay>,
    callbacks: RefCell<NodePlatformCallbacks>,
    wgpu_context: RefCell<Option<WgpuContext>>,
    /// winit event loop — stored here for pump_app_events() in tick()
    event_loop: RefCell<Option<winit::event_loop::EventLoop<()>>>,
    /// Shared window state — allows tick() to access callbacks and dispatch events
    window_state: RefCell<Option<Rc<crate::platform::node_window::NodeWindowState>>>,
}

impl NodePlatform {
    pub fn new() -> Self {
        let dispatcher = Arc::new(NodeDispatcher::new());
        let background_executor = BackgroundExecutor::new(dispatcher.clone());
        let foreground_executor = ForegroundExecutor::new(dispatcher.clone());

        // Use CosmicTextSystem from gpui_wgpu for text rendering (same as web platform)
        // The argument is the default font family name for fallback
        let text_system = Arc::new(gpui_wgpu::CosmicTextSystem::new("Helvetica"));
        let text_system: Arc<dyn PlatformTextSystem> = text_system;

        let active_display: Rc<dyn PlatformDisplay> = Rc::new(NodeDisplay::new());

        Self {
            dispatcher,
            background_executor,
            foreground_executor,
            text_system,
            active_window: RefCell::new(None),
            active_display,
            callbacks: RefCell::new(NodePlatformCallbacks::default()),
            wgpu_context: RefCell::new(None),
            event_loop: RefCell::new(None),
            window_state: RefCell::new(None),
        }
    }

    /// Pump OS events and trigger frame render. Called from GpuixRenderer::tick().
    /// `force_render`: when true, tells GPUI to call Render::render() even if
    /// the window isn't marked dirty. Set to true when render() received a new tree.
    pub fn tick(&self, force_render: bool) {
        use gpui::{
            point, px, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ModifiersChangedEvent,
            MouseButton, MouseDownEvent, MouseExitEvent, MouseMoveEvent, MouseUpEvent,
            PlatformInput, RequestFrameOptions, ScrollDelta, ScrollWheelEvent, TouchPhase,
        };
        use std::sync::atomic::{AtomicU64, Ordering as AtOrd};
        use std::time::Duration;
        use winit::application::ApplicationHandler;
        use winit::event::{ElementState, WindowEvent};
        use winit::platform::pump_events::EventLoopExtPumpEvents;

        static TICK_COUNT: AtomicU64 = AtomicU64::new(0);
        let n = TICK_COUNT.fetch_add(1, AtOrd::Relaxed);
        // Always log force_render=true ticks (re-render events), plus periodic status
        if force_render || n < 3 || n % 5000 == 0 {
            eprintln!("[GPUIX-RUST] tick() #{n} force_render={force_render} has_event_loop={} has_window_state={}",
                self.event_loop.borrow().is_some(),
                self.window_state.borrow().is_some());
        }

        // Collect events from winit via pump_app_events with a proper handler.
        // After run_app_on_demand, the event loop is in "on demand" mode and
        // pump_app_events drives it without blocking.
        struct TickHandler {
            events: Vec<WindowEvent>,
        }

        impl ApplicationHandler for TickHandler {
            fn resumed(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {}

            fn window_event(
                &mut self,
                _event_loop: &winit::event_loop::ActiveEventLoop,
                _window_id: winit::window::WindowId,
                event: WindowEvent,
            ) {
                self.events.push(event);
            }
        }

        let mut handler = TickHandler { events: Vec::new() };

        if let Some(ref mut event_loop) = *self.event_loop.borrow_mut() {
            let _ = event_loop.pump_app_events(Some(Duration::ZERO), &mut handler);
        }

        let events = handler.events;

        // Process collected events
        if let Some(ref state) = *self.window_state.borrow() {
            for event in events {
                match event {
                    WindowEvent::CursorMoved { position, .. } => {
                        let scale = state.scale_factor.get();
                        let pos = gpui::Point::new(
                            px(position.x as f32 / scale),
                            px(position.y as f32 / scale),
                        );
                        state.mouse_position.set(pos);

                        let input = PlatformInput::MouseMove(MouseMoveEvent {
                            position: pos,
                            pressed_button: state.pressed_button.get(),
                            modifiers: state.modifiers.get(),
                        });
                        let mut cbs = state.callbacks.borrow_mut();
                        if let Some(ref mut cb) = cbs.input {
                            cb(input);
                        }
                    }
                    WindowEvent::MouseInput {
                        state: btn_state,
                        button,
                        ..
                    } => {
                        let gpui_button = match button {
                            winit::event::MouseButton::Left => MouseButton::Left,
                            winit::event::MouseButton::Right => MouseButton::Right,
                            winit::event::MouseButton::Middle => MouseButton::Middle,
                            _ => MouseButton::Left,
                        };
                        let pos = state.mouse_position.get();
                        let mods = state.modifiers.get();

                        let input = match btn_state {
                            ElementState::Pressed => {
                                state.pressed_button.set(Some(gpui_button));
                                let click_count =
                                    state.click_state.borrow_mut().register_click(pos);
                                PlatformInput::MouseDown(MouseDownEvent {
                                    button: gpui_button,
                                    position: pos,
                                    modifiers: mods,
                                    click_count,
                                    first_mouse: false,
                                })
                            }
                            ElementState::Released => {
                                state.pressed_button.set(None);
                                let click_count = state.click_state.borrow().current_count;
                                PlatformInput::MouseUp(MouseUpEvent {
                                    button: gpui_button,
                                    position: pos,
                                    modifiers: mods,
                                    click_count,
                                })
                            }
                        };
                        let mut cbs = state.callbacks.borrow_mut();
                        if let Some(ref mut cb) = cbs.input {
                            cb(input);
                        }
                    }
                    WindowEvent::MouseWheel { delta, phase, .. } => {
                        let pos = state.mouse_position.get();
                        let mods = state.modifiers.get();

                        let scroll_delta = match delta {
                            winit::event::MouseScrollDelta::LineDelta(x, y) => {
                                ScrollDelta::Lines(point(-x, -y))
                            }
                            winit::event::MouseScrollDelta::PixelDelta(d) => {
                                ScrollDelta::Pixels(point(px(-(d.x as f32)), px(-(d.y as f32))))
                            }
                        };

                        let touch_phase = match phase {
                            winit::event::TouchPhase::Started => TouchPhase::Started,
                            winit::event::TouchPhase::Moved => TouchPhase::Moved,
                            winit::event::TouchPhase::Ended => TouchPhase::Ended,
                            winit::event::TouchPhase::Cancelled => TouchPhase::Ended,
                        };

                        let input = PlatformInput::ScrollWheel(ScrollWheelEvent {
                            position: pos,
                            delta: scroll_delta,
                            modifiers: mods,
                            touch_phase,
                        });
                        let mut cbs = state.callbacks.borrow_mut();
                        if let Some(ref mut cb) = cbs.input {
                            cb(input);
                        }
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        // Modifiers are tracked via WindowEvent::ModifiersChanged
                        // which winit sends before KeyboardInput. No need to emit
                        // ModifiersChanged again here — that would cause duplicates.
                        let mods = state.modifiers.get();

                        // Toggle capslock state on CapsLock key press.
                        if matches!(
                            event.logical_key,
                            winit::keyboard::Key::Named(winit::keyboard::NamedKey::CapsLock)
                        ) && event.state == ElementState::Pressed
                        {
                            let current = state.capslock.get();
                            state.capslock.set(gpui::Capslock { on: !current.on });
                        }

                        let key = winit_key_to_gpui_key(&event.logical_key);

                        if is_modifier_only_key(&key) {
                            continue;
                        }

                        let key_char = compute_winit_key_char(&event, &key, &mods);

                        let keystroke = Keystroke {
                            modifiers: mods,
                            key,
                            key_char,
                        };

                        let input = match event.state {
                            ElementState::Pressed => PlatformInput::KeyDown(KeyDownEvent {
                                keystroke,
                                is_held: event.repeat,
                                prefer_character_input: false,
                            }),
                            ElementState::Released => {
                                PlatformInput::KeyUp(KeyUpEvent { keystroke })
                            }
                        };

                        let mut cbs = state.callbacks.borrow_mut();
                        if let Some(ref mut cb) = cbs.input {
                            cb(input);
                        }
                    }
                    WindowEvent::ModifiersChanged(mods_event) => {
                        let winit_state = mods_event.state();
                        let modifiers = Modifiers {
                            control: winit_state.contains(winit::keyboard::ModifiersState::CONTROL),
                            alt: winit_state.contains(winit::keyboard::ModifiersState::ALT),
                            shift: winit_state.contains(winit::keyboard::ModifiersState::SHIFT),
                            platform: winit_state.contains(winit::keyboard::ModifiersState::SUPER),
                            function: false,
                        };
                        state.modifiers.set(modifiers);

                        let mut cbs = state.callbacks.borrow_mut();
                        if let Some(ref mut cb) = cbs.input {
                            cb(PlatformInput::ModifiersChanged(ModifiersChangedEvent {
                                modifiers,
                                capslock: state.capslock.get(),
                            }));
                        }
                    }
                    WindowEvent::CursorLeft { .. } => {
                        let pos = state.mouse_position.get();
                        let mods = state.modifiers.get();

                        state.is_hovered.set(false);

                        let input = PlatformInput::MouseExited(MouseExitEvent {
                            position: pos,
                            pressed_button: state.pressed_button.get(),
                            modifiers: mods,
                        });
                        let mut cbs = state.callbacks.borrow_mut();
                        if let Some(ref mut cb) = cbs.input {
                            cb(input);
                        }
                        if let Some(ref mut cb) = cbs.hover_status_change {
                            cb(false);
                        }
                    }
                    WindowEvent::CursorEntered { .. } => {
                        state.is_hovered.set(true);
                        let mut cbs = state.callbacks.borrow_mut();
                        if let Some(ref mut cb) = cbs.hover_status_change {
                            cb(true);
                        }
                    }
                    WindowEvent::Resized(new_size) => {
                        let scale = state.scale_factor.get();
                        let lw = new_size.width as f32 / scale;
                        let lh = new_size.height as f32 / scale;

                        *state.bounds.borrow_mut() = gpui::Bounds {
                            origin: gpui::Point::default(),
                            size: gpui::Size {
                                width: px(lw),
                                height: px(lh),
                            },
                        };

                        state
                            .renderer
                            .borrow_mut()
                            .update_drawable_size(gpui::Size {
                                width: gpui::DevicePixels(new_size.width as i32),
                                height: gpui::DevicePixels(new_size.height as i32),
                            });

                        let mut cbs = state.callbacks.borrow_mut();
                        if let Some(ref mut cb) = cbs.resize {
                            cb(
                                gpui::Size {
                                    width: px(lw),
                                    height: px(lh),
                                },
                                scale,
                            );
                        }
                    }
                    WindowEvent::Focused(focused) => {
                        state.is_active.set(focused);
                        let mut cbs = state.callbacks.borrow_mut();
                        if let Some(ref mut cb) = cbs.active_status_change {
                            cb(focused);
                        }
                    }
                    WindowEvent::CloseRequested => {
                        let mut cbs = state.callbacks.borrow_mut();
                        let should_close = cbs.should_close.as_mut().map(|cb| cb()).unwrap_or(true);
                        if should_close {
                            if let Some(close_cb) = cbs.close.take() {
                                close_cb();
                            }
                        }
                    }
                    _ => {}
                }
            }

            // Trigger frame render — force_render=true when JS sent a new tree
            let mut cbs = state.callbacks.borrow_mut();
            if let Some(ref mut callback) = cbs.request_frame {
                callback(RequestFrameOptions {
                    require_presentation: true,
                    force_render,
                });
            }
        }

        // Drain dispatcher queue (foreground tasks + delayed runnables)
        self.dispatcher.drain_main_thread_queue();
    }
}

impl Platform for NodePlatform {
    fn background_executor(&self) -> BackgroundExecutor {
        self.background_executor.clone()
    }

    fn foreground_executor(&self) -> ForegroundExecutor {
        self.foreground_executor.clone()
    }

    fn text_system(&self) -> Arc<dyn PlatformTextSystem> {
        self.text_system.clone()
    }

    /// Non-blocking run: init wgpu synchronously, call the callback, return immediately.
    /// This is the key difference from MacPlatform which enters [NSApp run] and never returns.
    fn run(&self, on_finish_launching: Box<dyn 'static + FnOnce()>) {
        // Non-blocking run: just call the callback and return immediately.
        // wgpu context will be created lazily when the first window is opened.
        eprintln!("[GPUIX-RUST] NodePlatform::run() — non-blocking");

        // Call the finish_launching callback — this is where GPUI sets up the app
        on_finish_launching();

        // Return immediately — JS event loop continues
        eprintln!("[GPUIX-RUST] NodePlatform::run() returned — JS event loop is alive");
    }

    fn quit(&self) {
        log::info!("[gpuix] NodePlatform::quit() called");
        if let Some(ref mut callback) = self.callbacks.borrow_mut().quit {
            callback();
        }
    }

    fn restart(&self, _binary_path: Option<PathBuf>) {}

    fn activate(&self, _ignoring_other_apps: bool) {}

    fn hide(&self) {}

    fn hide_other_apps(&self) {}

    fn unhide_other_apps(&self) {}

    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        vec![self.active_display.clone()]
    }

    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        Some(self.active_display.clone())
    }

    fn active_window(&self) -> Option<AnyWindowHandle> {
        *self.active_window.borrow()
    }

    fn open_window(
        &self,
        handle: AnyWindowHandle,
        params: WindowParams,
    ) -> anyhow::Result<Box<dyn PlatformWindow>> {
        use crate::platform::node_window::NodeWindow;
        use winit::application::ApplicationHandler;
        use winit::platform::pump_events::EventLoopExtPumpEvents;

        eprintln!("[GPUIX-RUST] open_window() called");

        // Extract desired size from params
        let width = f32::from(params.bounds.size.width) as u32;
        let height = f32::from(params.bounds.size.height) as u32;
        let width = if width == 0 { 800 } else { width };
        let height = if height == 0 { 600 } else { height };
        eprintln!("[GPUIX-RUST] open_window: size={width}x{height}");

        // Set NSApplication to Regular policy BEFORE creating the event loop.
        // Node.js processes default to Background/Accessory policy (no Dock icon,
        // no windows visible). Must happen before EventLoop::new().
        #[cfg(target_os = "macos")]
        {
            use objc2_app_kit::NSApplication;
            use objc2_foundation::MainThreadMarker;

            if let Some(mtm) = MainThreadMarker::new() {
                let app = NSApplication::sharedApplication(mtm);
                app.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Regular);
            }
        }

        // Use pump_app_events exclusively (NOT run_app_on_demand + exit()).
        // run_app_on_demand + exit() puts the macOS event loop in a terminated
        // state where subsequent pump_app_events can't properly show windows.
        // pump_app_events delivers the initial resumed() event on the first call,
        // which is where we create the window, and it returns immediately.
        let mut event_loop = winit::event_loop::EventLoop::new()
            .map_err(|e| anyhow::anyhow!("Failed to create winit event loop: {e}"))?;

        // Handler that creates the window during the resumed() event
        struct WindowCreator {
            width: u32,
            height: u32,
            winit_window: Option<winit::window::Window>,
            created: bool,
        }

        impl ApplicationHandler for WindowCreator {
            fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
                if self.created {
                    return;
                }
                self.created = true;

                eprintln!("[GPUIX-RUST] resumed() callback — creating window");
                let attrs = winit::window::WindowAttributes::default()
                    .with_title("GPUIX")
                    .with_inner_size(winit::dpi::LogicalSize::new(self.width, self.height))
                    .with_visible(true)
                    .with_active(true);

                match event_loop.create_window(attrs) {
                    Ok(w) => {
                        eprintln!("[GPUIX-RUST] window created in resumed()");
                        self.winit_window = Some(w);
                    }
                    Err(e) => {
                        eprintln!("[GPUIX-RUST] failed to create window: {e}");
                    }
                }
            }

            fn window_event(
                &mut self,
                _event_loop: &winit::event_loop::ActiveEventLoop,
                _window_id: winit::window::WindowId,
                _event: winit::event::WindowEvent,
            ) {
                // Ignore events during initial creation
            }
        }

        let mut creator = WindowCreator {
            width,
            height,
            winit_window: None,
            created: false,
        };

        // Pump events until window is created. The first pump triggers resumed().
        // Use None timeout to process all pending events and return.
        eprintln!("[GPUIX-RUST] open_window: pumping events for window creation...");
        for attempt in 0..10 {
            let _ = event_loop.pump_app_events(None, &mut creator);
            if creator.winit_window.is_some() {
                eprintln!("[GPUIX-RUST] open_window: window created on pump attempt {attempt}");
                break;
            }
            eprintln!("[GPUIX-RUST] open_window: pump attempt {attempt}, no window yet");
        }

        let winit_window = creator
            .winit_window
            .ok_or_else(|| anyhow::anyhow!("Window was not created after pumping events"))?;
        eprintln!("[GPUIX-RUST] open_window: winit window obtained, creating NodeWindow...");

        // Create NodeWindow with the winit window
        let (window, window_state) = NodeWindow::new(
            handle,
            params,
            winit_window,
            &mut self.wgpu_context.borrow_mut(),
        )?;
        eprintln!("[GPUIX-RUST] open_window: NodeWindow created successfully");

        // Store event loop and window state for tick()
        *self.event_loop.borrow_mut() = Some(event_loop);
        *self.window_state.borrow_mut() = Some(window_state);
        *self.active_window.borrow_mut() = Some(handle);

        // Bring app to front
        #[cfg(target_os = "macos")]
        {
            use objc2_app_kit::NSApplication;
            use objc2_foundation::MainThreadMarker;

            if let Some(mtm) = MainThreadMarker::new() {
                let app = NSApplication::sharedApplication(mtm);
                #[allow(deprecated)]
                app.activateIgnoringOtherApps(true);
            }
        }

        Ok(Box::new(window))
    }

    fn window_appearance(&self) -> WindowAppearance {
        WindowAppearance::Dark // Default to dark in Node.js context
    }

    fn open_url(&self, _url: &str) {}

    fn on_open_urls(&self, _callback: Box<dyn FnMut(Vec<String>)>) {}

    fn register_url_scheme(&self, _url: &str) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }

    fn prompt_for_paths(
        &self,
        _options: PathPromptOptions,
    ) -> oneshot::Receiver<Result<Option<Vec<PathBuf>>>> {
        let (tx, rx) = oneshot::channel();
        tx.send(Err(anyhow::anyhow!(
            "prompt_for_paths is not supported in Node.js"
        )))
        .ok();
        rx
    }

    fn prompt_for_new_path(
        &self,
        _directory: &Path,
        _suggested_name: Option<&str>,
    ) -> oneshot::Receiver<Result<Option<PathBuf>>> {
        let (tx, rx) = oneshot::channel();
        tx.send(Err(anyhow::anyhow!(
            "prompt_for_new_path is not supported in Node.js"
        )))
        .ok();
        rx
    }

    fn can_select_mixed_files_and_dirs(&self) -> bool {
        false
    }

    fn reveal_path(&self, _path: &Path) {}

    fn open_with_system(&self, _path: &Path) {}

    fn on_quit(&self, callback: Box<dyn FnMut()>) {
        self.callbacks.borrow_mut().quit = Some(callback);
    }

    fn on_reopen(&self, callback: Box<dyn FnMut()>) {
        self.callbacks.borrow_mut().reopen = Some(callback);
    }

    fn set_menus(&self, _menus: Vec<Menu>, _keymap: &Keymap) {}

    fn set_dock_menu(&self, _menu: Vec<MenuItem>, _keymap: &Keymap) {}

    fn on_app_menu_action(&self, callback: Box<dyn FnMut(&dyn Action)>) {
        self.callbacks.borrow_mut().app_menu_action = Some(callback);
    }

    fn on_will_open_app_menu(&self, callback: Box<dyn FnMut()>) {
        self.callbacks.borrow_mut().will_open_app_menu = Some(callback);
    }

    fn on_validate_app_menu_command(&self, callback: Box<dyn FnMut(&dyn Action) -> bool>) {
        self.callbacks.borrow_mut().validate_app_menu_command = Some(callback);
    }

    fn app_path(&self) -> Result<PathBuf> {
        Err(anyhow::anyhow!("app_path is not available in Node.js"))
    }

    fn path_for_auxiliary_executable(&self, _name: &str) -> Result<PathBuf> {
        Err(anyhow::anyhow!(
            "path_for_auxiliary_executable is not available in Node.js"
        ))
    }

    fn set_cursor_style(&self, _style: CursorStyle) {
        // No-op in Node.js — cursor is managed by the OS/winit
    }

    fn should_auto_hide_scrollbars(&self) -> bool {
        true
    }

    fn thermal_state(&self) -> ThermalState {
        ThermalState::Nominal
    }

    fn on_thermal_state_change(&self, _callback: Box<dyn FnMut()>) {}

    fn read_from_clipboard(&self) -> Option<ClipboardItem> {
        None
    }

    fn write_to_clipboard(&self, _item: ClipboardItem) {}

    fn read_from_find_pasteboard(&self) -> Option<ClipboardItem> {
        None
    }

    fn write_to_find_pasteboard(&self, _item: ClipboardItem) {}

    fn write_credentials(&self, _url: &str, _username: &str, _password: &[u8]) -> Task<Result<()>> {
        Task::ready(Err(anyhow::anyhow!(
            "credential storage is not available in Node.js"
        )))
    }

    fn read_credentials(&self, _url: &str) -> Task<Result<Option<(String, Vec<u8>)>>> {
        Task::ready(Ok(None))
    }

    fn delete_credentials(&self, _url: &str) -> Task<Result<()>> {
        Task::ready(Err(anyhow::anyhow!(
            "credential storage is not available in Node.js"
        )))
    }

    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> {
        Box::new(NodeKeyboardLayout)
    }

    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> {
        Rc::new(DummyKeyboardMapper)
    }

    fn on_keyboard_layout_change(&self, callback: Box<dyn FnMut()>) {
        self.callbacks.borrow_mut().keyboard_layout_change = Some(callback);
    }
}

/// Convert winit's Key enum to the GPUI key string.
/// Equivalent of gpui_web's dom_key_to_gpui_key but for winit's types.
fn winit_key_to_gpui_key(key: &winit::keyboard::Key) -> String {
    use winit::keyboard::{Key, NamedKey};
    match key {
        Key::Named(named) => match named {
            NamedKey::Enter => "enter".to_string(),
            NamedKey::Backspace => "backspace".to_string(),
            NamedKey::Tab => "tab".to_string(),
            NamedKey::Escape => "escape".to_string(),
            NamedKey::Delete => "delete".to_string(),
            NamedKey::Space => "space".to_string(),
            NamedKey::ArrowLeft => "left".to_string(),
            NamedKey::ArrowRight => "right".to_string(),
            NamedKey::ArrowUp => "up".to_string(),
            NamedKey::ArrowDown => "down".to_string(),
            NamedKey::Home => "home".to_string(),
            NamedKey::End => "end".to_string(),
            NamedKey::PageUp => "pageup".to_string(),
            NamedKey::PageDown => "pagedown".to_string(),
            NamedKey::Insert => "insert".to_string(),
            NamedKey::Control => "control".to_string(),
            NamedKey::Alt => "alt".to_string(),
            NamedKey::Shift => "shift".to_string(),
            NamedKey::Super | NamedKey::Meta => "platform".to_string(),
            NamedKey::CapsLock => "capslock".to_string(),
            NamedKey::F1 => "f1".to_string(),
            NamedKey::F2 => "f2".to_string(),
            NamedKey::F3 => "f3".to_string(),
            NamedKey::F4 => "f4".to_string(),
            NamedKey::F5 => "f5".to_string(),
            NamedKey::F6 => "f6".to_string(),
            NamedKey::F7 => "f7".to_string(),
            NamedKey::F8 => "f8".to_string(),
            NamedKey::F9 => "f9".to_string(),
            NamedKey::F10 => "f10".to_string(),
            NamedKey::F11 => "f11".to_string(),
            NamedKey::F12 => "f12".to_string(),
            NamedKey::F13 => "f13".to_string(),
            NamedKey::F14 => "f14".to_string(),
            NamedKey::F15 => "f15".to_string(),
            NamedKey::F16 => "f16".to_string(),
            NamedKey::F17 => "f17".to_string(),
            NamedKey::F18 => "f18".to_string(),
            NamedKey::F19 => "f19".to_string(),
            NamedKey::F20 => "f20".to_string(),
            _ => format!("{named:?}").to_lowercase(),
        },
        Key::Character(c) => c.to_lowercase(),
        Key::Unidentified(_) => "unidentified".to_string(),
        Key::Dead(_) => "dead".to_string(),
    }
}

fn is_modifier_only_key(key: &str) -> bool {
    matches!(key, "control" | "alt" | "shift" | "platform" | "capslock")
}

/// Compute the key_char for a winit KeyEvent.
/// When platform/control modifiers are held, key_char is None (the key is
/// being used as a keybinding, not character input).
fn compute_winit_key_char(
    event: &winit::event::KeyEvent,
    gpui_key: &str,
    modifiers: &gpui::Modifiers,
) -> Option<String> {
    if modifiers.platform || modifiers.control {
        return None;
    }

    if is_modifier_only_key(gpui_key) {
        return None;
    }

    if gpui_key == "space" {
        return Some(" ".to_string());
    }

    // winit provides the text that would be produced by this key event.
    // Only return printable characters — filter out control chars like
    // "\r" (Enter), "\t" (Tab), "\x1b" (Escape) which winit includes
    // in KeyEvent.text but are not text input.
    if let Some(text) = &event.text {
        let s = text.as_str();
        let mut chars = s.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            if !ch.is_control() {
                return Some(s.to_string());
            }
        } else if s.len() > 1 && !s.chars().any(|c| c.is_control()) {
            // Multi-char text (e.g. composed input) — only if all printable
            return Some(s.to_string());
        }
    }

    None
}
