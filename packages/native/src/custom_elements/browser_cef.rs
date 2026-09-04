use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::{c_char, c_void, CString},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicPtr, Ordering},
        Arc, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

use cef::{args::Args, *};
use objc::{
    msg_send,
    runtime::{Class, Object, Sel, BOOL, NO, YES},
    sel, sel_impl,
};
use raw_window_handle::RawWindowHandle;
#[cfg(feature = "cef-development-overrides")]
use std::ffi::CStr;

use super::{
    emit_state, emit_tagged_value, BrowserCommand, BrowserConfig, BrowserStateEvent, EventCallback,
};

static HANDLING_SEND_EVENT: AtomicBool = AtomicBool::new(false);
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
static DEBUG_ENABLED: OnceLock<bool> = OnceLock::new();
const MAX_BROWSER_CREATION_ATTEMPTS: u8 = 60;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const CLEAR_DATA_TIMEOUT: Duration = Duration::from_secs(10);
const BROWSER_CREATION_ERROR: &str = "CEF could not create a Chromium browser child view";
const NS_WINDOW_TABBING_MODE_DISALLOWED: isize = 2;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopDefaultMode: *const c_void;
    fn CFRunLoopRunInMode(
        mode: *const c_void,
        seconds: f64,
        return_after_source_handled: u8,
    ) -> i32;
}

thread_local! {
    static PROCESS: RefCell<CefProcess> = RefCell::new(CefProcess::default());
}

struct BrowserContextEntry {
    context: RequestContext,
    references: usize,
    pending_references: usize,
    ephemeral: bool,
}

struct BrowserEntry {
    browser: Browser,
    profile_key: String,
}

#[derive(Default)]
struct CefProcess {
    attempted: bool,
    initialized: bool,
    ready: Arc<AtomicBool>,
    error: Option<String>,
    root_cache_path: PathBuf,
    contexts: HashMap<String, BrowserContextEntry>,
    browsers: HashMap<i32, BrowserEntry>,
    pending_browser_creations: usize,
    active_browser_windows: usize,
}

pub(super) fn initialize(root_cache_path: Option<&str>) {
    PROCESS.with(|process| {
        let mut process = process.borrow_mut();
        if process.attempted {
            return;
        }
        process.attempted = true;
        if let Err(error) = initialize_process(&mut process, root_cache_path) {
            eprintln!("[gpuix] Chromium initialization failed: {error}");
            process.error = Some(error);
        }
    });
}

pub(super) fn available() -> bool {
    PROCESS.with(|process| process.borrow().initialized)
}

pub(super) fn initialization_error() -> Option<String> {
    PROCESS.with(|process| process.borrow().error.clone())
}

pub(super) fn do_message_loop_work() {
    let initialized = PROCESS.with(|process| process.borrow().initialized);
    if initialized {
        cef::do_message_loop_work();
    }
}

pub(super) fn shutdown() -> Result<(), String> {
    let initialized = PROCESS.with(|process| process.borrow().initialized);
    if !initialized {
        return Ok(());
    }

    SHUTTING_DOWN.store(true, Ordering::Release);
    let browsers = PROCESS.with(|process| {
        process
            .borrow()
            .browsers
            .values()
            .map(|entry| entry.browser.clone())
            .collect::<Vec<_>>()
    });
    for browser in &browsers {
        if let Some(host) = browser.host() {
            host.close_dev_tools();
            host.close_browser(1);
        }
    }
    if debug_enabled() {
        let (pending, windows) = PROCESS.with(|process| {
            let process = process.borrow();
            (
                process.pending_browser_creations,
                process.active_browser_windows,
            )
        });
        eprintln!(
            "[gpuix-cef] shutting down {} browser(s), {pending} pending creation(s), {windows} Views window(s)",
            browsers.len()
        );
    }
    drop(browsers);

    let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
    while PROCESS.with(|process| {
        let process = process.borrow();
        !process.browsers.is_empty()
            || process.pending_browser_creations > 0
            || process.active_browser_windows > 0
    }) && Instant::now() < deadline
    {
        cef::do_message_loop_work();
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.001, 1);
        }
        thread::sleep(Duration::from_millis(1));
    }

    let (remaining, pending, windows) = PROCESS.with(|process| {
        let process = process.borrow();
        (
            process.browsers.len(),
            process.pending_browser_creations,
            process.active_browser_windows,
        )
    });
    if remaining > 0 || pending > 0 || windows > 0 {
        let error = format!(
            "Chromium shutdown timed out with {remaining} browser(s), {pending} pending creation(s), and {windows} Views window(s)"
        );
        PROCESS.with(|process| process.borrow_mut().error = Some(error.clone()));
        SHUTTING_DOWN.store(false, Ordering::Release);
        return Err(error);
    }

    PROCESS.with(|process| process.borrow_mut().contexts.clear());
    cef::shutdown();
    PROCESS.with(|process| {
        let mut process = process.borrow_mut();
        process.initialized = false;
        process.ready.store(false, Ordering::Release);
    });
    SHUTTING_DOWN.store(false, Ordering::Release);
    Ok(())
}

fn debug_enabled() -> bool {
    *DEBUG_ENABLED.get_or_init(|| std::env::var_os("GPUIX_CEF_DEBUG").is_some())
}

fn initialize_process(
    process: &mut CefProcess,
    configured_root: Option<&str>,
) -> Result<(), String> {
    let runtime = resolve_runtime()?;
    load_cef_framework(&runtime.framework_binary)?;
    register_cef_app_protocol()?;
    validate_cef_api()?;

    let root_cache_path = browser_root_cache_path(configured_root)?;
    fs::create_dir_all(&root_cache_path).map_err(|error| {
        format!(
            "Could not create Chromium profile root {}: {error}",
            root_cache_path.display()
        )
    })?;
    let root_cache_path = root_cache_path.canonicalize().map_err(|error| {
        format!(
            "Could not resolve Chromium profile root {}: {error}",
            root_cache_path.display()
        )
    })?;

    let args = Args::new();
    let ready = process.ready.clone();
    let handler = GpuixBrowserProcessHandler::new(ready);
    let mut app = GpuixCefApp::new(handler);
    let settings = Settings {
        no_sandbox: if cef_sandbox_enabled() { 0 } else { 1 },
        browser_subprocess_path: cef_path_string(&runtime.helper_executable)?,
        framework_dir_path: cef_path_string(&runtime.framework_directory)?,
        main_bundle_path: runtime
            .main_bundle
            .as_deref()
            .map(cef_path_string)
            .transpose()?
            .unwrap_or_default(),
        external_message_pump: 0,
        root_cache_path: cef_path_string(&root_cache_path)?,
        persist_session_cookies: 1,
        log_file: cef_path_string(&root_cache_path.join("chromium.log"))?,
        log_severity: LogSeverity::WARNING,
        resources_dir_path: cef_path_string(&runtime.framework_directory.join("Resources"))?,
        locales_dir_path: cef_path_string(&runtime.framework_directory.join("Resources"))?,
        remote_debugging_port: remote_debugging_port(),
        use_views_default_popup: 1,
        ..Default::default()
    };

    let result = cef::initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if result != 1 {
        return Err(format!("cef_initialize returned {result}"));
    }

    process.initialized = true;
    process.root_cache_path = root_cache_path;
    Ok(())
}

struct CefRuntimePaths {
    framework_directory: PathBuf,
    framework_binary: PathBuf,
    helper_executable: PathBuf,
    main_bundle: Option<PathBuf>,
}

fn resolve_runtime() -> Result<CefRuntimePaths, String> {
    let main_bundle = main_app_bundle();
    #[cfg(not(feature = "cef-development-overrides"))]
    if main_bundle.is_none() {
        return Err(
            "Native Chromium requires a macOS application bundle; run `bun run build` and launch the generated .app"
                .to_string(),
        );
    }
    let roots = main_bundle
        .iter()
        .map(|bundle| bundle.join("Contents/Frameworks"))
        .collect::<Vec<_>>();
    #[cfg(feature = "cef-development-overrides")]
    let roots = {
        let mut roots = roots;
        if let Some(configured) = std::env::var_os("GPUIX_CEF_DIR") {
            roots.insert(0, PathBuf::from(configured));
        }
        if let Some(module) = current_module_directory() {
            roots.push(module.join("cef"));
        }
        if let Some(directory) = cef::sys::get_cef_dir() {
            roots.push(directory);
        }
        roots
    };

    let framework_directory = roots
        .iter()
        .find_map(|root| {
            let direct = root.join("Chromium Embedded Framework.framework");
            if direct.is_dir() {
                Some(direct)
            } else if root
                .file_name()
                .is_some_and(|name| name == "Chromium Embedded Framework.framework")
                && root.is_dir()
            {
                Some(root.clone())
            } else {
                None
            }
        })
        .ok_or_else(|| {
            "Chromium Embedded Framework.framework was not found in the application bundle"
                .to_string()
        })?;
    let framework_directory = framework_directory.canonicalize().map_err(|error| {
        format!(
            "Could not resolve Chromium framework {}: {error}",
            framework_directory.display()
        )
    })?;
    let framework_binary = framework_directory.join("Chromium Embedded Framework");
    if !framework_binary.is_file() {
        return Err(format!(
            "Chromium framework binary is missing at {}",
            framework_binary.display()
        ));
    }

    let helper_executable = resolve_helper(&roots, main_bundle.as_deref())?;
    Ok(CefRuntimePaths {
        framework_directory,
        framework_binary,
        helper_executable,
        main_bundle,
    })
}

fn resolve_helper(roots: &[PathBuf], main_bundle: Option<&Path>) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    #[cfg(feature = "cef-development-overrides")]
    if let Some(configured) = std::env::var_os("GPUIX_CEF_HELPER_PATH") {
        candidates.push(PathBuf::from(configured));
    }
    if let Some(bundle) = main_bundle {
        let app_name = bundle
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("GPUix");
        candidates.push(bundle.join("Contents/Frameworks").join(format!(
            "{app_name} Helper.app/Contents/MacOS/{app_name} Helper"
        )));
        candidates.push(bundle.join(
            "Contents/Frameworks/GPUix Chromium Helper.app/Contents/MacOS/GPUix Chromium Helper",
        ));
    }
    for root in roots {
        candidates
            .push(root.join("GPUix Chromium Helper.app/Contents/MacOS/GPUix Chromium Helper"));
        candidates.push(root.join("gpuix-cef-helper"));
    }

    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.canonicalize().ok())
        .ok_or_else(|| "GPUix Chromium Helper was not found in the application bundle".to_string())
}

fn load_cef_framework(path: &Path) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt as _;

    let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        format!(
            "Chromium framework path contains a NUL byte: {}",
            path.display()
        )
    })?;
    if cef::load_library(Some(unsafe { &*path.as_ptr() })) != 1 {
        return Err("Could not load Chromium Embedded Framework".to_string());
    }
    Ok(())
}

fn validate_cef_api() -> Result<(), String> {
    let expected = cef::sys::CEF_API_VERSION_LAST;
    let hash = api_hash(expected, 0);
    if hash.is_null() {
        return Err(format!(
            "Chromium does not support the compiled CEF API version {expected}"
        ));
    }
    let configured = api_version();
    if configured != expected {
        return Err(format!(
            "Chromium configured CEF API version {configured}, expected {expected}"
        ));
    }
    Ok(())
}

fn browser_root_cache_path(configured: Option<&str>) -> Result<PathBuf, String> {
    if let Some(path) = configured.filter(|path| !path.is_empty()) {
        return absolute_path(PathBuf::from(path));
    }
    if let Some(path) = std::env::var_os("GPUIX_BROWSER_ROOT_CACHE") {
        let path = PathBuf::from(path);
        return absolute_path(path);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is unavailable and GPUIX_BROWSER_ROOT_CACHE is unset".to_string())?;
    Ok(home.join("Library/Application Support/GPUix/Browser"))
}

fn absolute_path(path: PathBuf) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path)
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(path))
            .map_err(|error| format!("Could not resolve browser data path: {error}"))
    }
}

fn cef_path_string(path: &Path) -> Result<CefString, String> {
    path.to_str()
        .map(CefString::from)
        .ok_or_else(|| format!("Path is not valid UTF-8: {}", path.display()))
}

fn cef_sandbox_enabled() -> bool {
    #[cfg(feature = "cef-development-overrides")]
    if std::env::var("GPUIX_CEF_SANDBOX").as_deref() == Ok("0") {
        return false;
    }
    true
}

fn remote_debugging_port() -> i32 {
    #[cfg(feature = "cef-development-overrides")]
    {
        return std::env::var("GPUIX_CEF_REMOTE_DEBUGGING_PORT")
            .ok()
            .and_then(|value| value.parse::<i32>().ok())
            .filter(|port| (1024..=65535).contains(port))
            .unwrap_or(0);
    }
    #[cfg(not(feature = "cef-development-overrides"))]
    0
}

fn main_app_bundle() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    executable.ancestors().find_map(|ancestor| {
        ancestor
            .extension()
            .is_some_and(|extension| extension == "app")
            .then(|| ancestor.to_path_buf())
    })
}

#[cfg(feature = "cef-development-overrides")]
#[repr(C)]
struct DlInfo {
    file_name: *const c_char,
    base_address: *mut c_void,
    symbol_name: *const c_char,
    symbol_address: *mut c_void,
}

#[cfg(feature = "cef-development-overrides")]
#[link(name = "System")]
extern "C" {
    fn dladdr(address: *const c_void, info: *mut DlInfo) -> i32;
}

#[cfg(feature = "cef-development-overrides")]
fn current_module_directory() -> Option<PathBuf> {
    let mut info = DlInfo {
        file_name: std::ptr::null(),
        base_address: std::ptr::null_mut(),
        symbol_name: std::ptr::null(),
        symbol_address: std::ptr::null_mut(),
    };
    let address = current_module_directory as *const () as *const c_void;
    if unsafe { dladdr(address, &mut info) } == 0 || info.file_name.is_null() {
        return None;
    }
    let file_name = unsafe { CStr::from_ptr(info.file_name) };
    PathBuf::from(file_name.to_string_lossy().into_owned())
        .parent()
        .map(Path::to_path_buf)
}

#[repr(C)]
struct ObjcProtocol(c_void);

#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> *const Class;
    fn objc_getProtocol(name: *const c_char) -> *mut ObjcProtocol;
    fn objc_allocateProtocol(name: *const c_char) -> *mut ObjcProtocol;
    fn objc_registerProtocol(protocol: *mut ObjcProtocol);
    fn protocol_addProtocol(protocol: *mut ObjcProtocol, addition: *mut ObjcProtocol);
    fn class_addProtocol(class: *const Class, protocol: *mut ObjcProtocol) -> BOOL;
    fn class_addMethod(
        class: *const Class,
        selector: Sel,
        implementation: *const c_void,
        types: *const c_char,
    ) -> BOOL;
}

extern "C" fn is_handling_send_event(_: *mut Object, _: Sel) -> BOOL {
    if HANDLING_SEND_EVENT.load(Ordering::Acquire) {
        YES
    } else {
        NO
    }
}

extern "C" fn set_handling_send_event(_: *mut Object, _: Sel, handling: BOOL) {
    HANDLING_SEND_EVENT.store(handling == YES, Ordering::Release);
}

fn register_cef_app_protocol() -> Result<(), String> {
    let control_name = CString::new("CrAppControlProtocol").expect("static protocol name");
    let control_protocol = unsafe { objc_getProtocol(control_name.as_ptr()) };
    if control_protocol.is_null() {
        return Err("CEF did not register CrAppControlProtocol".to_string());
    }
    let protocol_name = CString::new("CefAppProtocol").expect("static protocol name");
    let mut protocol = unsafe { objc_getProtocol(protocol_name.as_ptr()) };
    if protocol.is_null() {
        protocol = unsafe { objc_allocateProtocol(protocol_name.as_ptr()) };
        if protocol.is_null() {
            return Err("Could not allocate CefAppProtocol".to_string());
        }
        unsafe {
            protocol_addProtocol(protocol, control_protocol);
            objc_registerProtocol(protocol);
        }
    }

    let class_name = CString::new("GPUIApplication").expect("static application class name");
    let app_class = unsafe { objc_getClass(class_name.as_ptr()) };
    if app_class.is_null() {
        return Err("GPUIApplication is not registered".to_string());
    }
    unsafe {
        class_addMethod(
            app_class,
            sel!(isHandlingSendEvent),
            is_handling_send_event as *const c_void,
            c"c@:".as_ptr(),
        );
        class_addMethod(
            app_class,
            sel!(setHandlingSendEvent:),
            set_handling_send_event as *const c_void,
            c"v@:c".as_ptr(),
        );
        class_addProtocol(app_class, protocol);
        let app: *mut Object = msg_send![app_class, sharedApplication];
        if app.is_null() {
            return Err("GPUIApplication is unavailable".to_string());
        }
    }
    Ok(())
}

cef::wrap_browser_process_handler! {
    struct GpuixBrowserProcessHandler {
        ready: Arc<AtomicBool>,
    }

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            if debug_enabled() {
                eprintln!("[gpuix-cef] context initialized");
            }
            self.ready.store(true, Ordering::Release);
        }
    }
}

cef::wrap_app! {
    struct GpuixCefApp {
        browser_process_handler: BrowserProcessHandler,
    }

    impl App {
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(self.browser_process_handler.clone())
        }
    }
}

#[derive(Clone)]
struct ClearDataState {
    context: RequestContext,
    callback: Option<EventCallback>,
    id: u64,
    generation: u64,
    serial: u64,
    done: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}

#[derive(Clone, Copy)]
enum ClearDataStage {
    Cache,
    Certificates,
    Authentication,
    Complete,
}

struct PendingBrowserCommand {
    serial: u64,
    deadline: Instant,
    done: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}

#[derive(Clone)]
struct PendingBrowserCreation {
    profile_key: String,
    active: Arc<AtomicBool>,
    window_active: Arc<AtomicBool>,
}

struct RetiringBrowser {
    browser: Option<Browser>,
    browser_view: BrowserView,
    window: Window,
    closing: Arc<AtomicBool>,
}

fn advance_clear_data(state: ClearDataState, stage: ClearDataStage) {
    if state.cancelled.load(Ordering::Acquire) {
        return;
    }
    let context = state.context.clone();
    match stage {
        ClearDataStage::Cache => {
            let mut completion =
                GpuixClearDataCompletionCallback::new(state, ClearDataStage::Certificates);
            context.clear_http_cache(Some(&mut completion));
        }
        ClearDataStage::Certificates => {
            let mut completion =
                GpuixClearDataCompletionCallback::new(state, ClearDataStage::Authentication);
            context.clear_certificate_exceptions(Some(&mut completion));
        }
        ClearDataStage::Authentication => {
            let mut completion =
                GpuixClearDataCompletionCallback::new(state, ClearDataStage::Complete);
            context.clear_http_auth_credentials(Some(&mut completion));
        }
        ClearDataStage::Complete => {
            if !state.done.swap(true, Ordering::AcqRel) {
                emit_state(
                    &state.callback,
                    state.id,
                    state.generation,
                    BrowserStateEvent {
                        command_serial: Some(state.serial),
                        ..Default::default()
                    },
                );
            }
        }
    }
}

fn begin_clear_data(state: ClearDataState) {
    if let Some(cookie_manager) = state.context.cookie_manager(None) {
        let mut completion = GpuixDeleteCookiesCallback::new(state.clone());
        if cookie_manager.delete_cookies(None, None, Some(&mut completion)) != 0 {
            return;
        }
    }
    advance_clear_data(state, ClearDataStage::Cache);
}

cef::wrap_delete_cookies_callback! {
    struct GpuixDeleteCookiesCallback {
        state: ClearDataState,
    }

    impl DeleteCookiesCallback {
        fn on_complete(&self, _num_deleted: i32) {
            advance_clear_data(self.state.clone(), ClearDataStage::Cache);
        }
    }
}

cef::wrap_completion_callback! {
    struct GpuixClearDataCompletionCallback {
        state: ClearDataState,
        next: ClearDataStage,
    }

    impl CompletionCallback {
        fn on_complete(&self) {
            advance_clear_data(self.state.clone(), self.next);
        }
    }
}

pub(super) struct BrowserRuntime {
    browser: Option<Browser>,
    browser_view: Option<BrowserView>,
    window: Option<Window>,
    client: Option<Client>,
    request_context: Option<RequestContext>,
    profile_key: Option<String>,
    generation: Option<u64>,
    parent_view: *mut c_void,
    host_window: Option<Arc<AtomicPtr<c_void>>>,
    pending_creation: Option<PendingBrowserCreation>,
    events_enabled: Option<Arc<AtomicBool>>,
    closing: Option<Arc<AtomicBool>>,
    native_view: *mut c_void,
    last_bounds: Option<(f32, f32, f32, f32)>,
    last_visible: Option<bool>,
    last_command_serial: u64,
    last_retry_serial: u64,
    pending_command: Option<PendingBrowserCommand>,
    last_state: BrowserStateEvent,
    initialization_error_reported: bool,
    creation_attempts: u8,
    creation_failed: bool,
}

impl Default for BrowserRuntime {
    fn default() -> Self {
        Self {
            browser: None,
            browser_view: None,
            window: None,
            client: None,
            request_context: None,
            profile_key: None,
            generation: None,
            parent_view: std::ptr::null_mut(),
            host_window: None,
            pending_creation: None,
            events_enabled: None,
            closing: None,
            native_view: std::ptr::null_mut(),
            last_bounds: None,
            last_visible: None,
            last_command_serial: 0,
            last_retry_serial: 0,
            pending_command: None,
            last_state: BrowserStateEvent::default(),
            initialization_error_reported: false,
            creation_attempts: 0,
            creation_failed: false,
        }
    }
}

impl BrowserRuntime {
    pub(super) fn update(
        &mut self,
        config: &BrowserConfig,
        bounds: gpui::Bounds<gpui::Pixels>,
        window: &mut gpui::Window,
        callback: &Option<EventCallback>,
        id: u64,
    ) -> Result<(), String> {
        if debug_enabled() {
            eprintln!(
                "[gpuix-cef] browser update source={} ready={} existing={}",
                config.source,
                PROCESS.with(|process| process.borrow().ready.load(Ordering::Acquire)),
                self.browser.is_some()
            );
        }
        if !available() {
            if !self.initialization_error_reported && !config.source.is_empty() {
                self.initialization_error_reported = true;
                let error = PROCESS.with(|process| {
                    process
                        .borrow()
                        .error
                        .clone()
                        .unwrap_or_else(|| "Chromium is not initialized".to_string())
                });
                return Err(error);
            }
            return Ok(());
        }

        let profile_key = format!(
            "{}:{}:{}",
            config.profile_id, config.profile_path, config.incognito
        );
        if self.profile_key.as_deref() != Some(&profile_key)
            || self.generation != Some(config.generation)
        {
            self.retire_active_browser();
            self.profile_key = Some(profile_key);
            self.generation = Some(config.generation);
        }
        let logical_bounds = (
            f32::from(bounds.origin.x),
            f32::from(bounds.origin.y),
            f32::from(bounds.size.width).max(1.0),
            f32::from(bounds.size.height).max(1.0),
        );
        let latest_command_serial = config
            .commands
            .last()
            .map(|command| command.serial)
            .unwrap_or_default();
        if self.browser_view.is_none() && latest_command_serial > self.last_retry_serial {
            self.last_retry_serial = latest_command_serial;
            self.creation_attempts = 0;
            self.creation_failed = false;
        }
        if self.browser_view.is_none()
            && !self.creation_failed
            && !config.source.is_empty()
            && PROCESS.with(|process| process.borrow().ready.load(Ordering::Acquire))
        {
            match self.create(config, logical_bounds, window, callback, id) {
                Ok(()) => {
                    self.creation_attempts = 0;
                    self.creation_failed = false;
                }
                Err(error) if error == BROWSER_CREATION_ERROR => {
                    self.creation_attempts = self.creation_attempts.saturating_add(1);
                    if self.creation_attempts >= MAX_BROWSER_CREATION_ATTEMPTS {
                        self.creation_failed = true;
                        return Err(error);
                    }
                    return Ok(());
                }
                Err(error) => {
                    self.creation_failed = true;
                    return Err(error);
                }
            }
        }
        if let Err(error) = self.adopt_created_browser(config, callback, id) {
            let profile_key = self.profile_key.clone();
            self.retire_active_browser();
            self.profile_key = profile_key;
            self.creation_failed = true;
            return Err(error);
        }

        let host_window = self
            .host_window
            .as_ref()
            .map(|window| window.load(Ordering::Acquire))
            .unwrap_or(std::ptr::null_mut());
        if self.last_bounds != Some(logical_bounds) {
            set_browser_window_bounds(self.parent_view, host_window, logical_bounds);
            self.last_bounds = Some(logical_bounds);
        }
        if self.last_visible != Some(config.visible) {
            if !config.visible {
                restore_parent_first_responder(self.parent_view, self.native_view);
            }
            self.last_visible = Some(config.visible);
        }
        set_browser_window_visible(
            self.parent_view,
            host_window,
            self.window.as_ref(),
            config.visible,
        );
        if self.browser.is_none() {
            return Ok(());
        }
        if let Some(pending) = self.pending_command.as_ref() {
            if pending.done.load(Ordering::Acquire) {
                self.last_command_serial = self.last_command_serial.max(pending.serial);
                self.pending_command = None;
            } else if Instant::now() >= pending.deadline {
                let serial = pending.serial;
                pending.cancelled.store(true, Ordering::Release);
                self.last_command_serial = self.last_command_serial.max(serial);
                self.pending_command = None;
                emit_state(
                    callback,
                    id,
                    config.generation,
                    BrowserStateEvent {
                        command_serial: Some(serial),
                        ..Default::default()
                    },
                );
                return Err(format!(
                    "Chromium clearData command {serial} timed out after {} seconds",
                    CLEAR_DATA_TIMEOUT.as_secs()
                ));
            }
        }
        if self.pending_command.is_none() {
            if let Some(command) = config
                .commands
                .iter()
                .find(|command| command.serial > self.last_command_serial)
            {
                if self.run_command(command, callback, id)? {
                    self.last_command_serial = command.serial;
                }
            }
        }
        self.report_state(callback, id, config.generation);
        Ok(())
    }

    fn create(
        &mut self,
        config: &BrowserConfig,
        bounds: (f32, f32, f32, f32),
        window: &mut gpui::Window,
        callback: &Option<EventCallback>,
        id: u64,
    ) -> Result<(), String> {
        let parent_view = parent_view(window)?;
        let profile_key = self
            .profile_key
            .clone()
            .ok_or_else(|| "Chromium profile identity is unavailable".to_string())?;
        let (mut request_context, pending_creation) = profile_context(config)?;
        let host_window = Arc::new(AtomicPtr::new(std::ptr::null_mut()));
        let events_enabled = Arc::new(AtomicBool::new(true));
        let closing = Arc::new(AtomicBool::new(false));
        let mut client = browser_client(
            callback.clone(),
            id,
            config.generation,
            events_enabled.clone(),
            closing.clone(),
            profile_key.clone(),
            pending_creation.clone(),
        );
        let url = CefString::from(config.source.as_str());
        let settings = BrowserSettings {
            windowless_frame_rate: 60,
            ..Default::default()
        };
        let mut browser_view_delegate = GpuixBrowserViewDelegate::new(pending_creation.clone());
        let Some(browser_view) = browser_view_create(
            Some(&mut client),
            Some(&url),
            Some(&settings),
            None,
            Some(&mut request_context),
            Some(&mut browser_view_delegate),
        ) else {
            release_pending_creation(&pending_creation);
            return Err(BROWSER_CREATION_ERROR.to_string());
        };
        track_browser_window(&pending_creation);
        let mut window_delegate = GpuixWindowDelegate::new(
            browser_view.clone(),
            host_window.clone(),
            pending_creation.clone(),
            bounds,
        );
        let Some(views_window) = window_create_top_level(Some(&mut window_delegate)) else {
            release_browser_window(&pending_creation);
            release_pending_creation(&pending_creation);
            return Err(BROWSER_CREATION_ERROR.to_string());
        };
        let native_window = host_window.load(Ordering::Acquire);
        if native_window.is_null() {
            views_window.close();
            return Err("CEF created a Views window without an AppKit window".to_string());
        }
        if let Err(error) = configure_browser_window(native_window) {
            views_window.close();
            return Err(error);
        }

        set_browser_window_bounds(parent_view, native_window, bounds);
        set_browser_window_visible(
            parent_view,
            native_window,
            Some(&views_window),
            config.visible,
        );
        self.parent_view = parent_view;
        self.host_window = Some(host_window);
        self.pending_creation = Some(pending_creation);
        self.events_enabled = Some(events_enabled);
        self.closing = Some(closing);
        self.browser_view = Some(browser_view);
        self.window = Some(views_window);
        self.client = Some(client);
        self.request_context = Some(request_context);
        self.last_bounds = Some(bounds);
        self.last_visible = Some(config.visible);
        Ok(())
    }

    fn adopt_created_browser(
        &mut self,
        config: &BrowserConfig,
        callback: &Option<EventCallback>,
        id: u64,
    ) -> Result<(), String> {
        if self.browser.is_some() {
            return Ok(());
        }
        let Some(browser) = self.browser_view.as_ref().and_then(|view| view.browser()) else {
            return Ok(());
        };
        register_browser(
            &browser,
            self.profile_key.as_deref().unwrap_or_default(),
            self.pending_creation.as_ref(),
        );
        let native_view = browser
            .host()
            .map(|host| host.window_handle() as *mut c_void)
            .filter(|view| !view.is_null())
            .ok_or_else(|| "CEF created a browser without an AppKit view".to_string())?;
        let native_window = self
            .host_window
            .as_ref()
            .map(|window| window.load(Ordering::Acquire))
            .unwrap_or(std::ptr::null_mut());
        if native_window_for_view(native_view) != Some(native_window) {
            return Err("CEF attached its browser view to an unexpected AppKit window".to_string());
        }

        self.native_view = native_view;
        self.browser = Some(browser);
        if let Some(command) = config
            .commands
            .iter()
            .find(|command| command.serial > self.last_command_serial)
            .filter(|command| {
                command.kind == "navigate"
                    && command.value.as_deref() == Some(config.source.as_str())
            })
        {
            self.last_command_serial = command.serial;
        }
        self.last_state = BrowserStateEvent {
            url: Some(config.source.clone()),
            loading: Some(true),
            can_go_back: Some(false),
            can_go_forward: Some(false),
            command_serial: Some(self.last_command_serial),
            ..Default::default()
        };
        emit_state(callback, id, config.generation, self.last_state.clone());
        if debug_enabled() {
            eprintln!("[gpuix-cef] browser Views host created");
        }
        Ok(())
    }

    fn run_command(
        &mut self,
        command: &BrowserCommand,
        callback: &Option<EventCallback>,
        id: u64,
    ) -> Result<bool, String> {
        let Some(browser) = self.browser.clone() else {
            return Ok(true);
        };
        match command.kind.as_str() {
            "navigate" => {
                let Some(url) = command.value.as_deref() else {
                    return Ok(true);
                };
                if !browser_url_allowed(url) {
                    return Err(format!("Blocked unsupported browser URL: {url}"));
                }
                if let Some(frame) = browser.main_frame() {
                    frame.load_url(Some(&CefString::from(url)));
                }
            }
            "back" => {
                if browser.can_go_back() != 0 {
                    browser.go_back();
                }
            }
            "forward" => {
                if browser.can_go_forward() != 0 {
                    browser.go_forward();
                }
            }
            "reload" => {
                if let Some(frame) = browser.main_frame() {
                    let url = userfree_string(frame.url());
                    if browser_url_allowed(&url) {
                        frame.load_url(Some(&CefString::from(url.as_str())));
                    }
                }
            }
            "stop" => browser.stop_load(),
            "focus" => {
                if let Some(host) = browser.host() {
                    host.set_focus(1);
                }
                make_first_responder(self.native_view);
            }
            "devtools" => {
                if let Some(host) = browser.host() {
                    let window_info = WindowInfo::default();
                    let settings = BrowserSettings::default();
                    host.show_dev_tools(Some(&window_info), None, Some(&settings), None);
                }
            }
            "clearData" => {
                if let Some(context) = self.request_context.clone() {
                    let done = Arc::new(AtomicBool::new(false));
                    let cancelled = Arc::new(AtomicBool::new(false));
                    self.pending_command = Some(PendingBrowserCommand {
                        serial: command.serial,
                        deadline: Instant::now() + CLEAR_DATA_TIMEOUT,
                        done: done.clone(),
                        cancelled: cancelled.clone(),
                    });
                    begin_clear_data(ClearDataState {
                        context,
                        callback: callback.clone(),
                        id,
                        generation: self.generation.unwrap_or_default(),
                        serial: command.serial,
                        done,
                        cancelled,
                    });
                    return Ok(false);
                }
            }
            "print" => {
                if let Some(host) = browser.host() {
                    host.print();
                }
            }
            _ => {}
        }
        Ok(true)
    }

    fn report_state(&mut self, callback: &Option<EventCallback>, id: u64, generation: u64) {
        let Some(browser) = self.browser.as_ref() else {
            return;
        };
        let state = BrowserStateEvent {
            url: browser
                .main_frame()
                .map(|frame| userfree_string(frame.url())),
            loading: Some(browser.is_loading() != 0),
            can_go_back: Some(browser.can_go_back() != 0),
            can_go_forward: Some(browser.can_go_forward() != 0),
            command_serial: Some(self.last_command_serial),
            ..Default::default()
        };
        if state != self.last_state {
            self.last_state = state.clone();
            emit_state(callback, id, generation, state);
        }
    }

    fn retire_active_browser(&mut self) {
        if let Some(events_enabled) = self.events_enabled.take() {
            events_enabled.store(false, Ordering::Release);
        }
        if let Some(pending) = self.pending_command.take() {
            pending.cancelled.store(true, Ordering::Release);
        }
        if !self.native_view.is_null() {
            restore_parent_first_responder(self.parent_view, self.native_view);
        }
        let native_window = self
            .host_window
            .as_ref()
            .map(|window| window.load(Ordering::Acquire))
            .unwrap_or(std::ptr::null_mut());
        set_browser_window_visible(self.parent_view, native_window, self.window.as_ref(), false);
        let browser = self.browser.take();
        let browser_view = self.browser_view.take();
        let views_window = self.window.take();
        let closing = self.closing.take();
        match (browser_view, views_window, closing) {
            (Some(browser_view), Some(window), Some(closing)) => {
                close_browser(RetiringBrowser {
                    browser,
                    browser_view,
                    window,
                    closing,
                });
            }
            (browser_view, window, closing) => {
                if let Some(closing) = closing {
                    closing.store(true, Ordering::Release);
                }
                if let Some(browser) = browser.as_ref() {
                    browser.stop_load();
                    if let Some(host) = browser.host() {
                        host.close_dev_tools();
                        host.close_browser(1);
                    }
                }
                if let Some(window) = window {
                    window.hide();
                    window.close();
                } else if let Some(pending) = self.pending_creation.as_ref() {
                    release_pending_creation(pending);
                }
                drop(browser_view);
            }
        }
        self.client = None;
        self.request_context = None;
        self.profile_key = None;
        self.generation = None;
        self.parent_view = std::ptr::null_mut();
        self.host_window = None;
        self.pending_creation = None;
        self.events_enabled = None;
        self.closing = None;
        self.native_view = std::ptr::null_mut();
        self.last_bounds = None;
        self.last_visible = None;
        self.last_state = BrowserStateEvent::default();
        self.creation_attempts = 0;
        self.creation_failed = false;
    }

    pub(super) fn destroy(&mut self) {
        self.retire_active_browser();
        self.last_command_serial = 0;
        self.last_retry_serial = 0;
    }
}

impl Drop for BrowserRuntime {
    fn drop(&mut self) {
        self.destroy();
    }
}

fn close_browser(retiring: RetiringBrowser) {
    let RetiringBrowser {
        browser,
        browser_view: _browser_view,
        window,
        closing,
    } = retiring;
    closing.store(true, Ordering::Release);
    window.hide();
    let identifier = browser.as_ref().map(|browser| browser.identifier());
    if let Some(browser) = browser.as_ref() {
        browser.stop_load();
        if let Some(host) = browser.host() {
            host.close_dev_tools();
            host.close_browser(1);
        }
    }
    window.close();
    if debug_enabled() {
        eprintln!(
            "[gpuix-cef] close requested browser={}",
            identifier.unwrap_or_default()
        );
    }
}

fn profile_context(
    config: &BrowserConfig,
) -> Result<(RequestContext, PendingBrowserCreation), String> {
    let key = format!(
        "{}:{}:{}",
        config.profile_id, config.profile_path, config.incognito
    );
    PROCESS.with(|process| {
        let mut process = process.borrow_mut();
        let context = if let Some(entry) = process.contexts.get(&key) {
            entry.context.clone()
        } else {
            let settings = if config.incognito {
                RequestContextSettings::default()
            } else {
                if config.profile_path.is_empty() {
                    return Err("Persistent Chromium profile path is empty".to_string());
                }
                let profile_path = validated_profile_path(
                    PathBuf::from(&config.profile_path),
                    &process.root_cache_path,
                )?;
                RequestContextSettings {
                    cache_path: cef_path_string(&profile_path)?,
                    persist_session_cookies: 1,
                    ..Default::default()
                }
            };
            let context =
                request_context_create_context(Some(&settings), None).ok_or_else(|| {
                    format!("Could not create Chromium profile {}", config.profile_id)
                })?;
            process.contexts.insert(
                key.clone(),
                BrowserContextEntry {
                    context: context.clone(),
                    references: 0,
                    pending_references: 0,
                    ephemeral: config.incognito,
                },
            );
            context
        };
        if let Some(entry) = process.contexts.get_mut(&key) {
            entry.pending_references += 1;
        }
        process.pending_browser_creations += 1;
        Ok((
            context,
            PendingBrowserCreation {
                profile_key: key,
                active: Arc::new(AtomicBool::new(true)),
                window_active: Arc::new(AtomicBool::new(false)),
            },
        ))
    })
}

fn validated_profile_path(path: PathBuf, root: &Path) -> Result<PathBuf, String> {
    let requested = absolute_path(path)?;
    let parent = requested.parent().ok_or_else(|| {
        format!(
            "Chromium profile {} has no parent directory",
            requested.display()
        )
    })?;
    let parent = parent.canonicalize().map_err(|error| {
        format!(
            "Could not resolve Chromium profile parent {}: {error}",
            parent.display()
        )
    })?;
    if parent != root {
        return Err(format!(
            "Chromium profile {} must be an immediate child of root cache {}",
            requested.display(),
            root.display()
        ));
    }
    let name = requested.file_name().ok_or_else(|| {
        format!(
            "Chromium profile {} has no directory name",
            requested.display()
        )
    })?;
    let candidate = parent.join(name);
    match fs::symlink_metadata(&candidate) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "Chromium profile {} cannot be a symbolic link",
                candidate.display()
            ));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(format!(
                "Chromium profile {} is not a directory",
                candidate.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&candidate).map_err(|error| {
                format!(
                    "Could not create Chromium profile {}: {error}",
                    candidate.display()
                )
            })?;
        }
        Err(error) => {
            return Err(format!(
                "Could not inspect Chromium profile {}: {error}",
                candidate.display()
            ));
        }
    }
    let resolved = candidate.canonicalize().map_err(|error| {
        format!(
            "Could not resolve Chromium profile {}: {error}",
            candidate.display()
        )
    })?;
    if resolved.parent() != Some(root) {
        return Err(format!(
            "Chromium profile {} escaped root cache {}",
            resolved.display(),
            root.display()
        ));
    }
    Ok(resolved)
}

fn register_browser(
    browser: &Browser,
    profile_key: &str,
    pending: Option<&PendingBrowserCreation>,
) {
    let consumed_pending =
        pending.is_some_and(|pending| pending.active.swap(false, Ordering::AcqRel));
    PROCESS.with(|process| {
        let mut process = process.borrow_mut();
        if consumed_pending {
            process.pending_browser_creations = process.pending_browser_creations.saturating_sub(1);
            if let Some(pending) = pending {
                if let Some(entry) = process.contexts.get_mut(&pending.profile_key) {
                    entry.pending_references = entry.pending_references.saturating_sub(1);
                }
            }
        }

        let inserted = if process.browsers.contains_key(&browser.identifier()) {
            false
        } else {
            process.browsers.insert(
                browser.identifier(),
                BrowserEntry {
                    browser: browser.clone(),
                    profile_key: profile_key.to_string(),
                },
            );
            true
        };
        if inserted {
            if let Some(entry) = process.contexts.get_mut(profile_key) {
                entry.references += 1;
            }
        }

        if let Some(pending) = pending.filter(|_| consumed_pending) {
            let remove = process
                .contexts
                .get(&pending.profile_key)
                .is_some_and(|entry| {
                    entry.ephemeral && entry.references == 0 && entry.pending_references == 0
                });
            if remove {
                process.contexts.remove(&pending.profile_key);
            }
        }
    });
}

fn release_pending_creation(pending: &PendingBrowserCreation) {
    if !pending.active.swap(false, Ordering::AcqRel) {
        return;
    }
    PROCESS.with(|process| {
        let mut process = process.borrow_mut();
        process.pending_browser_creations = process.pending_browser_creations.saturating_sub(1);
        let remove = process
            .contexts
            .get_mut(&pending.profile_key)
            .is_some_and(|entry| {
                entry.pending_references = entry.pending_references.saturating_sub(1);
                entry.ephemeral && entry.references == 0 && entry.pending_references == 0
            });
        if remove {
            process.contexts.remove(&pending.profile_key);
        }
    });
}

fn track_browser_window(pending: &PendingBrowserCreation) {
    if pending.window_active.swap(true, Ordering::AcqRel) {
        return;
    }
    PROCESS.with(|process| process.borrow_mut().active_browser_windows += 1);
}

fn release_browser_window(pending: &PendingBrowserCreation) {
    if !pending.window_active.swap(false, Ordering::AcqRel) {
        return;
    }
    PROCESS.with(|process| {
        let mut process = process.borrow_mut();
        process.active_browser_windows = process.active_browser_windows.saturating_sub(1);
    });
}

fn unregister_browser(identifier: i32) -> usize {
    let removed = PROCESS.with(|process| process.borrow_mut().browsers.remove(&identifier));
    if let Some(entry) = removed.as_ref() {
        PROCESS.with(|process| {
            let mut process = process.borrow_mut();
            let remove = process
                .contexts
                .get_mut(&entry.profile_key)
                .is_some_and(|context| {
                    context.references = context.references.saturating_sub(1);
                    context.ephemeral && context.references == 0 && context.pending_references == 0
                });
            if remove {
                process.contexts.remove(&entry.profile_key);
            }
        });
    }
    drop(removed);
    PROCESS.with(|process| process.borrow().browsers.len())
}

fn report_browser_closed(identifier: i32) {
    let remaining = unregister_browser(identifier);
    if debug_enabled() {
        eprintln!("[gpuix-cef] browser closed remaining={remaining}");
    }
}

// Keep the top-level host in CEF Views so its documented close handshake tears
// down Chromium before CefShutdown. AppKit only handles attachment and positioning.
cef::wrap_browser_view_delegate! {
    struct GpuixBrowserViewDelegate {
        pending_creation: PendingBrowserCreation,
    }

    impl ViewDelegate {}

    impl BrowserViewDelegate {
        fn on_browser_destroyed(
            &self,
            _browser_view: Option<&mut BrowserView>,
            _browser: Option<&mut Browser>,
        ) {
            release_pending_creation(&self.pending_creation);
        }

        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

cef::wrap_window_delegate! {
    struct GpuixWindowDelegate {
        browser_view: BrowserView,
        host_window: Arc<AtomicPtr<c_void>>,
        pending_creation: PendingBrowserCreation,
        bounds: (f32, f32, f32, f32),
    }

    impl ViewDelegate {}

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let Some(window) = window else {
                return;
            };
            window.set_to_fill_layout();
            let mut browser_view = View::from(&self.browser_view);
            window.add_child_view(Some(&mut browser_view));
            window.layout();
            let native_view = window.window_handle() as *mut c_void;
            if let Some(native_window) = native_window_for_view(native_view) {
                self.host_window
                    .store(native_window, Ordering::Release);
            }
        }

        fn on_window_closing(&self, _window: Option<&mut Window>) {
            detach_browser_window(self.host_window.load(Ordering::Acquire));
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            self.host_window
                .store(std::ptr::null_mut(), Ordering::Release);
            release_pending_creation(&self.pending_creation);
            release_browser_window(&self.pending_creation);
            if debug_enabled() {
                eprintln!("[gpuix-cef] browser Views window destroyed");
            }
        }

        fn initial_bounds(&self, _window: Option<&mut Window>) -> Rect {
            cef_rect((0.0, 0.0, self.bounds.2, self.bounds.3))
        }

        fn initial_show_state(&self, _window: Option<&mut Window>) -> ShowState {
            ShowState::HIDDEN
        }

        fn is_frameless(&self, _window: Option<&mut Window>) -> i32 {
            1
        }

        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> i32 {
            0
        }

        fn accepts_first_mouse(&self, _window: Option<&mut Window>) -> State {
            State::ENABLED
        }

        fn can_resize(&self, _window: Option<&mut Window>) -> i32 {
            0
        }

        fn can_maximize(&self, _window: Option<&mut Window>) -> i32 {
            0
        }

        fn can_minimize(&self, _window: Option<&mut Window>) -> i32 {
            0
        }

        fn can_close(&self, _window: Option<&mut Window>) -> i32 {
            self.browser_view
                .browser()
                .and_then(|browser| browser.host())
                .map(|host| host.try_close_browser())
                .unwrap_or(1)
        }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

fn browser_client(
    callback: Option<EventCallback>,
    id: u64,
    generation: u64,
    events_enabled: Arc<AtomicBool>,
    closing: Arc<AtomicBool>,
    profile_key: String,
    pending_creation: PendingBrowserCreation,
) -> Client {
    let callback = callback.map(|callback| {
        Arc::new(move |payload| {
            if events_enabled.load(Ordering::Acquire) {
                callback(payload);
            }
        }) as EventCallback
    });
    let display = GpuixDisplayHandler::new(callback.clone(), id, generation);
    let load = GpuixLoadHandler::new(callback.clone(), id, generation);
    let life_span = GpuixLifeSpanHandler::new(
        callback.clone(),
        id,
        generation,
        closing,
        profile_key,
        pending_creation,
    );
    let request = GpuixRequestHandler::new(callback.clone(), id, generation);
    let download = GpuixDownloadHandler::new();
    GpuixClient::new(display, load, life_span, request, download)
}

cef::wrap_display_handler! {
    struct GpuixDisplayHandler {
        callback: Option<EventCallback>,
        id: u64,
        generation: u64,
    }

    impl DisplayHandler {
        fn on_address_change(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            if frame.is_some_and(|frame| frame.is_main() != 0) {
                emit_state(
                    &self.callback,
                    self.id,
                    self.generation,
                    BrowserStateEvent {
                        url: url.map(ToString::to_string),
                        ..Default::default()
                    },
                );
            }
        }

        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            emit_state(
                &self.callback,
                self.id,
                self.generation,
                BrowserStateEvent {
                    title: title.map(ToString::to_string),
                    ..Default::default()
                },
            );
        }
    }
}

cef::wrap_load_handler! {
    struct GpuixLoadHandler {
        callback: Option<EventCallback>,
        id: u64,
        generation: u64,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut Browser>,
            is_loading: i32,
            can_go_back: i32,
            can_go_forward: i32,
        ) {
            emit_state(
                &self.callback,
                self.id,
                self.generation,
                BrowserStateEvent {
                    loading: Some(is_loading != 0),
                    can_go_back: Some(can_go_back != 0),
                    can_go_forward: Some(can_go_forward != 0),
                    ..Default::default()
                },
            );
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            if error_code == Errorcode::ABORTED || !frame.is_some_and(|frame| frame.is_main() != 0) {
                return;
            }
            let message = format!(
                "{} ({}): {}",
                failed_url.map(ToString::to_string).unwrap_or_default(),
                error_code.get_raw(),
                error_text.map(ToString::to_string).unwrap_or_default()
            );
            emit_state(
                &self.callback,
                self.id,
                self.generation,
                BrowserStateEvent {
                    loading: Some(false),
                    error: Some(message.clone()),
                    ..Default::default()
                },
            );
            emit_tagged_value(
                &self.callback,
                self.id,
                "browserError",
                self.generation,
                message,
            );
        }
    }
}

cef::wrap_life_span_handler! {
    struct GpuixLifeSpanHandler {
        callback: Option<EventCallback>,
        id: u64,
        generation: u64,
        closing: Arc<AtomicBool>,
        profile_key: String,
        pending_creation: PendingBrowserCreation,
    }

    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: i32,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: i32,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut i32>,
        ) -> i32 {
            let url = target_url.map(ToString::to_string).unwrap_or_default();
            if !browser_url_allowed(&url) {
                emit_tagged_value(
                    &self.callback,
                    self.id,
                    "browserError",
                    self.generation,
                    format!("Blocked unsupported browser URL: {url}"),
                );
                return 1;
            }
            emit_tagged_value(
                &self.callback,
                self.id,
                "browserOpen",
                self.generation,
                url,
            );
            1
        }

        fn on_before_dev_tools_popup(
            &self,
            _browser: Option<&mut Browser>,
            _window_info: Option<&mut WindowInfo>,
            client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _use_default_window: Option<&mut i32>,
        ) {
            if let Some(client) = client {
                *client = Some(dev_tools_client(
                    self.profile_key.clone(),
                    self.closing.clone(),
                ));
            }
        }

        fn on_after_created(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                register_browser(
                    browser,
                    &self.profile_key,
                    Some(&self.pending_creation),
                );
                if self.closing.load(Ordering::Acquire)
                    || SHUTTING_DOWN.load(Ordering::Acquire)
                {
                    if let Some(host) = browser.host() {
                        host.close_browser(1);
                    }
                }
            }
        }

        fn do_close(&self, browser: Option<&mut Browser>) -> i32 {
            let retiring = self.closing.load(Ordering::Acquire);
            let shutting_down = SHUTTING_DOWN.load(Ordering::Acquire);
            if debug_enabled() {
                eprintln!(
                    "[gpuix-cef] DoClose browser={} retiring={retiring} shutdown={shutting_down}",
                    browser
                        .as_ref()
                        .map(|browser| browser.identifier())
                        .unwrap_or_default(),
                );
            }
            0
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                report_browser_closed(browser.identifier());
            }
        }
    }
}

fn dev_tools_client(profile_key: String, closing: Arc<AtomicBool>) -> Client {
    let life_span = GpuixDevToolsLifeSpanHandler::new(profile_key, closing);
    GpuixDevToolsClient::new(life_span)
}

cef::wrap_life_span_handler! {
    struct GpuixDevToolsLifeSpanHandler {
        profile_key: String,
        closing: Arc<AtomicBool>,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                register_browser(browser, &self.profile_key, None);
                if self.closing.load(Ordering::Acquire)
                    || SHUTTING_DOWN.load(Ordering::Acquire)
                {
                    if let Some(host) = browser.host() {
                        host.close_browser(1);
                    }
                }
            }
        }

        fn do_close(&self, _browser: Option<&mut Browser>) -> i32 {
            0
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                report_browser_closed(browser.identifier());
            }
        }
    }
}

cef::wrap_client! {
    struct GpuixDevToolsClient {
        life_span_handler: LifeSpanHandler,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span_handler.clone())
        }
    }
}

cef::wrap_request_handler! {
    struct GpuixRequestHandler {
        callback: Option<EventCallback>,
        id: u64,
        generation: u64,
    }

    impl RequestHandler {
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: i32,
            _is_redirect: i32,
        ) -> i32 {
            if frame.is_some_and(|frame| frame.is_main() == 0) {
                return 0;
            }
            let url = request
                .map(|request| userfree_string(request.url()))
                .unwrap_or_default();
            if url.is_empty() || browser_url_allowed(&url) {
                return 0;
            }
            emit_tagged_value(
                &self.callback,
                self.id,
                "browserError",
                self.generation,
                format!("Blocked unsupported browser URL: {url}"),
            );
            1
        }

        fn on_render_process_terminated(
            &self,
            _browser: Option<&mut Browser>,
            status: TerminationStatus,
            error_code: i32,
            error_string: Option<&CefString>,
        ) {
            emit_tagged_value(
                &self.callback,
                self.id,
                "browserError",
                self.generation,
                format!(
                    "Chromium renderer terminated ({:?}, {error_code}): {}",
                    status,
                    error_string.map(ToString::to_string).unwrap_or_default()
                ),
            );
        }
    }
}

cef::wrap_download_handler! {
    struct GpuixDownloadHandler;

    impl DownloadHandler {
        fn can_download(
            &self,
            _browser: Option<&mut Browser>,
            _url: Option<&CefString>,
            _request_method: Option<&CefString>,
        ) -> i32 {
            0
        }
    }
}

cef::wrap_client! {
    struct GpuixClient {
        display_handler: DisplayHandler,
        load_handler: LoadHandler,
        life_span_handler: LifeSpanHandler,
        request_handler: RequestHandler,
        download_handler: DownloadHandler,
    }

    impl Client {
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(self.display_handler.clone())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.load_handler.clone())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span_handler.clone())
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(self.request_handler.clone())
        }

        fn download_handler(&self) -> Option<DownloadHandler> {
            Some(self.download_handler.clone())
        }
    }
}

fn parent_view(window: &gpui::Window) -> Result<*mut c_void, String> {
    let handle = window
        .native_window_handle()
        .map_err(|error| format!("Browser parent view is unavailable: {error}"))?;
    match handle.as_raw() {
        RawWindowHandle::AppKit(handle) => Ok(handle.ns_view.as_ptr()),
        _ => Err("Browser parent is not an AppKit view".to_string()),
    }
}

fn cef_rect(bounds: (f32, f32, f32, f32)) -> Rect {
    Rect {
        x: bounds.0.round() as i32,
        y: bounds.1.round() as i32,
        width: bounds.2.round().max(1.0) as i32,
        height: bounds.3.round().max(1.0) as i32,
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NativePoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeRect {
    origin: NativePoint,
    size: NativeSize,
}

unsafe impl objc::Encode for NativePoint {
    fn encode() -> objc::Encoding {
        unsafe { objc::Encoding::from_str("{CGPoint=dd}") }
    }
}

unsafe impl objc::Encode for NativeSize {
    fn encode() -> objc::Encoding {
        unsafe { objc::Encoding::from_str("{CGSize=dd}") }
    }
}

unsafe impl objc::Encode for NativeRect {
    fn encode() -> objc::Encoding {
        unsafe { objc::Encoding::from_str("{CGRect={CGPoint=dd}{CGSize=dd}}") }
    }
}

fn native_window_for_view(native_view: *mut c_void) -> Option<*mut c_void> {
    if native_view.is_null() {
        return None;
    }
    let window: *mut Object = unsafe { msg_send![native_view as *mut Object, window] };
    (!window.is_null()).then_some(window as *mut c_void)
}

fn configure_browser_window(native_window: *mut c_void) -> Result<(), String> {
    if native_window.is_null() {
        return Err("Cannot configure an unavailable Chromium window".to_string());
    }
    unsafe {
        let window = native_window as *mut Object;
        let _: () = msg_send![window, setTabbingMode: NS_WINDOW_TABBING_MODE_DISALLOWED];
        let style_mask: usize = msg_send![window, styleMask];
        let tabbing_mode: isize = msg_send![window, tabbingMode];
        if tabbing_mode != NS_WINDOW_TABBING_MODE_DISALLOWED {
            return Err("Could not disable native Chromium window tabbing".to_string());
        }
        if debug_enabled() {
            eprintln!(
                "[gpuix-cef] frameless Views window style={style_mask} tabbing={tabbing_mode}"
            );
        }
    }
    Ok(())
}

fn attach_browser_window(parent_view: *mut c_void, native_window: *mut c_void) {
    if parent_view.is_null() || native_window.is_null() {
        return;
    }
    unsafe {
        let parent = parent_view as *mut Object;
        let window = native_window as *mut Object;
        let owner: *mut Object = msg_send![parent, window];
        if owner.is_null() {
            return;
        }
        let current_owner: *mut Object = msg_send![window, parentWindow];
        if current_owner == owner {
            return;
        }
        if !current_owner.is_null() {
            let _: () = msg_send![current_owner, removeChildWindow: window];
        }
        let _: () = msg_send![owner, addChildWindow: window ordered: 1isize];
    }
}

fn set_browser_window_bounds(
    parent_view: *mut c_void,
    native_window: *mut c_void,
    bounds: (f32, f32, f32, f32),
) {
    if parent_view.is_null() || native_window.is_null() {
        return;
    }
    unsafe {
        let parent = parent_view as *mut Object;
        let window = native_window as *mut Object;
        let owner: *mut Object = msg_send![parent, window];
        if owner.is_null() {
            return;
        }
        let parent_bounds: NativeRect = msg_send![parent, bounds];
        let flipped: BOOL = msg_send![parent, isFlipped];
        let y = if flipped == YES {
            bounds.1 as f64
        } else {
            parent_bounds.size.height - bounds.1 as f64 - bounds.3 as f64
        };
        let frame_in_parent = NativeRect {
            origin: NativePoint {
                x: bounds.0 as f64,
                y,
            },
            size: NativeSize {
                width: bounds.2 as f64,
                height: bounds.3 as f64,
            },
        };
        let no_view = std::ptr::null_mut::<Object>();
        let frame_in_owner: NativeRect =
            msg_send![parent, convertRect: frame_in_parent toView: no_view];
        let frame_on_screen: NativeRect = msg_send![owner, convertRectToScreen: frame_in_owner];
        let _: () = msg_send![window, setFrame: frame_on_screen display: NO];
    }
    attach_browser_window(parent_view, native_window);
}

fn set_browser_window_visible(
    parent_view: *mut c_void,
    native_window: *mut c_void,
    views_window: Option<&Window>,
    requested_visible: bool,
) {
    if native_window.is_null() {
        return;
    }
    unsafe {
        let window = native_window as *mut Object;
        if requested_visible && !parent_view.is_null() {
            attach_browser_window(parent_view, native_window);
        }
        let owner: *mut Object = msg_send![window, parentWindow];
        let owner_visible = if owner.is_null() {
            false
        } else {
            let visible: BOOL = msg_send![owner, isVisible];
            let minimized: BOOL = msg_send![owner, isMiniaturized];
            visible == YES && minimized == NO
        };
        let should_show = requested_visible && owner_visible;
        let native_visible: BOOL = msg_send![window, isVisible];
        let views_visible = views_window.is_some_and(|window| window.is_visible() != 0);
        if should_show {
            if !views_visible {
                if let Some(views_window) = views_window {
                    views_window.show();
                }
            }
            if native_visible == NO {
                let _: () = msg_send![window, orderFront: std::ptr::null_mut::<Object>()];
            }
        } else {
            if views_visible {
                if let Some(views_window) = views_window {
                    views_window.hide();
                }
            }
            if native_visible == YES {
                let _: () = msg_send![window, orderOut: std::ptr::null_mut::<Object>()];
            }
        }
    }
}

fn detach_browser_window(native_window: *mut c_void) {
    if native_window.is_null() {
        return;
    }
    unsafe {
        let window = native_window as *mut Object;
        let _: () = msg_send![window, orderOut: std::ptr::null_mut::<Object>()];
        let owner: *mut Object = msg_send![window, parentWindow];
        if !owner.is_null() {
            let _: () = msg_send![owner, removeChildWindow: window];
        }
    }
}

fn make_first_responder(native_view: *mut c_void) {
    if native_view.is_null() {
        return;
    }
    unsafe {
        let view = native_view as *mut Object;
        let window: *mut Object = msg_send![view, window];
        if !window.is_null() {
            let _: () = msg_send![window, makeKeyWindow];
            let _: BOOL = msg_send![window, makeFirstResponder: view];
        }
    }
}

fn restore_parent_first_responder(parent_view: *mut c_void, native_view: *mut c_void) {
    if parent_view.is_null() || native_view.is_null() {
        return;
    }
    unsafe {
        let parent = parent_view as *mut Object;
        let native = native_view as *mut Object;
        let browser_window: *mut Object = msg_send![native, window];
        if browser_window.is_null() {
            return;
        }
        let key: BOOL = msg_send![browser_window, isKeyWindow];
        if key == NO {
            return;
        }
        let owner: *mut Object = msg_send![parent, window];
        if owner.is_null() {
            return;
        }
        let _: () = msg_send![owner, makeKeyWindow];
        let _: BOOL = msg_send![owner, makeFirstResponder: parent];
    }
}

fn userfree_string(value: CefStringUserfree) -> String {
    CefString::from(&value).to_string()
}

pub(super) fn browser_url_allowed(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://") || url == "about:blank"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "gpuix-browser-profile-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn unbundled_process_fails_before_resolving_cef_assets() {
        if main_app_bundle().is_none() {
            match resolve_runtime() {
                Err(error) => assert!(error.contains("requires a macOS application bundle")),
                Ok(_) => panic!("unbundled CEF must fail closed"),
            }
        }
    }

    #[test]
    fn profile_path_is_authorized_before_creation() {
        let base = temporary_root();
        let root = base.join("root");
        let outside = base.join("outside");
        fs::create_dir_all(&root).expect("root");
        fs::create_dir_all(&outside).expect("outside");
        let root = root.canonicalize().expect("canonical root");

        let rejected = outside.join("must-not-exist");
        assert!(validated_profile_path(rejected.clone(), &root).is_err());
        assert!(!rejected.exists());

        let accepted = root.join("workspace");
        assert_eq!(
            validated_profile_path(accepted.clone(), &root).expect("valid child"),
            accepted
        );
        assert!(accepted.is_dir());

        let linked = root.join("linked");
        symlink(&outside, &linked).expect("symlink");
        assert!(validated_profile_path(linked, &root).is_err());

        fs::remove_dir_all(base).expect("cleanup");
    }
}
