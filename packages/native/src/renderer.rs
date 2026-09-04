//! GPUIX retained renderer for napi desktop hosts and GPUI's browser platform.
//!
//! Mutation-based API: React's reconciler sends individual mutations
//! (createElement, appendChild, setStyle, etc.) instead of a full JSON tree.
//! Rust maintains a RetainedTree and rebuilds GPUI elements from it each frame.
//!
//! Desktop lifecycle:
//!   const renderer = new GpuixRenderer(eventCallback)
//!   renderer.init({ title: 'My App', width: 800, height: 600 })
//!   renderer.applyBatch(json)             // one atomic React commit
//!   setTimeout(function loop() {         // macOS pumps AppKit; Win/Linux polls UI thread
//!     if (!renderer.tick()) process.exit(0)
//!     setTimeout(loop, 8)
//!   })
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
use futures::{channel::mpsc, StreamExt as _};
use gpui::AppContext as _;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use napi::bindgen_prelude::*;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use napi_derive::napi;
use std::cell::RefCell;
use std::collections::HashMap;
#[cfg(any(target_os = "macos", target_family = "wasm"))]
use std::rc::Rc;
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
use std::sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm_bindgen::JsCast as _;

use crate::custom_elements::CustomElementRegistry;
use crate::element_tree::EventPayload;
use crate::retained_tree::RetainedTree;
// Custom elements still style their own sub-parts directly.
pub(crate) use crate::style::resolve::{apply_interactive_styles, parse_font_weight};
use crate::text::{selection_frame_reset, SharedSelection};
use crate::theme::Theme;

mod auto_height;
mod batch;
mod frame;
pub(crate) mod scroll_into_view;
pub(crate) mod scroll_marker;
pub(crate) mod scroll_motion;
pub(crate) mod scroll_timeline;
pub(crate) mod scrollbar;
pub(crate) mod view_transition;
mod virtual_list;

pub use batch::apply_batch_to_tree;
use frame::{build_element, unmounted_virtual_row, BuildCtx};
use virtual_list::VirtualListEntry;

/// The Window menu items act on the focused window, and the root element is the
/// only place in GPUIX that has one. `crate::app_menu` owns everything else.
#[cfg(target_os = "macos")]
fn with_window_menu_actions(root: gpui::Div) -> gpui::Div {
    use crate::app_menu::{CloseWindow, MinimizeWindow, ZoomWindow};
    use gpui::prelude::*;

    root.on_action(|_: &MinimizeWindow, window, _cx| window.minimize_window())
        .on_action(|_: &ZoomWindow, window, _cx| window.zoom_window())
        .on_action(|_: &CloseWindow, window, _cx| window.remove_window())
}

#[cfg(not(target_os = "macos"))]
fn with_window_menu_actions(root: gpui::Div) -> gpui::Div {
    root
}

/// Abstracted event callback shared by desktop, browser, and test renderers.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) type EventCallback = Arc<dyn Fn(EventPayload) + Send + Sync>;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub(crate) type EventCallback = Rc<dyn Fn(EventPayload)>;

/// Validate and convert a JS number (f64) to a u64 element ID.
/// JS numbers are f64 — lossless for integers up to 2^53.
fn raw_element_id(id: f64) -> std::result::Result<u64, String> {
    if !id.is_finite() || id < 0.0 || id.fract() != 0.0 || id > 9_007_199_254_740_991.0 {
        return Err(format!("Invalid element id: {id}"));
    }
    Ok(id as u64)
}

/// A scroll offset as the two JS numbers `[x, y]`. Adding `0.0` turns a
/// negative zero into a plain zero. An unscrolled axis can carry `-0.0`,
/// and a JS caller that compares offsets with `Object.is` separates `-0`
/// from `0`.
pub(crate) fn offset_to_js(offset: gpui::Point<gpui::Pixels>) -> [f64; 2] {
    [
        f64::from(f32::from(offset.x)) + 0.0,
        f64::from(f32::from(offset.y)) + 0.0,
    ]
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn to_element_id(id: f64) -> Result<u64> {
    raw_element_id(id).map_err(Error::from_reason)
}

thread_local! {
    #[cfg(target_os = "macos")]
    static MAC_PLATFORM: RefCell<Option<Rc<gpui_macos::MacPlatform>>> = const { RefCell::new(None) };
    #[cfg(target_os = "macos")]
    static GPUI_APP: RefCell<Option<gpui::ApplicationHandle>> = const { RefCell::new(None) };
    #[cfg(target_os = "macos")]
    static GPUI_WINDOW: RefCell<Option<gpui::WindowHandle<GpuixView>>> = const { RefCell::new(None) };
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    static WEB_APP: RefCell<Option<gpui::ApplicationHandle>> = const { RefCell::new(None) };
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    static WEB_WINDOW: RefCell<Option<gpui::WindowHandle<GpuixView>>> = const { RefCell::new(None) };
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    static PENDING_DEBUG_OVERLAY: RefCell<Option<gpui::DebugFrameOverlayMode>> =
        const { RefCell::new(None) };
    /// Shared scroll handles — GpuixView writes here during render(),
    /// platform-local handlers read from here for programmatic scroll control.
    /// ScrollHandle is Rc<RefCell<...>> so its methods (set_offset, offset,
    /// scroll_to_item) work without an App context.
    ///
    /// NOTE: This is a singleton — if multiple renderers/windows coexist,
    /// the last one to render wins. Acceptable for now (single-window only).
    /// TODO: Scope by renderer/window ID when multi-window support is added.
    static SCROLL_HANDLES: RefCell<HashMap<u64, gpui::ScrollHandle>> = RefCell::new(HashMap::new());
    static VIRTUAL_LIST_STATES: RefCell<HashMap<u64, gpui::ListState>> = RefCell::new(HashMap::new());
    /// Virtual-list scrolls queued for the next `GpuixView::render`, applied
    /// AFTER `VirtualListEntry::sync` splices that frame's child changes.
    ///
    /// Never applied eagerly: JS computes row indices against the child list it
    /// just committed, but that commit only reaches `gpui::ListState` when the
    /// next render splices it in. An eager `scroll_to` would be shifted a
    /// second time by `splice_focusable` and land on the wrong row.
    static PENDING_VIRTUAL_LIST_SCROLLS: RefCell<HashMap<u64, gpui::ListOffset>> =
        RefCell::new(HashMap::new());
}

const SELECTION_SCROLL_TICK_MS: u64 = 24;
const SELECTION_SCROLL_EDGE_PX: f32 = 36.0;
const SELECTION_SCROLL_MAX_STEP_PX: f32 = 24.0;

/// Signed list scroll step for a pointer near a viewport edge.
fn selection_scroll_step(
    bounds: gpui::Bounds<gpui::Pixels>,
    position: gpui::Point<gpui::Pixels>,
) -> f32 {
    let height = f32::from(bounds.size.height);
    if height <= 0.0 {
        return 0.0;
    }
    let edge = SELECTION_SCROLL_EDGE_PX.min(height / 6.0);
    if edge <= 0.0 {
        return 0.0;
    }
    let y = f32::from(position.y);
    let top = f32::from(bounds.top());
    let bottom = f32::from(bounds.bottom());
    let scaled = |penetration: f32| {
        let progress = (penetration / edge).clamp(0.0, 1.0);
        SELECTION_SCROLL_MAX_STEP_PX * progress * progress
    };
    if y < top + edge {
        -scaled(top + edge - y)
    } else if y > bottom - edge {
        scaled(y - (bottom - edge))
    } else {
        0.0
    }
}

/// Queue a virtual-list scroll for the next render. `offset_in_item` may be
/// negative: gpui then anchors the viewport top above the item, which is what
/// keeps a row pixel-stable while unmeasured rows are spliced in above it.
pub(crate) fn queue_virtual_list_scroll(id: u64, index: usize, offset_in_item: f32) {
    PENDING_VIRTUAL_LIST_SCROLLS.with(|cell| {
        cell.borrow_mut().insert(
            id,
            gpui::ListOffset {
                item_ix: index,
                offset_in_item: gpui::px(offset_in_item),
            },
        );
    });
}

fn parse_debug_frame_overlay_mode_str(
    mode: &str,
) -> std::result::Result<gpui::DebugFrameOverlayMode, String> {
    match mode {
        "hidden" => Ok(gpui::DebugFrameOverlayMode::Hidden),
        "minimal" => Ok(gpui::DebugFrameOverlayMode::Minimal),
        "full" => Ok(gpui::DebugFrameOverlayMode::Full),
        other => Err(format!(
            "Unknown debug frame overlay mode {other:?}. Use hidden, minimal, or full."
        )),
    }
}

#[cfg(test)]
mod selection_scroll_tests {
    use super::*;

    #[test]
    fn selection_scroll_ramps_at_viewport_edges() {
        let bounds = gpui::Bounds::new(
            gpui::point(gpui::px(10.0), gpui::px(20.0)),
            gpui::size(gpui::px(300.0), gpui::px(200.0)),
        );
        assert_eq!(
            selection_scroll_step(bounds, gpui::point(gpui::px(20.0), gpui::px(120.0))),
            0.0
        );
        assert!(selection_scroll_step(bounds, gpui::point(gpui::px(20.0), gpui::px(20.0))) < 0.0);
        assert!(selection_scroll_step(bounds, gpui::point(gpui::px(20.0), gpui::px(220.0))) > 0.0);
        assert!(
            selection_scroll_step(bounds, gpui::point(gpui::px(20.0), gpui::px(220.0)))
                > selection_scroll_step(bounds, gpui::point(gpui::px(20.0), gpui::px(200.0)))
        );
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn parse_debug_frame_overlay_mode(mode: &str) -> Result<gpui::DebugFrameOverlayMode> {
    parse_debug_frame_overlay_mode_str(mode).map_err(Error::from_reason)
}

pub(crate) fn debug_frame_overlay_mode_name(mode: gpui::DebugFrameOverlayMode) -> &'static str {
    match mode {
        gpui::DebugFrameOverlayMode::Hidden => "hidden",
        gpui::DebugFrameOverlayMode::Minimal => "minimal",
        gpui::DebugFrameOverlayMode::Full => "full",
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn debug_frame_overlay_stats_js(
    stats: gpui::DebugFrameOverlayStats,
) -> DebugFrameOverlayStats {
    DebugFrameOverlayStats {
        current_ms: stats.current_ms.map(|ms| ms as f64),
        p90_ms: stats.p90_ms.map(|ms| ms as f64),
        p99_ms: stats.p99_ms.map(|ms| ms as f64),
        max_ms: stats.max_ms.map(|ms| ms as f64),
        frames: stats.frames as f64,
        samples: stats.samples as f64,
    }
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
fn recv_ui_response<T>(receiver: std::sync::mpsc::Receiver<T>, operation: &str) -> Result<T> {
    match receiver.recv_timeout(Duration::from_secs(2)) {
        Ok(response) => Ok(response),
        Err(RecvTimeoutError::Timeout) => Err(Error::from_reason(format!(
            "Timed out after 2 seconds waiting for {operation}"
        ))),
        Err(RecvTimeoutError::Disconnected) => Err(Error::from_reason(format!(
            "The GPUI UI thread stopped during {operation}"
        ))),
    }
}

#[cfg(target_os = "macos")]
fn update_window<R>(
    update: impl FnOnce(&mut GpuixView, &mut gpui::Window, &mut gpui::Context<GpuixView>) -> R,
) -> Result<R> {
    let window = GPUI_WINDOW
        .with(|window| *window.borrow())
        .ok_or_else(|| Error::from_reason("GPUI window is not initialized"))?;

    GPUI_APP.with(|app| {
        let app = app.borrow();
        let app = app
            .as_ref()
            .ok_or_else(|| Error::from_reason("GPUI application is not initialized"))?;
        app.update(|cx| {
            window
                .update(cx, update)
                .map_err(|error| Error::from_reason(error.to_string()))
        })
    })
}

#[cfg(target_os = "macos")]
// Input handlers can update GpuixView, so dispatch without leasing the root view.
fn update_window_without_view<R>(
    update: impl FnOnce(&mut gpui::Window, &mut gpui::App) -> R,
) -> Result<R> {
    let window = GPUI_WINDOW
        .with(|window| *window.borrow())
        .ok_or_else(|| Error::from_reason("GPUI window is not initialized"))?;

    GPUI_APP.with(|app| {
        let app = app.borrow();
        let app = app
            .as_ref()
            .ok_or_else(|| Error::from_reason("GPUI application is not initialized"))?;
        app.update(|cx| {
            gpui::AnyWindowHandle::from(window)
                .update(cx, move |_view, window, cx| update(window, cx))
                .map_err(|error| Error::from_reason(error.to_string()))
        })
    })
}

#[cfg(target_os = "macos")]
fn invalidate_window() -> Result<()> {
    update_window(|_view, window, cx| {
        cx.notify();
        window.refresh();
    })
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
enum MouseInput {
    Click {
        x: f64,
        y: f64,
        button: u32,
        modifiers: gpui::Modifiers,
    },
    Down {
        x: f64,
        y: f64,
        button: u32,
        modifiers: gpui::Modifiers,
    },
    Up {
        x: f64,
        y: f64,
        button: u32,
        modifiers: gpui::Modifiers,
    },
    Move {
        x: f64,
        y: f64,
        pressed_button: Option<u32>,
        modifiers: gpui::Modifiers,
    },
    Wheel {
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
        modifiers: gpui::Modifiers,
    },
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
enum KeyInput {
    Keystrokes(String),
    Down { keystroke: String, is_held: bool },
    Up(String),
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
enum ClockControl {
    Pause,
    Set(f64),
    FastForward(f64),
    Resume,
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
enum UiCommand {
    Invalidate,
    ActivateWindow,
    SetWindowTitle(String),
    SetDebugFrameOverlay(gpui::DebugFrameOverlayMode),
    CycleDebugFrameOverlay {
        response: SyncSender<String>,
    },
    GetDebugFrameOverlay {
        response: SyncSender<String>,
    },
    GetDebugFrameOverlayStats {
        response: SyncSender<DebugFrameOverlayStats>,
    },
    ResetDebugFrameOverlayStats,
    ScrollTo {
        id: u64,
        x: f32,
        y: f32,
        behavior: scroll_motion::Behavior,
    },
    ScrollToItem {
        id: u64,
        index: usize,
        offset: f32,
    },
    ScrollIntoView {
        id: u64,
        block: scroll_into_view::Align,
        inline: scroll_into_view::Align,
        behavior: scroll_motion::Behavior,
        container: scroll_into_view::Container,
    },
    ViewTransitionCapture,
    ViewTransitionStart {
        options: String,
    },
    GetScrollOffset {
        id: u64,
        response: SyncSender<Option<[f64; 2]>>,
    },
    GetListScrollTop {
        id: u64,
        response: SyncSender<Option<[f64; 3]>>,
    },
    GetAutomationBounds {
        response: SyncSender<HashMap<u64, crate::automation::ElementBounds>>,
    },
    GetWindowSize {
        response: SyncSender<WindowSize>,
    },
    GetElementBounds {
        id: u64,
        response: SyncSender<Option<crate::automation::ElementBounds>>,
    },
    FocusElement(u64),
    FocusNext,
    FocusPrevious,
    SetWindowKeyEvents {
        key_down: bool,
        key_up: bool,
        event_id: u64,
    },
    ControlClock {
        control: ClockControl,
        response: SyncSender<f64>,
    },
    DispatchMouse {
        input: MouseInput,
        response: SyncSender<std::result::Result<(), String>>,
    },
    DispatchKey {
        input: KeyInput,
        response: SyncSender<std::result::Result<(), String>>,
    },
    #[cfg(all(target_os = "windows", feature = "test-support"))]
    CaptureScreenshot {
        path: String,
        response: SyncSender<std::result::Result<(), String>>,
    },
    Blur,
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
fn refresh_ui_window(
    window: gpui::WindowHandle<GpuixView>,
    cx: &mut gpui::AsyncApp,
) -> anyhow::Result<()> {
    window.update(cx, |_view, window, cx| {
        cx.notify();
        window.refresh();
    })
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
async fn run_ui_commands(
    mut commands: mpsc::UnboundedReceiver<UiCommand>,
    window: gpui::WindowHandle<GpuixView>,
    cx: &mut gpui::AsyncApp,
) {
    while let Some(command) = commands.next().await {
        let result = match command {
            UiCommand::Invalidate => refresh_ui_window(window, cx),
            UiCommand::ActivateWindow => window.update(cx, |_view, window, cx| {
                cx.activate(true);
                window.activate_window();
            }),
            UiCommand::SetWindowTitle(title) => window.update(cx, move |view, window, cx| {
                view.window_title = title;
                cx.notify();
                window.refresh();
            }),
            UiCommand::SetDebugFrameOverlay(mode) => {
                window.update(cx, move |_view, window, _cx| {
                    window.set_debug_frame_overlay_mode(mode);
                })
            }
            UiCommand::CycleDebugFrameOverlay { response } => {
                window.update(cx, move |_view, window, _cx| {
                    window.cycle_debug_frame_overlay_mode();
                    response
                        .send(
                            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).into(),
                        )
                        .ok();
                })
            }
            UiCommand::GetDebugFrameOverlay { response } => {
                window.update(cx, move |_view, window, _cx| {
                    response
                        .send(
                            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).into(),
                        )
                        .ok();
                })
            }
            UiCommand::GetDebugFrameOverlayStats { response } => {
                window.update(cx, move |_view, window, _cx| {
                    response
                        .send(debug_frame_overlay_stats_js(
                            window.debug_frame_overlay_stats(),
                        ))
                        .ok();
                })
            }
            UiCommand::ResetDebugFrameOverlayStats => window.update(cx, |_view, window, _cx| {
                window.reset_debug_frame_overlay_stats();
            }),
            UiCommand::ScrollTo { id, x, y, behavior } => {
                if !VIRTUAL_LIST_STATES.with(|cell| {
                    let states = cell.borrow();
                    let Some(state) = states.get(&id) else {
                        return false;
                    };
                    state.set_offset_from_scrollbar(gpui::point(gpui::px(x), gpui::px(y)));
                    true
                }) {
                    window
                        .update(cx, |view, _window, _cx| {
                            view.scroll_to_offset(id, x, y, behavior);
                        })
                        .ok();
                }
                refresh_ui_window(window, cx)
            }
            UiCommand::ScrollToItem { id, index, offset } => {
                if !VIRTUAL_LIST_STATES.with(|cell| {
                    if !cell.borrow().contains_key(&id) {
                        return false;
                    }
                    queue_virtual_list_scroll(id, index, offset);
                    true
                }) {
                    SCROLL_HANDLES.with(|cell| {
                        if let Some(handle) = cell.borrow().get(&id) {
                            handle.scroll_to_item(index);
                        }
                    });
                }
                refresh_ui_window(window, cx)
            }
            UiCommand::ScrollIntoView {
                id,
                block,
                inline,
                behavior,
                container,
            } => {
                window
                    .update(cx, |view, _window, _cx| {
                        let tree = view.tree.lock().unwrap();
                        scroll_into_view::scroll_into_view(
                            &tree,
                            id,
                            block,
                            inline,
                            behavior,
                            container,
                            |id| SCROLL_HANDLES.with(|cell| cell.borrow().get(&id).cloned()),
                        );
                    })
                    .ok();
                refresh_ui_window(window, cx)
            }
            UiCommand::ViewTransitionCapture => window.update(cx, |view, _window, _cx| {
                view.view_transition_capture();
            }),
            UiCommand::ViewTransitionStart { options } => {
                window
                    .update(cx, move |view, _window, _cx| {
                        if let Err(error) = view.view_transition_start(&options) {
                            log::warn!("Invalid view transition options: {error}");
                        }
                    })
                    .ok();
                refresh_ui_window(window, cx)
            }
            UiCommand::GetScrollOffset { id, response } => {
                let offset = VIRTUAL_LIST_STATES
                    .with(|cell| {
                        cell.borrow().get(&id).map(|state| {
                            let offset = state.scroll_px_offset_for_scrollbar();
                            offset_to_js(offset)
                        })
                    })
                    .or_else(|| {
                        SCROLL_HANDLES.with(|cell| {
                            cell.borrow().get(&id).map(|handle| {
                                let offset = handle.offset();
                                offset_to_js(offset)
                            })
                        })
                    });
                response.send(offset).ok();
                Ok(())
            }
            UiCommand::GetListScrollTop { id, response } => {
                let top = VIRTUAL_LIST_STATES.with(|cell| {
                    cell.borrow().get(&id).map(|state| {
                        let top = state.logical_scroll_top();
                        [
                            top.item_ix as f64,
                            f64::from(f32::from(top.offset_in_item)),
                            f64::from(f32::from(state.viewport_bounds().size.height)),
                        ]
                    })
                });
                response.send(top).ok();
                Ok(())
            }
            UiCommand::GetWindowSize { response } => {
                window.update(cx, move |_view, window, _cx| {
                    let size = window.viewport_size();
                    response
                        .send(WindowSize {
                            width: f32::from(size.width) as f64,
                            height: f32::from(size.height) as f64,
                        })
                        .ok();
                })
            }
            UiCommand::GetAutomationBounds { response } => {
                window.update(cx, move |_view, window, cx| {
                    cx.notify();
                    window.refresh();
                    window.on_next_frame(move |_window, _cx| {
                        response.send(crate::automation::all_bounds()).ok();
                    });
                })
            }
            UiCommand::GetElementBounds { id, response } => {
                window.update(cx, move |_view, window, cx| {
                    cx.notify();
                    window.refresh();
                    window.on_next_frame(move |_window, _cx| {
                        response.send(crate::automation::get_bounds(id)).ok();
                    });
                })
            }
            UiCommand::FocusElement(id) => window.update(cx, move |view, window, cx| {
                view.reveal_virtual_list_ancestor(id);
                if let Some(handle) = view.focus_handles.get(&id) {
                    handle.focus(window, cx);
                }
                cx.notify();
                window.refresh();
            }),
            UiCommand::FocusNext => window.update(cx, |_view, window, cx| window.focus_next(cx)),
            UiCommand::FocusPrevious => {
                window.update(cx, |_view, window, cx| window.focus_prev(cx))
            }
            UiCommand::SetWindowKeyEvents {
                key_down,
                key_up,
                event_id,
            } => window.update(cx, move |view, window, cx| {
                view.window_key_down = key_down;
                view.window_key_up = key_up;
                view.window_key_event_id = event_id;
                cx.notify();
                window.refresh();
            }),
            UiCommand::ControlClock { control, response } => {
                window.update(cx, move |view, _window, cx| {
                    let now_ms = match control {
                        ClockControl::Pause => view.clock.pause(),
                        ClockControl::Set(now_ms) => view.clock.set_ms(now_ms),
                        ClockControl::FastForward(delta_ms) => view.clock.fast_forward_ms(delta_ms),
                        ClockControl::Resume => view.clock.resume(),
                    };
                    cx.notify();
                    response.send(now_ms).ok();
                })
            }
            UiCommand::DispatchMouse { input, response } => {
                let result =
                    gpui::AnyWindowHandle::from(window).update(cx, move |_view, window, cx| {
                        match input {
                            MouseInput::Click {
                                x,
                                y,
                                button,
                                modifiers,
                            } => {
                                crate::automation::dispatch_click(
                                    window, cx, x, y, button, modifiers,
                                );
                            }
                            MouseInput::Down {
                                x,
                                y,
                                button,
                                modifiers,
                            } => {
                                crate::automation::dispatch_mouse_down(
                                    window, cx, x, y, button, modifiers,
                                );
                            }
                            MouseInput::Up {
                                x,
                                y,
                                button,
                                modifiers,
                            } => {
                                crate::automation::dispatch_mouse_up(
                                    window, cx, x, y, button, modifiers,
                                );
                            }
                            MouseInput::Move {
                                x,
                                y,
                                pressed_button,
                                modifiers,
                            } => {
                                crate::automation::dispatch_mouse_move(
                                    window,
                                    cx,
                                    x,
                                    y,
                                    pressed_button,
                                    modifiers,
                                );
                            }
                            MouseInput::Wheel {
                                x,
                                y,
                                delta_x,
                                delta_y,
                                modifiers,
                            } => {
                                crate::automation::dispatch_scroll_wheel(
                                    window, cx, x, y, delta_x, delta_y, modifiers,
                                );
                            }
                        }
                    });
                response
                    .send(
                        result
                            .as_ref()
                            .map(|_| ())
                            .map_err(|error| format!("{error:#}")),
                    )
                    .ok();
                result
            }
            UiCommand::DispatchKey { input, response } => {
                let result = gpui::AnyWindowHandle::from(window)
                    .update(cx, move |_view, window, cx| match input {
                        KeyInput::Keystrokes(keystrokes) => {
                            crate::automation::dispatch_keystrokes(window, cx, &keystrokes)
                        }
                        KeyInput::Down { keystroke, is_held } => {
                            crate::automation::dispatch_key_down(window, cx, &keystroke, is_held)
                        }
                        KeyInput::Up(keystroke) => {
                            crate::automation::dispatch_key_up(window, cx, &keystroke)
                        }
                    })
                    .and_then(|result| result.map_err(anyhow::Error::msg));
                response
                    .send(
                        result
                            .as_ref()
                            .map(|_| ())
                            .map_err(|error| format!("{error:#}")),
                    )
                    .ok();
                result
            }
            #[cfg(all(target_os = "windows", feature = "test-support"))]
            UiCommand::CaptureScreenshot { path, response } => {
                let error_response = response.clone();
                let result = window.update(cx, move |_view, window, cx| {
                    cx.notify();
                    window.refresh();
                    window.on_next_frame(move |window, _cx| {
                        let result = window
                            .render_to_image()
                            .map_err(|error| format!("Screenshot capture failed: {error}"))
                            .and_then(|image| {
                                image
                                    .save(&path)
                                    .map_err(|error| format!("Failed to save screenshot: {error}"))
                            });
                        response.send(result).ok();
                    });
                });
                if let Err(error) = &result {
                    error_response.send(Err(format!("{error:#}"))).ok();
                }
                result
            }
            UiCommand::Blur => window.update(cx, |_view, window, _cx| window.blur()),
        };
        if let Err(error) = result {
            if cx.update(|cx| cx.windows().is_empty()) {
                break;
            }
            log::error!("Failed to handle GPUI UI command: {error:#}");
        }
    }
    cx.update(|cx| cx.quit());
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}

/// JS-side values for environment overrides such as `GPUIX_SCROLLBARS`.
///
/// A map instead of `std::env::set_var`, because `setenv` races `getenv`
/// on another thread, and Windows and Linux paint on a dedicated UI thread.
fn env_overrides() -> &'static std::sync::Mutex<HashMap<String, Option<String>>> {
    static OVERRIDES: std::sync::OnceLock<std::sync::Mutex<HashMap<String, Option<String>>>> =
        std::sync::OnceLock::new();
    OVERRIDES.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Reads an override, or the real process environment when JS never set one.
pub(crate) fn env_var(key: &str) -> Option<String> {
    if let Some(value) = env_overrides().lock().unwrap().get(key) {
        return value.clone();
    }
    std::env::var(key).ok()
}

/// Records one `process.env` entry for `env_var` readers.
///
/// Rust reads overrides such as `GPUIX_SCROLLBARS` at paint. Node writes a
/// `process.env` assignment through to `setenv`, but Bun only updates its
/// JS snapshot. A caller on Bun must push the value across with this
/// function.
#[napi]
pub fn sync_env_var(key: String, value: Option<String>) {
    env_overrides().lock().unwrap().insert(key, value);
}

/// The main GPUI renderer exposed to Node.js.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[napi]
pub struct GpuixRenderer {
    event_callback: Mutex<Option<Arc<ThreadsafeFunction<EventPayload>>>>,
    tree: Arc<Mutex<RetainedTree>>,
    initialized: Arc<Mutex<bool>>,
    /// Shared with GpuixView so napi methods can read the live selection
    /// without an App context. Paint and napi calls can use different threads.
    selection: SharedSelection,
    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
    ui_commands: Mutex<Option<mpsc::UnboundedSender<UiCommand>>>,
    /// False after `Platform::run` returns. `tick()` reports that so JS can
    /// `process.exit`, matching macOS where `pump_events` returning false is
    /// the last-window-closed signal. The UI thread owns the Win32/Linux loop,
    /// so `tick()` cannot pump it; it only observes this flag.
    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
    ui_running: Arc<AtomicBool>,
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[napi]
impl GpuixRenderer {
    fn event_callback_for_view(&self) -> Option<EventCallback> {
        self.event_callback.lock().unwrap().clone().map(|tsf| {
            Arc::new(move |payload: EventPayload| {
                tsf.call(Ok(payload), ThreadsafeFunctionCallMode::NonBlocking);
            }) as EventCallback
        })
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
    fn send_ui_command(&self, command: UiCommand) -> Result<()> {
        self.ui_commands
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(|| Error::from_reason("GPUI application is not initialized"))?
            .unbounded_send(command)
            .map_err(|_| Error::from_reason("The GPUI UI thread is not running"))
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
    fn dispatch_mouse_input(&self, input: MouseInput) -> Result<()> {
        let (response_sender, response_receiver) = sync_channel(1);
        self.send_ui_command(UiCommand::DispatchMouse {
            input,
            response: response_sender,
        })?;
        recv_ui_response(response_receiver, "the GPUI UI command")?.map_err(Error::from_reason)
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
    fn dispatch_key_input(&self, input: KeyInput) -> Result<()> {
        let (response_sender, response_receiver) = sync_channel(1);
        self.send_ui_command(UiCommand::DispatchKey {
            input,
            response: response_sender,
        })?;
        recv_ui_response(response_receiver, "the GPUI key command")?.map_err(Error::from_reason)
    }

    fn automation_bounds(&self) -> Result<HashMap<u64, crate::automation::ElementBounds>> {
        #[cfg(target_os = "macos")]
        return Ok(crate::automation::all_bounds());

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::GetAutomationBounds { response })?;
            return recv_ui_response(receiver, "the automation bounds query");
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    fn element_bounds(&self, id: u64) -> Result<Option<crate::automation::ElementBounds>> {
        #[cfg(target_os = "macos")]
        return Ok(crate::automation::get_bounds(id));

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::GetElementBounds { id, response })?;
            return recv_ui_response(receiver, "the element bounds query");
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = id;
            Err(Error::from_reason("Unsupported operating system"))
        }
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
    fn control_clock(&self, control: ClockControl) -> Result<f64> {
        let (response, receiver) = sync_channel(1);
        self.send_ui_command(UiCommand::ControlClock { control, response })?;
        recv_ui_response(receiver, "the automation clock command")
    }

    fn request_invalidate(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        return invalidate_window();

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::Invalidate);

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason(
            "The production GPUIX renderer does not support this operating system",
        ))
    }

    #[napi(constructor)]
    pub fn new(event_callback: Option<ThreadsafeFunction<EventPayload>>) -> Self {
        let _ = env_logger::try_init();
        Self {
            event_callback: Mutex::new(event_callback.map(Arc::new)),
            tree: Arc::new(Mutex::new(RetainedTree::new())),
            initialized: Arc::new(Mutex::new(false)),
            selection: SharedSelection::default(),
            #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
            ui_commands: Mutex::new(None),
            #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
            ui_running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Initialize GPUI using the native event-loop architecture for this OS.
    #[napi]
    pub fn init(&self, options: Option<WindowOptions>) -> Result<()> {
        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = options;
            return Err(Error::from_reason(
                "The production GPUIX renderer does not support this operating system",
            ));
        }

        #[cfg(target_os = "macos")]
        return self.init_macos(options);

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.init_threaded(options);
    }

    #[cfg(target_os = "macos")]
    fn init_macos(&self, options: Option<WindowOptions>) -> Result<()> {
        let options = options.unwrap_or_default();

        {
            let initialized = self.initialized.lock().unwrap();
            if *initialized {
                return Err(Error::from_reason("Renderer is already initialized"));
            }
        }
        if MAC_PLATFORM.with(|platform| platform.borrow().is_some()) {
            return Err(Error::from_reason(
                "A GPUI application already exists on this thread",
            ));
        }

        let width = options.width.unwrap_or(800.0);
        let height = options.height.unwrap_or(600.0);
        let title = options.title.clone().unwrap_or_else(|| "GPUIX".to_string());
        let app_name = options.app_name.clone().unwrap_or_else(|| title.clone());
        // `focus: false` must also skip `cx.activate`: the window flag only
        // decides key status inside the app, activation is what steals focus.
        let activate = options.focus.unwrap_or(true);
        let window_options = options.clone();

        let platform = Rc::new(gpui_macos::MacPlatform::new_embedded());

        let tree = self.tree.clone();
        let callback = self.event_callback_for_view();

        let selection = self.selection.clone();
        let opened_window = Rc::new(RefCell::new(None));
        let startup_error = Rc::new(RefCell::new(None));
        let opened_window_for_app = opened_window.clone();
        let startup_error_for_app = startup_error.clone();
        // bun/node is not a .app. A Dock icon with no window cannot relaunch.
        // Last window close quits AppKit; tick() returns false and JS exits.
        let app = gpui::Application::with_platform(platform.clone())
            .with_quit_mode(gpui::QuitMode::LastWindowClosed);
        let app_handle = app.run_embedded(move |cx: &mut gpui::App| {
            crate::custom_elements::input::init(cx);
            // After the other bindings: `set_menus` reads key equivalents out of
            // the keymap, so every binding must exist before it runs.
            crate::app_menu::init(&app_name, cx);
            let bounds = gpui::Bounds::centered(
                None,
                gpui::size(gpui::px(width as f32), gpui::px(height as f32)),
                cx,
            );

            match cx.open_window(
                to_gpui_window_options(&window_options, bounds),
                |_window, cx| {
                    cx.new(|_cx| {
                        GpuixView::new(tree.clone(), callback.clone(), title, selection.clone())
                    })
                },
            ) {
                Ok(window_handle) => {
                    *opened_window_for_app.borrow_mut() = Some(window_handle);
                    if activate {
                        cx.activate(true);
                    }
                }
                Err(error) => {
                    *startup_error_for_app.borrow_mut() = Some(error.to_string());
                }
            }
        });

        let startup_result = match startup_error.borrow_mut().take() {
            Some(error) => Err(Error::from_reason(format!(
                "Failed to open the GPUI window: {error}"
            ))),
            None => opened_window
                .borrow_mut()
                .take()
                .ok_or_else(|| Error::from_reason("GPUI did not open the application window")),
        };
        let window_handle = match startup_result {
            Ok(window_handle) => window_handle,
            Err(error) => {
                app_handle.update(|cx| cx.quit());
                if platform.pump_events() {
                    MAC_PLATFORM.with(|stored| {
                        *stored.borrow_mut() = Some(platform.clone());
                    });
                }
                return Err(error);
            }
        };

        MAC_PLATFORM.with(|stored| {
            *stored.borrow_mut() = Some(platform);
        });
        GPUI_APP.with(|a| {
            *a.borrow_mut() = Some(app_handle);
        });
        GPUI_WINDOW.with(|w| {
            *w.borrow_mut() = Some(window_handle);
        });

        *self.initialized.lock().unwrap() = true;
        self.event_callback.lock().unwrap().take();
        Ok(())
    }

    // GPUI declares PerMonitorV2 only in the host exe manifest (zed#8936).
    // A napi .node is loaded by node.exe/bun.exe, so that manifest never applies.
    // Call on gpuix-ui only, before WindowsPlatform::new. Never on the Node thread:
    // the V2 fallback is SetThreadDpiAwarenessContext, which would change bun itself.
    #[cfg(target_os = "windows")]
    fn enable_per_monitor_dpi() {
        use windows::Win32::UI::HiDpi::{
            AreDpiAwarenessContextsEqual, GetThreadDpiAwarenessContext,
            SetProcessDpiAwarenessContext, SetThreadDpiAwarenessContext,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };

        unsafe {
            let current = GetThreadDpiAwarenessContext();
            if AreDpiAwarenessContextsEqual(current, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
                .as_bool()
            {
                return;
            }
            if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_ok() {
                return;
            }
            if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE).is_ok() {
                return;
            }
            // Process awareness is already locked (node/bun manifest). This thread has no HWND yet.
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
    fn init_threaded(&self, options: Option<WindowOptions>) -> Result<()> {
        let options = options.unwrap_or_default();
        if *self.initialized.lock().unwrap() {
            return Err(Error::from_reason("Renderer is already initialized"));
        }

        let width = options.width.unwrap_or(800.0);
        let height = options.height.unwrap_or(600.0);
        let title = options.title.clone().unwrap_or_else(|| "GPUIX".to_string());
        // `focus: false` must also skip `cx.activate`: the window flag only
        // decides key status inside the app, activation is what steals focus.
        let activate = options.focus.unwrap_or(true);
        let window_options = options.clone();
        let tree = self.tree.clone();
        let selection = self.selection.clone();
        let callback = self.event_callback_for_view();
        let (command_sender, command_receiver) = mpsc::unbounded();
        let (startup_sender, startup_receiver) = sync_channel(1);
        let exit_startup_sender = startup_sender.clone();
        let ui_running = self.ui_running.clone();
        let ui_running_for_run = ui_running.clone();

        std::thread::Builder::new()
            .name("gpuix-ui".to_string())
            .spawn(move || {
                #[cfg(target_os = "windows")]
                Self::enable_per_monitor_dpi();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    // Default is already LastWindowClosed on Windows/Linux.
                    // Set it anyway so a GPUI default change cannot leave bun
                    // running after the last window closes, as on macOS.
                    gpui_platform::application()
                        .with_quit_mode(gpui::QuitMode::LastWindowClosed)
                        .run(move |cx| {
                            crate::custom_elements::input::init(cx);
                            let bounds = gpui::Bounds::centered(
                                None,
                                gpui::size(gpui::px(width as f32), gpui::px(height as f32)),
                                cx,
                            );
                            let window = match cx.open_window(
                                to_gpui_window_options(&window_options, bounds),
                                |_window, cx| {
                                    cx.new(|_| GpuixView::new(tree, callback, title, selection))
                                },
                            ) {
                                Ok(window) => window,
                                Err(error) => {
                                    startup_sender
                                        .send(Err(format!(
                                            "Failed to open the GPUI window: {error}"
                                        )))
                                        .ok();
                                    cx.quit();
                                    return;
                                }
                            };

                            cx.spawn(async move |cx| {
                                run_ui_commands(command_receiver, window, cx).await;
                            })
                            .detach();
                            if activate {
                                cx.activate(true);
                            }
                            ui_running_for_run.store(true, Ordering::Release);
                            startup_sender.send(Ok(())).ok();
                        });
                }));
                ui_running.store(false, Ordering::Release);

                let error = match result {
                    Ok(()) => {
                        "The GPUI event loop exited before initialization completed".to_string()
                    }
                    Err(payload) => format!(
                        "The GPUI UI thread panicked during initialization: {}",
                        panic_message(payload)
                    ),
                };
                exit_startup_sender.try_send(Err(error)).ok();
            })
            .map_err(|error| {
                Error::from_reason(format!("Failed to spawn the GPUI UI thread: {error}"))
            })?;

        startup_receiver
            .recv()
            .map_err(|_| Error::from_reason("The GPUI UI thread stopped during initialization"))?
            .map_err(Error::from_reason)?;

        *self.ui_commands.lock().unwrap() = Some(command_sender);
        *self.initialized.lock().unwrap() = true;
        self.event_callback.lock().unwrap().take();
        Ok(())
    }

    /// Apply a batch of mutations in a single FFI call.
    ///
    /// Accepts a JSON array of mutation tuples. Each tuple is an array where
    /// the first element is the operation name (string) and remaining elements
    /// are the arguments:
    ///
    ///   ["createElement",    id, "type"]
    ///   ["destroyElement",   id]
    ///   ["appendChild",      parentId, childId]
    ///   ["insertBefore",     parentId, childId, beforeId]
    ///   ["setStyle",         id, { ...style }]
    ///   ["setText",          id, "content"]
    ///   ["setEventListener", id, "eventType", true|false]
    ///   ["setRoot",          id]
    ///   ["setCustomProp",    id, "key", value]
    ///
    /// Returns accumulated destroyed IDs from all destroyElement ops.
    /// Acquires the tree mutex ONCE for the entire batch.
    #[napi]
    pub fn apply_batch(&self, json: String) -> Result<Vec<f64>> {
        let mut tree = self.tree.lock().unwrap();
        let destroyed =
            apply_batch_to_tree(&mut tree, json.as_bytes()).map_err(Error::from_reason)?;
        drop(tree);
        self.request_invalidate()?;
        Ok(destroyed)
    }

    // ── Frame loop ───────────────────────────────────────────────────

    /// Pump the native event loop. Returns false after the last window closes.
    #[napi]
    pub fn tick(&self) -> Result<bool> {
        let initialized = *self.initialized.lock().unwrap();
        if !initialized {
            return Err(Error::from_reason(
                "Renderer not initialized. Call init() first.",
            ));
        }

        #[cfg(target_os = "macos")]
        {
            let running = MAC_PLATFORM.with(|p| {
                p.borrow()
                    .as_ref()
                    .map(|platform| platform.pump_events())
                    .unwrap_or(false)
            });
            return Ok(running);
        }

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let running = self.ui_running.load(Ordering::Acquire);
            if !running {
                self.ui_commands.lock().unwrap().take();
            }
            return Ok(running);
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason(
            "The production GPUIX renderer does not support this operating system",
        ))
    }

    #[napi]
    pub fn is_initialized(&self) -> bool {
        *self.initialized.lock().unwrap()
    }

    /// Whether JavaScript must call tick() until it returns false.
    ///
    /// macOS: tick() pumps AppKit. Windows/Linux: tick() only reports whether
    /// the UI thread is still inside `Platform::run`. Both return false after
    /// the last window closes so the JS frame loop can exit the process.
    #[napi]
    pub fn requires_tick(&self) -> bool {
        cfg!(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        ))
    }

    /// The paintable size of the window in logical pixels, excluding any
    /// platform title bar. This used to answer a hardcoded 800x600, so anything
    /// that turned a mouse position into layout coordinates pointed at the
    /// wrong place on every window that was not exactly that size.
    #[napi]
    pub fn get_window_size(&self) -> Result<WindowSize> {
        #[cfg(target_os = "macos")]
        return update_window(|_view, window, _cx| {
            let size = window.viewport_size();
            WindowSize {
                width: f32::from(size.width) as f64,
                height: f32::from(size.height) as f64,
            }
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::GetWindowSize { response })?;
            return recv_ui_response(receiver, "the window size query");
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason(
            "The production GPUIX renderer does not support this operating system",
        ))
    }

    #[napi]
    pub fn get_window_insets(&self) -> Result<WindowInsets> {
        #[cfg(target_os = "macos")]
        return update_window(|_view, window, _cx| WindowInsets::from_gpui(window.insets()));

        #[cfg(not(target_os = "macos"))]
        Ok(WindowInsets::default())
    }

    /// `"hidden"` | `"minimal"` | `"full"`. Paints into the scene after layout.
    #[napi]
    pub fn set_debug_frame_overlay(&self, mode: String) -> Result<String> {
        let mode = parse_debug_frame_overlay_mode(&mode)?;
        #[cfg(target_os = "macos")]
        return update_window(move |_view, window, _cx| {
            window.set_debug_frame_overlay_mode(mode);
            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).to_string()
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            self.send_ui_command(UiCommand::SetDebugFrameOverlay(mode))?;
            return self.debug_frame_overlay_mode();
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Scroll every ancestor scroll box so the element shows, like the
    /// web `scrollIntoView`. `block` places it on the y axis and
    /// `inline` on the x axis: `start`, `center`, `end` or `nearest`.
    /// The defaults match the web: `start` and `nearest`. The
    /// `scroll-margin` of the element and the `scroll-padding` of each
    /// box apply. `container: "nearest"` scrolls only the nearest scroll
    /// box, like the web option, so the outer view stays put.
    #[napi]
    pub fn scroll_into_view(
        &self,
        element_id: f64,
        block: Option<String>,
        inline: Option<String>,
        behavior: Option<String>,
        container: Option<String>,
    ) -> Result<()> {
        let id = to_element_id(element_id)?;
        let block = scroll_into_view::Align::parse(block.as_deref(), scroll_into_view::Align::Start);
        let inline =
            scroll_into_view::Align::parse(inline.as_deref(), scroll_into_view::Align::Nearest);
        let behavior = scroll_motion::Behavior::parse(behavior.as_deref());
        let container = scroll_into_view::Container::parse(container.as_deref());
        #[cfg(target_os = "macos")]
        {
            let tree = self.tree.lock().unwrap();
            scroll_into_view::scroll_into_view(&tree, id, block, inline, behavior, container, |id| {
                SCROLL_HANDLES.with(|cell| cell.borrow().get(&id).cloned())
            });
            drop(tree);
            return invalidate_window();
        }

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::ScrollIntoView {
            id,
            block,
            inline,
            behavior,
            container,
        });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Clone every element that has a `viewTransitionName`, with its painted
    /// bounds. Call this before the React update, then `viewTransitionStart`
    /// after it. `startViewTransition` in `@gpuix/react` does both.
    #[napi]
    pub fn view_transition_capture(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        return update_window(|view, _window, _cx| view.view_transition_capture());

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::ViewTransitionCapture);

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Animate every captured name toward its new element. `options` is the
    /// JSON of a `ViewTransitionOptions` value, or nothing for a crossfade.
    #[napi]
    pub fn view_transition_start(&self, options: Option<String>) -> Result<()> {
        let options = options.unwrap_or_else(|| "{}".to_string());
        #[cfg(target_os = "macos")]
        {
            update_window(move |view, _window, _cx| {
                view.view_transition_start(&options).map_err(Error::from_reason)
            })??;
            return invalidate_window();
        }

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::ViewTransitionStart { options });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Hidden → minimal → full → hidden.
    #[napi]
    pub fn cycle_debug_frame_overlay(&self) -> Result<String> {
        #[cfg(target_os = "macos")]
        return update_window(move |_view, window, _cx| {
            window.cycle_debug_frame_overlay_mode();
            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).to_string()
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::CycleDebugFrameOverlay { response })?;
            return recv_ui_response(receiver, "the debug frame overlay query");
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    #[napi]
    pub fn get_debug_frame_overlay(&self) -> Result<String> {
        self.debug_frame_overlay_mode()
    }

    fn debug_frame_overlay_mode(&self) -> Result<String> {
        #[cfg(target_os = "macos")]
        return update_window(|_view, window, _cx| {
            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).to_string()
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::GetDebugFrameOverlay { response })?;
            recv_ui_response(receiver, "the debug frame overlay query")
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Clears the last 1000 draw samples. Frame count stays.
    #[napi]
    pub fn reset_debug_frame_overlay_stats(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        return update_window(|_view, window, _cx| {
            window.reset_debug_frame_overlay_stats();
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::ResetDebugFrameOverlayStats);

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Same numbers as the on-screen overlay: current, p90, p99, max, frames.
    #[napi]
    pub fn get_debug_frame_overlay_stats(&self) -> Result<DebugFrameOverlayStats> {
        #[cfg(target_os = "macos")]
        return update_window(|_view, window, _cx| {
            debug_frame_overlay_stats_js(window.debug_frame_overlay_stats())
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::GetDebugFrameOverlayStats { response })?;
            match receiver.recv_timeout(Duration::from_secs(2)) {
                Ok(stats) => Ok(stats),
                Err(RecvTimeoutError::Timeout) => Err(Error::from_reason(
                    "Timed out after 2 seconds waiting for debug frame overlay stats",
                )),
                Err(RecvTimeoutError::Disconnected) => Err(Error::from_reason(
                    "The GPUI UI thread stopped during the debug frame overlay stats query",
                )),
            }
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Bring the window forward and give it focus. This is how a window opened
    /// with `show: false` or `focus: false` is revealed later.
    #[napi]
    pub fn activate_window(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        return update_window(|_view, window, cx| {
            cx.activate(true);
            window.activate_window();
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::ActivateWindow);

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason(
            "The production GPUIX renderer does not support this operating system",
        ))
    }

    #[napi]
    pub fn set_window_title(&self, title: String) -> Result<()> {
        #[cfg(target_os = "macos")]
        return update_window(move |view, window, cx| {
            view.window_title = title;
            cx.notify();
            window.refresh();
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::SetWindowTitle(title));

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason(
            "The production GPUIX renderer does not support this operating system",
        ))
    }

    #[napi]
    pub fn focus_element(&self, element_id: f64) -> Result<()> {
        let id = to_element_id(element_id)?;
        #[cfg(target_os = "macos")]
        return update_window(move |view, window, cx| {
            view.reveal_virtual_list_ancestor(id);
            if let Some(handle) = view.focus_handles.get(&id) {
                handle.focus(window, cx);
            }
            cx.notify();
            window.refresh();
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::FocusElement(id));

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Move focus to the next GPUI tab stop.
    #[napi]
    pub fn focus_next(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        return update_window(|_view, window, cx| window.focus_next(cx));

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::FocusNext);

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Move focus to the previous GPUI tab stop.
    #[napi]
    pub fn focus_previous(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        return update_window(|_view, window, cx| window.focus_prev(cx));

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::FocusPrevious);

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Enable the window key events requested by the React renderer.
    #[napi]
    pub fn set_window_key_events(&self, key_down: bool, key_up: bool, event_id: f64) -> Result<()> {
        let event_id = to_element_id(event_id)?;
        #[cfg(target_os = "macos")]
        return update_window(move |view, window, cx| {
            view.window_key_down = key_down;
            view.window_key_up = key_up;
            view.window_key_event_id = event_id;
            cx.notify();
            window.refresh();
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::SetWindowKeyEvents {
            key_down,
            key_up,
            event_id,
        });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    #[napi]
    pub fn blur(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        return update_window(move |_view, window, _cx| window.blur());

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::Blur);

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    // ── Selection API ────────────────────────────────────────────────

    /// The current text selection joined in document order, or null.
    #[napi]
    pub fn get_selected_text(&self) -> Option<String> {
        self.selection.lock().selected_text()
    }

    /// Drop the current selection and request a repaint.
    #[napi]
    pub fn clear_selection(&self) -> Result<()> {
        self.selection.lock().clear();
        self.request_invalidate()
    }

    // ── Scroll API ───────────────────────────────────────────────────
    // GpuixView syncs scroll handles and virtual list states to thread-local maps.

    /// Set the scroll offset of a scrollable element.
    /// x and y are negative pixel values (scroll down = more negative y).
    /// `behavior` is `auto`, `instant` or `smooth`, like the web `scrollTo`
    /// option. `auto` reads the `scroll-behavior` of the box.
    #[napi]
    pub fn scroll_to(&self, element_id: f64, x: f64, y: f64, behavior: Option<String>) -> Result<()> {
        let id = to_element_id(element_id)?;
        let behavior = scroll_motion::Behavior::parse(behavior.as_deref());
        #[cfg(target_os = "macos")]
        if !VIRTUAL_LIST_STATES.with(|cell| {
            let states = cell.borrow();
            let Some(state) = states.get(&id) else {
                return false;
            };
            state.set_offset_from_scrollbar(gpui::point(gpui::px(x as f32), gpui::px(y as f32)));
            true
        }) {
            let smooth = {
                let tree = self.tree.lock().unwrap();
                behavior.smooth(tree.elements.get(&id).and_then(|el| el.style.as_deref()))
            };
            SCROLL_HANDLES.with(|cell| {
                let handles = cell.borrow();
                let to = gpui::point(gpui::px(x as f32), gpui::px(y as f32));
                if let Some(handle) = handles.get(&id) {
                    if smooth {
                        scroll_motion::animate(id, handle, to);
                    } else {
                        handle.set_offset(to);
                    }
                } else {
                    // The element has not painted yet, so it has no scroll
                    // handle. The offset waits until the frame that creates
                    // the handle, and applies in one step there.
                    scroll_motion::defer(id, to);
                }
            });
        }
        #[cfg(target_os = "macos")]
        return invalidate_window();

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::ScrollTo {
            id,
            x: x as f32,
            y: y as f32,
            behavior,
        });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Scroll a child into view by its index in the children list.
    ///
    /// For a `<virtual-list>` the scroll is queued and applied on the next
    /// render, after that frame's child splice, so indices computed against a
    /// just-committed child list are never shifted twice. `offsetInItem` is in
    /// pixels and may be negative, which anchors the viewport top above the
    /// item and resolves against measured heights at layout time.
    #[napi]
    pub fn scroll_to_item(
        &self,
        element_id: f64,
        index: f64,
        offset_in_item: Option<f64>,
    ) -> Result<()> {
        let id = to_element_id(element_id)?;
        let index = index as usize;
        let offset = offset_in_item.unwrap_or(0.0) as f32;
        #[cfg(target_os = "macos")]
        if !VIRTUAL_LIST_STATES.with(|cell| {
            if !cell.borrow().contains_key(&id) {
                return false;
            }
            queue_virtual_list_scroll(id, index, offset);
            true
        }) {
            SCROLL_HANDLES.with(|cell| {
                let handles = cell.borrow();
                if let Some(handle) = handles.get(&id) {
                    handle.scroll_to_item(index);
                }
            });
        }
        #[cfg(target_os = "macos")]
        return invalidate_window();

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::ScrollToItem { id, index, offset });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = offset;
            Err(Error::from_reason("Unsupported operating system"))
        }
    }

    /// The logical scroll anchor of a `<virtual-list>`:
    /// `[itemIndex, offsetInItemPx, viewportHeightPx]`, or null for anything
    /// else. `itemIndex == item count` is gpui's at-end sentinel.
    ///
    /// Unlike `getScrollOffset` this is exact even while row heights are still
    /// estimates, because it is the anchor gpui itself scrolls by.
    #[napi]
    pub fn get_list_scroll_top(&self, element_id: f64) -> Result<Option<Vec<f64>>> {
        let id = to_element_id(element_id)?;
        #[cfg(target_os = "macos")]
        return Ok(VIRTUAL_LIST_STATES.with(|cell| {
            cell.borrow().get(&id).map(|state| {
                let top = state.logical_scroll_top();
                vec![
                    top.item_ix as f64,
                    f64::from(f32::from(top.offset_in_item)),
                    f64::from(f32::from(state.viewport_bounds().size.height)),
                ]
            })
        }));

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::GetListScrollTop { id, response })?;
            return Ok(
                recv_ui_response(receiver, "the GPUI list scroll query")?.map(|top| top.to_vec())
            );
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Get the current scroll offset of a scrollable element.
    /// Returns [x, y] or null if the element has no scroll handle.
    #[napi]
    pub fn get_scroll_offset(&self, element_id: f64) -> Result<Option<Vec<f64>>> {
        let id = to_element_id(element_id)?;
        #[cfg(target_os = "macos")]
        return Ok(VIRTUAL_LIST_STATES
            .with(|cell| {
                cell.borrow().get(&id).map(|state| {
                    let offset = state.scroll_px_offset_for_scrollbar();
                    offset_to_js(offset).to_vec()
                })
            })
            .or_else(|| {
                SCROLL_HANDLES.with(|cell| {
                    let handles = cell.borrow();
                    handles.get(&id).map(|handle| {
                        let offset = handle.offset();
                        offset_to_js(offset).to_vec()
                    })
                })
            }));

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::GetScrollOffset { id, response })?;
            return Ok(
                recv_ui_response(receiver, "the GPUI scroll query")?.map(|[x, y]| vec![x, y])
            );
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    #[napi]
    pub fn get_automation_tree(&self) -> Result<String> {
        self.request_invalidate()?;
        let bounds = self.automation_bounds()?;
        let tree = self.tree.lock().unwrap();
        let json = tree.to_automation_json(&bounds);
        serde_json::to_string(&json)
            .map_err(|e| Error::from_reason(format!("JSON serialization failed: {}", e)))
    }

    #[napi]
    pub fn get_element_bounds(&self, id: f64) -> Result<Option<Vec<f64>>> {
        let id = to_element_id(id)?;
        Ok(self
            .element_bounds(id)?
            .map(|bounds| vec![bounds.x, bounds.y, bounds.width, bounds.height]))
    }

    #[napi]
    pub fn get_all_text(&self) -> Vec<String> {
        let tree = self.tree.lock().unwrap();
        let mut texts = Vec::new();
        if let Some(root_id) = tree.root_id {
            collect_text(root_id, &tree, &mut texts);
        }
        texts
    }

    #[napi]
    pub fn get_painted_text(&self) -> Vec<String> {
        crate::text::painted_text()
    }

    /// Every highlight wash painted in the last frame, in paint order.
    ///
    /// A quad is invisible to `getPaintedText()`, so this is the only way to
    /// assert on `highlight` without a screenshot.
    #[napi]
    pub fn get_painted_highlights(&self) -> Vec<crate::element_tree::HighlightMatch> {
        crate::text::painted_highlights()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Simulate space-separated keystrokes through the focused element's input pipeline.
    #[napi]
    pub fn simulate_keystrokes(&self, keystrokes: String) -> Result<()> {
        #[cfg(target_os = "macos")]
        return update_window_without_view(move |window, cx| {
            crate::automation::dispatch_keystrokes(window, cx, &keystrokes)
        })?
        .map_err(Error::from_reason);

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.dispatch_key_input(KeyInput::Keystrokes(keystrokes));

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = keystrokes;
            Err(Error::from_reason("Unsupported operating system"))
        }
    }

    #[napi]
    pub fn simulate_key_down(&self, keystroke: String, is_held: Option<bool>) -> Result<()> {
        let is_held = is_held.unwrap_or(false);

        #[cfg(target_os = "macos")]
        return update_window_without_view(move |window, cx| {
            crate::automation::dispatch_key_down(window, cx, &keystroke, is_held)
        })?
        .map_err(Error::from_reason);

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.dispatch_key_input(KeyInput::Down { keystroke, is_held });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = (keystroke, is_held);
            Err(Error::from_reason("Unsupported operating system"))
        }
    }

    #[napi]
    pub fn simulate_key_up(&self, keystroke: String) -> Result<()> {
        #[cfg(target_os = "macos")]
        return update_window_without_view(move |window, cx| {
            crate::automation::dispatch_key_up(window, cx, &keystroke)
        })?
        .map_err(Error::from_reason);

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.dispatch_key_input(KeyInput::Up(keystroke));

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = keystroke;
            Err(Error::from_reason("Unsupported operating system"))
        }
    }

    /// `modifiers` uses the `press()` syntax: "cmd", "cmd-shift", "alt".
    #[napi]
    pub fn simulate_click(
        &self,
        x: f64,
        y: f64,
        button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<()> {
        let button = button.unwrap_or(0);
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());

        #[cfg(target_os = "macos")]
        return update_window_without_view(move |window, cx| {
            crate::automation::dispatch_click(window, cx, x, y, button, modifiers);
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.dispatch_mouse_input(MouseInput::Click {
            x,
            y,
            button,
            modifiers,
        });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = (x, y, button);
            Err(Error::from_reason(
                "The production GPUIX renderer does not support this operating system",
            ))
        }
    }

    #[napi]
    pub fn simulate_mouse_down(
        &self,
        x: f64,
        y: f64,
        button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<()> {
        let button = button.unwrap_or(0);
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());

        #[cfg(target_os = "macos")]
        return update_window_without_view(move |window, cx| {
            crate::automation::dispatch_mouse_down(window, cx, x, y, button, modifiers);
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.dispatch_mouse_input(MouseInput::Down {
            x,
            y,
            button,
            modifiers,
        });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = (x, y, button);
            Err(Error::from_reason(
                "The production GPUIX renderer does not support this operating system",
            ))
        }
    }

    #[napi]
    pub fn simulate_mouse_up(
        &self,
        x: f64,
        y: f64,
        button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<()> {
        let button = button.unwrap_or(0);
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());

        #[cfg(target_os = "macos")]
        return update_window_without_view(move |window, cx| {
            crate::automation::dispatch_mouse_up(window, cx, x, y, button, modifiers);
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.dispatch_mouse_input(MouseInput::Up {
            x,
            y,
            button,
            modifiers,
        });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = (x, y, button);
            Err(Error::from_reason(
                "The production GPUIX renderer does not support this operating system",
            ))
        }
    }

    #[napi]
    pub fn simulate_mouse_move(
        &self,
        x: f64,
        y: f64,
        pressed_button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<()> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());

        #[cfg(target_os = "macos")]
        return update_window_without_view(move |window, cx| {
            crate::automation::dispatch_mouse_move(window, cx, x, y, pressed_button, modifiers);
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.dispatch_mouse_input(MouseInput::Move {
            x,
            y,
            pressed_button,
            modifiers,
        });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = (x, y, pressed_button);
            Err(Error::from_reason(
                "The production GPUIX renderer does not support this operating system",
            ))
        }
    }

    /// Dispatch a wheel event through the same GPUI hit test the trackpad uses.
    /// Deltas are pixels: negative `delta_y` scrolls down, negative `delta_x`
    /// pans right, matching `TestGpuixRenderer::simulate_scroll_wheel`.
    #[napi]
    pub fn simulate_scroll_wheel(
        &self,
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
        modifiers: Option<String>,
    ) -> Result<()> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());

        #[cfg(target_os = "macos")]
        return update_window_without_view(move |window, cx| {
            crate::automation::dispatch_scroll_wheel(window, cx, x, y, delta_x, delta_y, modifiers);
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.dispatch_mouse_input(MouseInput::Wheel {
            x,
            y,
            delta_x,
            delta_y,
            modifiers,
        });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = (x, y, delta_x, delta_y, modifiers);
            Err(Error::from_reason(
                "The production GPUIX renderer does not support this operating system",
            ))
        }
    }

    #[napi]
    pub fn clock_pause(&self) -> Result<f64> {
        #[cfg(target_os = "macos")]
        return update_window(move |view, _window, cx| {
            let now_ms = view.clock.pause();
            cx.notify();
            now_ms
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.control_clock(ClockControl::Pause);

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    #[napi]
    pub fn clock_set(&self, now_ms: f64) -> Result<f64> {
        #[cfg(target_os = "macos")]
        return update_window(move |view, _window, cx| {
            let now_ms = view.clock.set_ms(now_ms);
            cx.notify();
            now_ms
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.control_clock(ClockControl::Set(now_ms));

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = now_ms;
            Err(Error::from_reason("Unsupported operating system"))
        }
    }

    #[napi]
    pub fn clock_fast_forward(&self, delta_ms: f64) -> Result<f64> {
        #[cfg(target_os = "macos")]
        return update_window(move |view, _window, cx| {
            let now_ms = view.clock.fast_forward_ms(delta_ms);
            cx.notify();
            now_ms
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.control_clock(ClockControl::FastForward(delta_ms));

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = delta_ms;
            Err(Error::from_reason("Unsupported operating system"))
        }
    }

    #[napi]
    pub fn clock_resume(&self) -> Result<f64> {
        #[cfg(target_os = "macos")]
        return update_window(move |view, _window, cx| {
            let now_ms = view.clock.resume();
            cx.notify();
            now_ms
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.control_clock(ClockControl::Resume);

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    #[napi]
    pub fn capture_screenshot(&self, path: String) -> Result<()> {
        #[cfg(all(target_os = "macos", feature = "test-support"))]
        {
            let image = update_window(move |_view, window, cx| {
                cx.notify();
                window.refresh();
                window.render_to_image()
            })?
            .map_err(|e| Error::from_reason(format!("Screenshot capture failed: {}", e)))?;
            image
                .save(&path)
                .map_err(|e| Error::from_reason(format!("Failed to save screenshot: {}", e)))?;
            Ok(())
        }

        #[cfg(all(target_os = "windows", feature = "test-support"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::CaptureScreenshot { path, response })?;
            return recv_ui_response(receiver, "screenshot capture")?.map_err(Error::from_reason);
        }

        #[cfg(not(all(
            feature = "test-support",
            any(target_os = "macos", target_os = "windows")
        )))]
        {
            let _ = path;
            Err(Error::from_reason(
                "captureScreenshot needs a test-support build on macOS or Windows",
            ))
        }
    }
}

fn collect_text(id: u64, tree: &RetainedTree, texts: &mut Vec<String>) {
    if let Some(element) = tree.elements.get(&id) {
        if let Some(ref content) = element.content {
            texts.push(content.clone());
        }
        for &child_id in &element.children {
            collect_text(child_id, tree, texts);
        }
    }
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn start_web_app(
    tree: Arc<Mutex<RetainedTree>>,
    selection: SharedSelection,
    event_callback: EventCallback,
) -> Result<(), wasm_bindgen::JsValue> {
    if WEB_APP.with(|stored| stored.borrow().is_some()) {
        return Err(wasm_bindgen::JsValue::from_str(
            "GPUIX web is already running",
        ));
    }
    gpui_platform::web_init();
    let app = gpui_platform::single_threaded_web().run_embedded(move |cx| {
        crate::custom_elements::input::init(cx);
        let window = cx.open_window(Default::default(), |window, cx| {
            if let Some(mode) = PENDING_DEBUG_OVERLAY.with(|pending| pending.borrow_mut().take()) {
                window.set_debug_frame_overlay_mode(mode);
            }
            cx.new(|_| {
                GpuixView::new(
                    tree,
                    Some(event_callback),
                    "GPUIX Web".to_string(),
                    selection,
                )
            })
        });
        match window {
            Ok(window) => WEB_WINDOW.with(|stored| *stored.borrow_mut() = Some(window)),
            Err(error) => log::error!("Failed to open the GPUIX web window: {error:#}"),
        }
        cx.activate(true);
    });
    WEB_APP.with(|stored| *stored.borrow_mut() = Some(app));
    Ok(())
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn web_element_id(id: f64) -> Result<u64, wasm_bindgen::JsValue> {
    raw_element_id(id).map_err(|error| wasm_bindgen::JsValue::from_str(&error))
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn web_number_array(values: impl IntoIterator<Item = f64>) -> wasm_bindgen::JsValue {
    let result = js_sys::Array::new();
    for value in values {
        result.push(&value.into());
    }
    result.into()
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn web_string_array(values: impl IntoIterator<Item = String>) -> wasm_bindgen::JsValue {
    let result = js_sys::Array::new();
    for value in values {
        result.push(&value.into());
    }
    result.into()
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn update_web_window<R>(
    update: impl FnOnce(&mut GpuixView, &mut gpui::Window, &mut gpui::Context<GpuixView>) -> R,
) -> Result<R, wasm_bindgen::JsValue> {
    WEB_APP.with(|app| {
        let app = app.borrow();
        let app = app
            .as_ref()
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("GPUIX web is not initialized"))?;
        app.update(|cx| {
            WEB_WINDOW.with(|window| {
                let window = (*window.borrow()).ok_or_else(|| {
                    wasm_bindgen::JsValue::from_str("GPUIX web window is not ready")
                })?;
                window
                    .update(cx, update)
                    .map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))
            })
        })
    })
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn update_web_window_without_view<R>(
    update: impl FnOnce(&mut gpui::Window, &mut gpui::App) -> R,
) -> Result<R, wasm_bindgen::JsValue> {
    WEB_APP.with(|app| {
        let app = app.borrow();
        let app = app
            .as_ref()
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("GPUIX web is not initialized"))?;
        app.update(|cx| {
            WEB_WINDOW.with(|window| {
                let window = (*window.borrow()).ok_or_else(|| {
                    wasm_bindgen::JsValue::from_str("GPUIX web window is not ready")
                })?;
                gpui::AnyWindowHandle::from(window)
                    .update(cx, move |_view, window, cx| update(window, cx))
                    .map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))
            })
        })
    })
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn notify_web() {
    if let Err(error) = update_web_window(|_view, _window, cx| cx.notify()) {
        if WEB_WINDOW.with(|window| window.borrow().is_some()) {
            log::error!("Failed to invalidate the GPUIX web window: {error:?}");
        }
    }
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn web_event_callback(callback: js_sys::Function) -> EventCallback {
    Rc::new(move |payload| {
        let Ok(json) = serde_json::to_string(&payload) else {
            log::error!("Failed to serialize GPUIX browser event");
            return;
        };
        let Ok(payload) = js_sys::JSON::parse(&json) else {
            log::error!("Failed to create GPUIX browser event object");
            return;
        };
        let callback = callback.clone();
        let task = wasm_bindgen::closure::Closure::once_into_js(move || {
            if let Err(error) = callback.call2(
                &wasm_bindgen::JsValue::UNDEFINED,
                &wasm_bindgen::JsValue::NULL,
                &payload,
            ) {
                log::error!("GPUIX browser event callback failed: {error:?}");
            }
        });
        let task: js_sys::Function = task.unchecked_into();
        if let Some(window) = web_sys::window() {
            window.queue_microtask(&task);
        }
    })
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[wasm_bindgen::prelude::wasm_bindgen(js_name = GpuixRenderer)]
pub struct WebGpuixRenderer {
    tree: Arc<Mutex<RetainedTree>>,
    selection: SharedSelection,
    event_callback: EventCallback,
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[wasm_bindgen::prelude::wasm_bindgen(js_class = GpuixRenderer)]
impl WebGpuixRenderer {
    #[wasm_bindgen::prelude::wasm_bindgen(constructor)]
    pub fn new(event_callback: js_sys::Function) -> Self {
        Self {
            tree: Arc::new(Mutex::new(RetainedTree::new())),
            selection: SharedSelection::default(),
            event_callback: web_event_callback(event_callback),
        }
    }

    pub fn init(&self, _options: wasm_bindgen::JsValue) -> Result<(), wasm_bindgen::JsValue> {
        start_web_app(
            self.tree.clone(),
            self.selection.clone(),
            self.event_callback.clone(),
        )
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = applyBatch)]
    pub fn apply_batch(
        &self,
        json: String,
    ) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        let destroyed = apply_batch_to_tree(&mut self.tree.lock().unwrap(), json.as_bytes())
            .map_err(|error| wasm_bindgen::JsValue::from_str(&error))?;
        notify_web();
        Ok(web_number_array(destroyed))
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = isInitialized)]
    pub fn is_initialized(&self) -> bool {
        WEB_APP.with(|app| app.borrow().is_some())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = requiresTick)]
    pub fn requires_tick(&self) -> bool {
        false
    }

    pub fn tick(&self) {}

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getWindowSize)]
    pub fn get_window_size(&self) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        let window = web_sys::window()
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("Browser window is unavailable"))?;
        let size = js_sys::Object::new();
        js_sys::Reflect::set(&size, &"width".into(), &window.inner_width()?)?;
        js_sys::Reflect::set(&size, &"height".into(), &window.inner_height()?)?;
        Ok(size.into())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getWindowInsets)]
    pub fn get_window_insets(&self) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        let insets = update_web_window(|_view, window, _cx| window.insets())?;
        window_insets_js(insets)
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = setWindowTitle)]
    pub fn set_window_title(&self, title: String) -> Result<(), wasm_bindgen::JsValue> {
        update_web_window(move |view, _window, cx| {
            view.window_title = title;
            cx.notify();
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = focusElement)]
    pub fn focus_element(&self, element_id: f64) -> Result<(), wasm_bindgen::JsValue> {
        let id = web_element_id(element_id)?;
        update_web_window(move |view, window, cx| {
            view.reveal_virtual_list_ancestor(id);
            if let Some(handle) = view.focus_handles.get(&id) {
                handle.focus(window, cx);
            }
            cx.notify();
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = focusNext)]
    pub fn focus_next(&self) -> Result<(), wasm_bindgen::JsValue> {
        update_web_window(|_view, window, cx| window.focus_next(cx))
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = focusPrevious)]
    pub fn focus_previous(&self) -> Result<(), wasm_bindgen::JsValue> {
        update_web_window(|_view, window, cx| window.focus_prev(cx))
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = setWindowKeyEvents)]
    pub fn set_window_key_events(
        &self,
        key_down: bool,
        key_up: bool,
        event_id: f64,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let event_id = web_element_id(event_id)?;
        update_web_window(move |view, _window, cx| {
            view.window_key_down = key_down;
            view.window_key_up = key_up;
            view.window_key_event_id = event_id;
            cx.notify();
        })
    }

    pub fn blur(&self) -> Result<(), wasm_bindgen::JsValue> {
        update_web_window(|_view, window, _cx| window.blur())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getSelectedText)]
    pub fn get_selected_text(&self) -> wasm_bindgen::JsValue {
        self.selection
            .lock()
            .selected_text()
            .map_or(wasm_bindgen::JsValue::NULL, |value| {
                wasm_bindgen::JsValue::from_str(&value)
            })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = clearSelection)]
    pub fn clear_selection(&self) {
        self.selection.lock().clear();
        notify_web();
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = scrollTo)]
    pub fn scroll_to(&self, element_id: f64, x: f64, y: f64) -> Result<(), wasm_bindgen::JsValue> {
        let id = web_element_id(element_id)?;
        if !VIRTUAL_LIST_STATES.with(|states| {
            let states = states.borrow();
            let Some(state) = states.get(&id) else {
                return false;
            };
            state.set_offset_from_scrollbar(gpui::point(gpui::px(x as f32), gpui::px(y as f32)));
            true
        }) {
            SCROLL_HANDLES.with(|handles| {
                let to = gpui::point(gpui::px(x as f32), gpui::px(y as f32));
                if let Some(handle) = handles.borrow().get(&id) {
                    handle.set_offset(to);
                } else {
                    // The element has not painted yet, so it has no scroll
                    // handle. The offset waits until the frame that creates
                    // the handle.
                    scroll_motion::defer(id, to);
                }
            });
        }
        notify_web();
        Ok(())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = scrollToItem)]
    pub fn scroll_to_item(
        &self,
        element_id: f64,
        index: f64,
        offset_in_item: Option<f64>,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let id = web_element_id(element_id)?;
        let index = index as usize;
        let offset = offset_in_item.unwrap_or(0.0) as f32;
        if !VIRTUAL_LIST_STATES.with(|states| {
            if !states.borrow().contains_key(&id) {
                return false;
            }
            queue_virtual_list_scroll(id, index, offset);
            true
        }) {
            SCROLL_HANDLES.with(|handles| {
                if let Some(handle) = handles.borrow().get(&id) {
                    handle.scroll_to_item(index);
                }
            });
        }
        notify_web();
        Ok(())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getListScrollTop)]
    pub fn get_list_scroll_top(
        &self,
        element_id: f64,
    ) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        let id = web_element_id(element_id)?;
        let top = VIRTUAL_LIST_STATES.with(|states| {
            states.borrow().get(&id).map(|state| {
                let top = state.logical_scroll_top();
                [
                    top.item_ix as f64,
                    f64::from(f32::from(top.offset_in_item)),
                    f64::from(f32::from(state.viewport_bounds().size.height)),
                ]
            })
        });
        let Some(top) = top else {
            return Ok(wasm_bindgen::JsValue::NULL);
        };
        Ok(web_number_array(top))
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getScrollOffset)]
    pub fn get_scroll_offset(
        &self,
        element_id: f64,
    ) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        let id = web_element_id(element_id)?;
        let offset = VIRTUAL_LIST_STATES
            .with(|states| {
                states.borrow().get(&id).map(|state| {
                    let offset = state.scroll_px_offset_for_scrollbar();
                    [
                        f64::from(f32::from(offset.x)),
                        f64::from(f32::from(offset.y)),
                    ]
                })
            })
            .or_else(|| {
                SCROLL_HANDLES.with(|handles| {
                    handles.borrow().get(&id).map(|handle| {
                        let offset = handle.offset();
                        [
                            f64::from(f32::from(offset.x)),
                            f64::from(f32::from(offset.y)),
                        ]
                    })
                })
            });
        let Some([x, y]) = offset else {
            return Ok(wasm_bindgen::JsValue::NULL);
        };
        Ok(web_number_array([x, y]))
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getAutomationTree)]
    pub fn get_automation_tree(&self) -> Result<String, wasm_bindgen::JsValue> {
        notify_web();
        let bounds = crate::automation::all_bounds();
        let tree = self.tree.lock().unwrap();
        serde_json::to_string(&tree.to_automation_json(&bounds)).map_err(|error| {
            wasm_bindgen::JsValue::from_str(&format!("JSON serialization failed: {error}"))
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getElementBounds)]
    pub fn get_element_bounds(
        &self,
        element_id: f64,
    ) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        let Some(bounds) = crate::automation::get_bounds(web_element_id(element_id)?) else {
            return Ok(wasm_bindgen::JsValue::NULL);
        };
        Ok(web_number_array([
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
        ]))
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getAllText)]
    pub fn get_all_text(&self) -> wasm_bindgen::JsValue {
        let tree = self.tree.lock().unwrap();
        let mut texts = Vec::new();
        if let Some(root_id) = tree.root_id {
            collect_text(root_id, &tree, &mut texts);
        }
        web_string_array(texts)
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getPaintedText)]
    pub fn get_painted_text(&self) -> wasm_bindgen::JsValue {
        web_string_array(crate::text::painted_text())
    }

    /// The same array of objects the napi build returns.
    ///
    /// Through `serde_json` and `JSON.parse`, not `serde-wasm-bindgen`: this is
    /// a test-only API, and both crates here are already dependencies. Building
    /// the nested value by hand with `js_sys` is 20 lines of noise.
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getPaintedHighlights)]
    pub fn get_painted_highlights(&self) -> wasm_bindgen::JsValue {
        let matches: Vec<crate::element_tree::HighlightMatch> = crate::text::painted_highlights()
            .into_iter()
            .map(Into::into)
            .collect();
        serde_json::to_string(&matches)
            .ok()
            .and_then(|json| js_sys::JSON::parse(&json).ok())
            .unwrap_or(wasm_bindgen::JsValue::NULL)
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = simulateClick)]
    pub fn simulate_click(
        &self,
        x: f64,
        y: f64,
        button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());
        update_web_window_without_view(move |window, cx| {
            crate::automation::dispatch_click(window, cx, x, y, button.unwrap_or(0), modifiers);
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = simulateMouseDown)]
    pub fn simulate_mouse_down(
        &self,
        x: f64,
        y: f64,
        button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());
        update_web_window_without_view(move |window, cx| {
            crate::automation::dispatch_mouse_down(
                window,
                cx,
                x,
                y,
                button.unwrap_or(0),
                modifiers,
            );
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = simulateMouseUp)]
    pub fn simulate_mouse_up(
        &self,
        x: f64,
        y: f64,
        button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());
        update_web_window_without_view(move |window, cx| {
            crate::automation::dispatch_mouse_up(window, cx, x, y, button.unwrap_or(0), modifiers);
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = simulateMouseMove)]
    pub fn simulate_mouse_move(
        &self,
        x: f64,
        y: f64,
        pressed_button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());
        update_web_window_without_view(move |window, cx| {
            crate::automation::dispatch_mouse_move(window, cx, x, y, pressed_button, modifiers);
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = simulateScrollWheel)]
    pub fn simulate_scroll_wheel(
        &self,
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
        modifiers: Option<String>,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());
        update_web_window_without_view(move |window, cx| {
            crate::automation::dispatch_scroll_wheel(window, cx, x, y, delta_x, delta_y, modifiers);
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = clockPause)]
    pub fn clock_pause(&self) -> Result<f64, wasm_bindgen::JsValue> {
        update_web_window(|view, _window, cx| {
            let now_ms = view.clock.pause();
            cx.notify();
            now_ms
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = clockSet)]
    pub fn clock_set(&self, now_ms: f64) -> Result<f64, wasm_bindgen::JsValue> {
        update_web_window(move |view, _window, cx| {
            let now_ms = view.clock.set_ms(now_ms);
            cx.notify();
            now_ms
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = clockFastForward)]
    pub fn clock_fast_forward(&self, delta_ms: f64) -> Result<f64, wasm_bindgen::JsValue> {
        update_web_window(move |view, _window, cx| {
            let now_ms = view.clock.fast_forward_ms(delta_ms);
            cx.notify();
            now_ms
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = clockResume)]
    pub fn clock_resume(&self) -> Result<f64, wasm_bindgen::JsValue> {
        update_web_window(|view, _window, cx| {
            let now_ms = view.clock.resume();
            cx.notify();
            now_ms
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = setDebugFrameOverlay)]
    pub fn set_debug_frame_overlay(&self, mode: String) -> Result<String, wasm_bindgen::JsValue> {
        let mode = parse_debug_frame_overlay_mode_str(&mode)
            .map_err(|error| wasm_bindgen::JsValue::from_str(&error))?;
        // Graphics init is async. render() sets the overlay before WEB_WINDOW exists.
        if WEB_WINDOW.with(|window| window.borrow().is_none()) {
            PENDING_DEBUG_OVERLAY.with(|pending| *pending.borrow_mut() = Some(mode));
            return Ok(debug_frame_overlay_mode_name(mode).to_string());
        }
        update_web_window(move |_view, window, cx| {
            window.set_debug_frame_overlay_mode(mode);
            cx.notify();
            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).to_string()
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getDebugFrameOverlay)]
    pub fn get_debug_frame_overlay(&self) -> Result<String, wasm_bindgen::JsValue> {
        if WEB_WINDOW.with(|window| window.borrow().is_none()) {
            let pending = PENDING_DEBUG_OVERLAY.with(|pending| *pending.borrow());
            return Ok(debug_frame_overlay_mode_name(
                pending.unwrap_or(gpui::DebugFrameOverlayMode::Hidden),
            )
            .to_string());
        }
        update_web_window(|_view, window, _cx| {
            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).to_string()
        })
    }
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
impl Drop for GpuixRenderer {
    fn drop(&mut self) {
        self.ui_commands.lock().unwrap().take();
    }
}

// ── GPUI View ────────────────────────────────────────────────────────

pub(crate) struct GpuixView {
    pub(crate) tree: Arc<Mutex<RetainedTree>>,
    pub(crate) event_callback: Option<EventCallback>,
    pub(crate) window_title: String,
    pub(crate) window_key_down: bool,
    pub(crate) window_key_up: bool,
    pub(crate) window_key_event_id: u64,
    /// Persistent FocusHandles keyed by element ID.
    /// Created lazily for elements with keyboard or focus/blur listeners.
    /// Handles persist across renders so GPUI maintains focus state.
    pub(crate) focus_handles: HashMap<u64, gpui::FocusHandle>,
    /// Active focus/blur subscriptions keyed by element and event type.
    pub(crate) focus_subscriptions: HashMap<(u64, String), gpui::Subscription>,
    /// Registry for custom element types (input, editor, diff, etc.).
    /// Stores factories (one per type) and live instances (one per element ID).
    pub(crate) custom_registry: CustomElementRegistry,
    /// Persistent ScrollHandles keyed by element ID.
    /// Created lazily for elements with overflow: "scroll" (or per-axis scroll).
    /// Handles persist across renders so GPUI maintains scroll offset state.
    pub(crate) scroll_handles: HashMap<u64, gpui::ScrollHandle>,
    /// Native animation clocks keyed by retained element ID.
    pub(crate) motion_states: HashMap<u64, crate::motion::MotionState>,
    /// What each scroll box's scrollbar remembers between frames.
    pub(crate) scrollbars: scrollbar::States,
    /// Live text selection, shared with the paint closures and the napi methods.
    pub(crate) selection: SharedSelection,
    /// Persistent measurement and scroll state for React-backed virtual lists.
    virtual_lists: HashMap<u64, VirtualListEntry>,
    /// Latest pointer sample and list during selection edge scrolling.
    selection_drag_position: Option<gpui::Point<gpui::Pixels>>,
    selection_scroll_list: Option<u64>,
    selection_scroll_task: Option<gpui::Task<()>>,
    /// Motion / review clock. Live wall time unless automation freezes it.
    pub(crate) clock: crate::automation::AutomationClock,
    /// The cascade every frame starts from.
    ///
    /// It has to keep its identity between frames. The resolved-style cache
    /// asks whether the cascade an element resolved under is still the current
    /// one, and it compares pointers. A fresh root on every frame would answer
    /// no for every element that reads an inherited value, and the whole tree
    /// would resolve again on every frame.
    root_cascade: RefCell<Option<(Theme, gpui::Pixels, crate::inheritance::Inherited)>>,
    /// Resolved `highlight` state, keyed by the element that declared it.
    /// Empty in every app that does not use search.
    highlights: HashMap<u64, HighlightCacheEntry>,
    /// The running view transition, when one runs.
    view_transition: Option<view_transition::VtState>,
    /// Captures `viewTransitionCapture` took, waiting for the start call.
    pending_view_transition: Option<HashMap<String, view_transition::VtCapture>>,
}

/// Two-level cache for one element's `highlight`.
///
/// The group list is keyed by `search_revision`, which a query change does NOT
/// move, so typing in a find bar never re-walks or re-folds text. The matches
/// are additionally keyed by the matcher hash, which excludes `activeIndex` and
/// the colours, so moving the find cursor only re-colours what it already found.
///
/// Do not key the group list on `subtree_revision`: `highlight` is a custom
/// prop, so every keystroke moves that revision and the cache would do nothing.
/// `highlight_cache_tests` at the bottom of this file compares `Arc` identity
/// and fails if either level regresses. A timing budget does not catch it: on
/// the 1000-turn chat the broken version is 2.7ms against 1.9ms.
struct HighlightCacheEntry {
    revision: u64,
    groups: Arc<crate::text::GroupList>,
    matcher_hash: u64,
    /// The spec plus the located matches. Ordinals and colours are decided at
    /// paint, so a colour or `activeIndex` change reuses this whole value.
    context: Arc<crate::text::HighlightContext>,
    /// Last identity delivered through `onHighlight`. Only written once an
    /// event is really queued, so adding the listener later still reports.
    reported: Option<u64>,
}

fn emit_highlight_events(callback: &Option<EventCallback>, events: &[(u64, usize)]) {
    for &(id, total) in events {
        emit_event_full(callback, id, "highlight", |payload| {
            payload.match_count = Some(total as f64);
        });
    }
}

fn window_key_events(
    callback: Option<EventCallback>,
    key_down: bool,
    key_up: bool,
    event_id: u64,
) -> impl gpui::IntoElement {
    use gpui::prelude::*;

    gpui::canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            if key_down || cfg!(all(target_arch = "wasm32", target_os = "unknown")) {
                let callback = callback.clone();
                window.on_root_key_event(move |event: &gpui::KeyDownEvent, phase, _window, _cx| {
                    if phase != gpui::DispatchPhase::Bubble {
                        return;
                    }
                    if key_down {
                        emit_event_full(&callback, event_id, "windowKeyDown", |payload| {
                            payload.key = Some(event.keystroke.key.clone());
                            payload.key_char = event.keystroke.key_char.clone();
                            payload.is_held = Some(event.is_held);
                            payload.modifiers = Some(event.keystroke.modifiers.into());
                        });
                    }
                    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
                    if event.keystroke.key == "tab" {
                        // Keep browser focus on GPUI's hidden keyboard element.
                        _cx.stop_propagation();
                    }
                });
            }
            if key_up {
                let callback = callback.clone();
                window.on_root_key_event(move |event: &gpui::KeyUpEvent, phase, _window, _cx| {
                    if phase != gpui::DispatchPhase::Bubble {
                        return;
                    }
                    emit_event_full(&callback, event_id, "windowKeyUp", |payload| {
                        payload.key = Some(event.keystroke.key.clone());
                        payload.key_char = event.keystroke.key_char.clone();
                        payload.modifiers = Some(event.keystroke.modifiers.into());
                    });
                });
            }
        },
    )
    .absolute()
    .w(gpui::px(0.0))
    .h(gpui::px(0.0))
}

/// Resolve one element's `highlight` prop, reusing both cache levels.
///
/// Returns the context, plus the match count when `has_listener` and the result
/// differs from the last one this element reported. Identity, not count:
/// swapping a query for a different one with the same number of hits is still a
/// new result.
fn resolve_highlight(
    cache: &mut HashMap<u64, HighlightCacheEntry>,
    tree: &RetainedTree,
    id: u64,
    value: &serde_json::Value,
    theme: &Theme,
    has_listener: bool,
) -> Option<(Arc<crate::text::HighlightContext>, Option<usize>)> {
    let set = crate::text::HighlightSet::parse(value, theme)?;
    // `search_revision`, NOT `subtree_revision`: `highlight` is a custom prop,
    // so the general revision moves on every keystroke and this cache would
    // never hit for the one case it exists for.
    let revision = tree.elements.get(&id)?.search_revision;
    let matcher_hash = set.matcher_hash();

    let cached = cache
        .get(&id)
        .filter(|entry| entry.revision == revision && entry.matcher_hash == matcher_hash);
    let context = match cached {
        // Nothing moved at all. Returning the same `Arc` keeps the whole
        // subtree's inherited value identical, which the cache tests assert.
        Some(entry) if entry.context.set == set => entry.context.clone(),
        // Same matches, different colours or find cursor: reuse the located
        // matches and swap only the spec. No text is scanned.
        Some(entry) => {
            let context = Arc::new(crate::text::HighlightContext {
                declaration: id,
                set,
                matches: entry.context.matches.clone(),
            });
            cache.get_mut(&id)?.context = context.clone();
            context
        }
        None => {
            let groups = match cache.get(&id) {
                Some(entry) if entry.revision == revision => entry.groups.clone(),
                _ => Arc::new(crate::text::GroupList::collect(tree, id)),
            };
            let context = Arc::new(crate::text::HighlightContext {
                declaration: id,
                matches: Arc::new(crate::text::search::resolve(&groups, &set)),
                set,
            });
            let reported = cache.get(&id).and_then(|entry| entry.reported);
            cache.insert(
                id,
                HighlightCacheEntry {
                    revision,
                    groups,
                    matcher_hash,
                    context: context.clone(),
                    reported,
                },
            );
            context
        }
    };

    if !has_listener {
        return Some((context, None));
    }
    let identity = context.matches.identity();
    let entry = cache.get_mut(&id)?;
    if entry.reported == Some(identity) {
        return Some((context, None));
    }
    entry.reported = Some(identity);
    let total = context.matches.total;
    Some((context, Some(total)))
}

impl GpuixView {
    pub(crate) fn new(
        tree: Arc<Mutex<RetainedTree>>,
        event_callback: Option<EventCallback>,
        window_title: String,
        selection: SharedSelection,
    ) -> Self {
        Self {
            tree,
            event_callback,
            window_title,
            window_key_down: false,
            window_key_up: false,
            window_key_event_id: 0,
            focus_handles: HashMap::new(),
            focus_subscriptions: HashMap::new(),
            custom_registry: CustomElementRegistry::with_defaults(),
            scroll_handles: HashMap::new(),
            motion_states: HashMap::new(),
            scrollbars: HashMap::new(),
            selection,
            virtual_lists: HashMap::new(),
            selection_drag_position: None,
            selection_scroll_list: None,
            selection_scroll_task: None,
            clock: crate::automation::AutomationClock::new(),
            root_cascade: RefCell::new(None),
            highlights: HashMap::new(),
            view_transition: None,
            pending_view_transition: None,
        }
    }

    /// Clone every named element and its painted bounds, before the tree
    /// swaps. `view_transition_start` consumes the result.
    pub(crate) fn view_transition_capture(&mut self) {
        let tree = self.tree.lock().unwrap();
        self.pending_view_transition = Some(view_transition::capture(&tree));
    }

    /// Start the transition against the tree as it is now. A start without a
    /// capture still animates the names the new tree carries.
    pub(crate) fn view_transition_start(
        &mut self,
        options_json: &str,
    ) -> std::result::Result<(), String> {
        let options = view_transition::VtOptions::parse(options_json)?;
        let captures = self.pending_view_transition.take().unwrap_or_default();
        let tree = self.tree.lock().unwrap();
        let state = view_transition::VtState::new(captures, options, &tree);
        drop(tree);
        self.view_transition = Some(state);
        Ok(())
    }

    /// Set one scroll offset, in one step or as a glide when the box asks
    /// for `scroll-behavior: smooth` or the caller asks for `smooth`.
    /// Only the UI-command thread calls this, so macOS builds without it.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub(crate) fn scroll_to_offset(
        &self,
        id: u64,
        x: f32,
        y: f32,
        behavior: scroll_motion::Behavior,
    ) {
        let smooth = {
            let tree = self.tree.lock().unwrap();
            behavior.smooth(tree.elements.get(&id).and_then(|el| el.style.as_deref()))
        };
        SCROLL_HANDLES.with(|cell| {
            let to = gpui::point(gpui::px(x), gpui::px(y));
            if let Some(handle) = cell.borrow().get(&id) {
                if smooth {
                    scroll_motion::animate(id, handle, to);
                } else {
                    handle.set_offset(to);
                }
            } else {
                // The element has not painted yet, so it has no scroll
                // handle. The offset waits until the frame that creates
                // the handle, and applies in one step there.
                scroll_motion::defer(id, to);
            }
        });
    }

    /// The root cascade for `theme`, reusing the last one while the theme
    /// holds still.
    fn root_cascade(&self, theme: &Theme, rem_size: gpui::Pixels) -> crate::inheritance::Inherited {
        let mut slot = self.root_cascade.borrow_mut();
        if let Some((built_for, built_rem, cascade)) = slot.as_ref() {
            if built_for == theme && *built_rem == rem_size {
                return cascade.clone();
            }
        }
        // Inheritance takes plain values, so the theme is read here rather
        // than inside it.
        let cascade = crate::inheritance::Inherited::root(
            crate::color::from_gpui(theme.accent),
            theme.dark,
            f32::from(rem_size),
        );
        *slot = Some((theme.clone(), rem_size, cascade.clone()));
        cascade
    }

    fn build_virtual_child(
        &mut self,
        list_id: u64,
        index: usize,
        expected_child_id: u64,
        cascade: crate::inheritance::Inherited,
        highlight: Option<std::sync::Arc<crate::text::HighlightContext>>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        let row_focus_handle = self.virtual_lists.get_mut(&list_id).and_then(|entry| {
            entry.seen_rows.insert(expected_child_id);
            (entry.child_at(index) == Some(expected_child_id))
                .then(|| {
                    index
                        .checked_sub(entry.window_start)
                        .and_then(|offset| entry.row_focus_handles.get(offset).cloned())
                })
                .flatten()
                .flatten()
        });

        let tree_arc = self.tree.clone();
        let tree = tree_arc.lock().unwrap();
        let window_start = self
            .virtual_lists
            .get(&list_id)
            .map(|entry| entry.window_start)
            .unwrap_or(0);
        let child_matches = tree.elements.get(&list_id).and_then(|list| {
            index
                .checked_sub(window_start)
                .and_then(|offset| list.children.get(offset))
        }) == Some(&expected_child_id);
        if !child_matches {
            let height = self
                .virtual_lists
                .get(&list_id)
                .and_then(|entry| entry.config.estimated_item_height)
                .unwrap_or(1.0);
            return unmounted_virtual_row(height);
        }

        let callback = self.event_callback.clone();
        let now = self.clock.now();
        let mut motion_active = false;
        let mut highlight_events = Vec::new();

        // Re-resolve against the tree as it is NOW. gpui calls this during
        // layout and prepaint, after the root render returned, and on Windows
        // and Linux the Node thread can commit new text in between. Reusing the
        // captured ranges would paint a wash over the wrong glyphs, or at a byte
        // offset that is no longer a character boundary.
        let mut highlight = highlight;
        if let Some(declaration) = highlight.as_ref().map(|ctx| ctx.declaration) {
            highlight = tree
                .elements
                .get(&declaration)
                .and_then(|element| element.custom_props.get("highlight"))
                .and_then(|value| {
                    resolve_highlight(
                        &mut self.highlights,
                        &tree,
                        declaration,
                        value,
                        &Theme::dark(),
                        false,
                    )
                })
                .map(|(context, _)| context);
        }

        let mut build_ctx = BuildCtx {
            tree: &tree,
            event_callback: &callback,
            focus_handles: &self.focus_handles,
            scroll_handles: &mut self.scroll_handles,
            custom_registry: &mut self.custom_registry,
            virtual_lists: &mut self.virtual_lists,
            motion_states: &mut self.motion_states,
            scrollbars: &mut self.scrollbars,
            now,
            motion_active: &mut motion_active,
            selection: self.selection.clone(),
            cascade,
            highlight,
            highlights: &mut self.highlights,
            highlight_events: &mut highlight_events,
            vt: self.view_transition.as_ref(),
            frozen: false,
            direct_rules: Vec::new(),
            descendant_rules: Vec::new(),
        };
        // A virtual row builds outside the tree walk, so it has no child
        // position and the index states do not apply to it.
        let child = build_element(expected_child_id, None, &mut build_ctx, window, cx);
        emit_highlight_events(&callback, &highlight_events);
        if motion_active {
            window.request_animation_frame();
        }
        let Some(focus_handle) = row_focus_handle else {
            return child;
        };
        gpui::div()
            .id(gpui::SharedString::from(format!(
                "__gpuix_virtual_row_{}_{}",
                list_id, expected_child_id
            )))
            .w_full()
            .track_focus(&focus_handle)
            .child(child)
            .into_any_element()
    }

    pub(crate) fn scroll_virtual_list_to_item(
        &self,
        id: u64,
        index: usize,
        offset_in_item: f32,
    ) -> bool {
        if !self.virtual_lists.contains_key(&id) {
            return false;
        }
        queue_virtual_list_scroll(id, index, offset_in_item);
        emit_event_full(&self.event_callback, id, "visibleRange", |payload| {
            payload.start_index = Some(index as f64);
            payload.end_index = Some((index + 1) as f64);
        });
        true
    }

    /// The list's logical scroll anchor as
    /// `[item_ix, offset_in_item_px, viewport_height_px]`.
    ///
    /// `item_ix == item count` is gpui's at-end sentinel (a bottom-aligned
    /// list resting at its very end); the viewport height is what lets a
    /// caller convert that into a position relative to the trailing rows.
    pub(crate) fn virtual_list_scroll_top(&self, id: u64) -> Option<[f64; 3]> {
        let state = &self.virtual_lists.get(&id)?.state;
        let top = state.logical_scroll_top();
        Some([
            top.item_ix as f64,
            f64::from(f32::from(top.offset_in_item)),
            f64::from(f32::from(state.viewport_bounds().size.height)),
        ])
    }

    pub(crate) fn set_virtual_list_offset(&self, id: u64, x: f32, y: f32) -> bool {
        let Some(entry) = self.virtual_lists.get(&id) else {
            return false;
        };
        entry
            .state
            .set_offset_from_scrollbar(gpui::point(gpui::px(x), gpui::px(y)));
        true
    }

    pub(crate) fn virtual_list_offset(&self, id: u64) -> Option<[f64; 2]> {
        let offset = self
            .virtual_lists
            .get(&id)?
            .state
            .scroll_px_offset_for_scrollbar();
        Some(offset_to_js(offset))
    }

    pub(crate) fn reveal_virtual_list_ancestor(&self, id: u64) -> bool {
        let tree_arc = self.tree.clone();
        let tree = tree_arc.lock().unwrap();
        let mut current = id;
        let location = loop {
            let Some(parent_id) = tree
                .elements
                .get(&current)
                .and_then(|element| element.parent)
            else {
                break None;
            };
            if self.virtual_lists.contains_key(&parent_id) {
                let index = tree
                    .elements
                    .get(&parent_id)
                    .and_then(|parent| parent.children.iter().position(|child| *child == current));
                break index.map(|index| (parent_id, index));
            }
            current = parent_id;
        };
        drop(tree);

        let Some((list_id, index)) = location else {
            return false;
        };
        self.scroll_virtual_list_to_item(list_id, index, 0.0)
    }
}

impl GpuixView {
    fn on_selection_mouse_move(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.selection.lock().is_dragging() {
            self.stop_selection_scroll();
            return;
        }
        let list_id = self
            .selection_scroll_list
            .filter(|id| {
                self.virtual_lists.get(id).is_some_and(|entry| {
                    let bounds = entry.state.viewport_bounds();
                    position.x >= bounds.left() && position.x <= bounds.right()
                })
            })
            .or_else(|| {
                self.virtual_lists
                    .iter()
                    .find(|(_, entry)| entry.state.viewport_bounds().contains(&position))
                    .map(|(id, _)| *id)
            });
        let Some(list_id) = list_id else {
            self.stop_selection_scroll();
            return;
        };
        self.selection_drag_position = Some(position);
        self.selection_scroll_list = Some(list_id);
        self.schedule_selection_scroll(cx);
    }

    fn stop_selection_scroll(&mut self) {
        self.selection_drag_position = None;
        self.selection_scroll_list = None;
        self.selection_scroll_task = None;
    }

    fn schedule_selection_scroll(&mut self, cx: &mut gpui::Context<Self>) {
        if self.selection_scroll_task.is_some() || !self.selection.lock().is_dragging() {
            return;
        }
        let (Some(position), Some(list_id)) =
            (self.selection_drag_position, self.selection_scroll_list)
        else {
            return;
        };
        let Some(entry) = self.virtual_lists.get(&list_id) else {
            return;
        };
        if selection_scroll_step(entry.state.viewport_bounds(), position) == 0.0 {
            return;
        }
        self.selection_scroll_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(SELECTION_SCROLL_TICK_MS))
                .await;
            if let Err(error) = view.update(cx, |view, cx| {
                view.selection_scroll_task = None;
                view.step_selection_scroll(cx);
            }) {
                log::debug!("selection scroll stopped after view teardown: {error}");
            }
        }));
    }

    fn step_selection_scroll(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.selection.lock().is_dragging() {
            self.stop_selection_scroll();
            return;
        }
        let (Some(position), Some(list_id)) =
            (self.selection_drag_position, self.selection_scroll_list)
        else {
            return;
        };
        let Some(entry) = self.virtual_lists.get(&list_id) else {
            self.stop_selection_scroll();
            return;
        };
        let step = selection_scroll_step(entry.state.viewport_bounds(), position);
        if step == 0.0 {
            return;
        }

        let before = entry.state.logical_scroll_top();
        let selection_moved = crate::text::paint::update_drag_at(&self.selection, position);
        entry.state.scroll_by(gpui::px(step));
        let after = entry.state.logical_scroll_top();
        let list_moved =
            after.item_ix != before.item_ix || after.offset_in_item != before.offset_in_item;
        if !selection_moved && !list_moved {
            self.stop_selection_scroll();
            return;
        }
        cx.notify();
        self.schedule_selection_scroll(cx);
    }

    /// Sync focus handles with the current element tree.
    /// Creates handles for new focusable elements, subscribes on_focus/on_blur,
    /// and cleans up handles for destroyed elements.
    fn sync_focus_handles(
        &mut self,
        tree: &RetainedTree,
        callback: &Option<EventCallback>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let tab_index = |element: &crate::retained_tree::RetainedElement| {
            element
                .custom_props
                .get("tabIndex")
                .and_then(|value| value.as_i64())
                .and_then(|index| isize::try_from(index).ok())
        };
        let needs_focus = |element: &crate::retained_tree::RetainedElement| {
            matches!(element.element_type.as_str(), "input" | "textarea")
                || tab_index(element).is_some()
                || element.events.contains("keyDown")
                || element.events.contains("keyUp")
                || element.events.contains("focus")
                || element.events.contains("blur")
        };
        // Create handles for elements that need focus but don't have one yet.
        for (&id, element) in &tree.elements {
            let tab_index = tab_index(element).or_else(|| {
                matches!(element.element_type.as_str(), "input" | "textarea").then_some(0)
            });

            if needs_focus(element) && !self.focus_handles.contains_key(&id) {
                let handle = match tab_index {
                    Some(index) => cx.focus_handle().tab_index(index).tab_stop(index >= 0),
                    None => cx.focus_handle(),
                };
                // Focus once, at creation. Re-focusing every frame would
                // steal focus back from whatever the user clicked next.
                if element.auto_focus {
                    handle.focus(window, cx);
                }
                self.focus_handles.insert(id, handle);
            } else if let (Some(handle), Some(index)) =
                (self.focus_handles.get(&id).cloned(), tab_index)
            {
                self.focus_handles
                    .insert(id, handle.tab_index(index).tab_stop(index >= 0));
            } else if let Some(handle) = self.focus_handles.get(&id).cloned() {
                self.focus_handles.insert(id, handle.tab_stop(false));
            }
        }

        self.focus_subscriptions.retain(|(id, event), _| {
            tree.elements
                .get(id)
                .is_some_and(|element| element.events.contains(event))
        });
        for (&id, element) in &tree.elements {
            let Some(handle) = self.focus_handles.get(&id).cloned() else {
                continue;
            };
            let focus_key = (id, "focus".to_string());
            if element.events.contains("focus")
                && !self.focus_subscriptions.contains_key(&focus_key)
            {
                let callback = callback.clone();
                let subscription = cx.on_focus(&handle, window, move |_this, _window, _cx| {
                    emit_event_full(&callback, id, "focus", |_| {});
                });
                self.focus_subscriptions.insert(focus_key, subscription);
            }
            let blur_key = (id, "blur".to_string());
            if element.events.contains("blur") && !self.focus_subscriptions.contains_key(&blur_key)
            {
                let callback = callback.clone();
                let subscription = cx.on_blur(&handle, window, move |_this, _window, _cx| {
                    emit_event_full(&callback, id, "blur", |_| {});
                });
                self.focus_subscriptions.insert(blur_key, subscription);
            }
        }

        // Clean up handles for elements that no longer exist.
        self.focus_handles
            .retain(|id, _| tree.elements.get(id).is_some_and(&needs_focus));
    }
}

impl gpui::Render for GpuixView {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::IntoElement;

        window.set_window_title(&self.window_title);

        // Clone Arc so we don't borrow self.tree — frees self for focus_handles access.
        let tree_arc = self.tree.clone();
        let tree = tree_arc.lock().unwrap();
        let callback = self.event_callback.clone();

        // Sync focus handles before building elements.
        self.sync_focus_handles(&tree, &callback, window, cx);

        // One frame of the running view transition, taken out of `self` so
        // the build can borrow the rest of the view. A transition past its
        // end drops here, and the frame paints the live tree alone.
        let now = self.clock.now();
        let view_transition = self.view_transition.take().and_then(|mut transition| {
            transition.tick(now).then_some(transition)
        });
        // A frame can build between the capture and the start call, after
        // the swap already removed the old elements from the tree. The ids
        // of the pending captures count as kept too, or that frame drops
        // their scroll handles, and the frozen copy of a scrolled screen
        // then paints from a fresh handle at offset zero.
        let pending_ids: std::collections::HashSet<u64> = self
            .pending_view_transition
            .as_ref()
            .map(|captures| {
                captures
                    .values()
                    .flat_map(|capture| capture.tree.elements.keys().copied())
                    .collect()
            })
            .unwrap_or_default();
        let kept_by_transition = |id: &u64| {
            pending_ids.contains(id)
                || view_transition
                    .as_ref()
                    .is_some_and(|transition| transition.keeps(*id))
        };

        // Ensure custom element instances are destroyed when their IDs disappear.
        // A frozen view-transition copy keeps its instances until it fades.
        self.custom_registry
            .prune_missing(|id| tree.elements.contains_key(&id) || kept_by_transition(&id));

        // Clean up scroll handles for destroyed elements (IDs removed from tree).
        // Scrollability-based cleanup (element still exists but style changed
        // from scroll to non-scroll) is handled inside build_host_container().
        self.scroll_handles
            .retain(|id, _| tree.elements.contains_key(id) || kept_by_transition(id));
        self.virtual_lists
            .retain(|id, _| tree.elements.contains_key(id) || kept_by_transition(id));
        self.motion_states
            .retain(|id, _| tree.elements.contains_key(id) || kept_by_transition(id));

        // Build the element tree. custom_registry, focus_handles, and scroll_handles
        // are different fields of self, so Rust allows borrowing all simultaneously.
        let theme = Theme::dark();
        let root_cascade = self.root_cascade(&theme, window.rem_size());
        let mut motion_active = false;
        // Smooth scrolls, snap containers and initial targets step once per
        // frame, against the bounds the last frame painted.
        if scroll_motion::frame(&tree, &self.scroll_handles, now) {
            motion_active = true;
        }
        // Pruned by DECLARATION, not existence: an element that drops its
        // `highlight` prop keeps living, and its cached group list holds a copy
        // of every string in its subtree.
        self.highlights.retain(|id, _| {
            tree.elements
                .get(id)
                .is_some_and(|element| element.custom_props.contains_key("highlight"))
        });
        let mut highlight_events = Vec::new();
        let (result, exit_copies) = match tree.root_id {
            Some(root_id) => {
                let mut ctx = BuildCtx {
                    tree: &tree,
                    event_callback: &callback,
                    focus_handles: &self.focus_handles,
                    scroll_handles: &mut self.scroll_handles,
                    custom_registry: &mut self.custom_registry,
                    virtual_lists: &mut self.virtual_lists,
                    motion_states: &mut self.motion_states,
                    scrollbars: &mut self.scrollbars,
                    now,
                    motion_active: &mut motion_active,
                    selection: self.selection.clone(),
                    cascade: root_cascade,
                    highlight: None,
                    highlights: &mut self.highlights,
                    highlight_events: &mut highlight_events,
                    vt: view_transition.as_ref(),
                    frozen: false,
                    direct_rules: Vec::new(),
                    descendant_rules: Vec::new(),
                };
                let root = build_element(root_id, None, &mut ctx, window, cx);
                // Names captured without a successor paint as frozen copies
                // over the tree. Built after the root, so they paint last.
                let copies = view_transition::exit_copies(&mut ctx, window, cx);
                (root, copies)
            }
            None => (gpui::Empty.into_any_element(), Vec::new()),
        };
        // A transition animates every frame until it comes to rest.
        if view_transition.is_some() {
            motion_active = true;
        }
        self.view_transition = view_transition;
        // Flushed after the root build so a `setState` in the handler cannot
        // re-enter this build.
        emit_highlight_events(&callback, &highlight_events);

        // The frame reset must paint BEFORE any text, so it is the first child of
        // the root wrapper. Without it the selection registry accumulates stale
        // entries across frames and a drag resolves against elements that are no
        // longer on screen.
        let result = {
            use gpui::prelude::*;
            let drag_move_view = cx.weak_entity();
            let drag_end_view = drag_move_view.clone();
            let root = gpui::div().size_full();
            with_window_menu_actions(root)
                .when(
                    self.window_key_down
                        || self.window_key_up
                        || cfg!(all(target_arch = "wasm32", target_os = "unknown")),
                    |root| {
                        root.child(window_key_events(
                            callback.clone(),
                            self.window_key_down,
                            self.window_key_up,
                            self.window_key_event_id,
                        ))
                    },
                )
                .child(selection_frame_reset(
                    self.selection.clone(),
                    move |position, app| {
                        drag_move_view
                            .update(app, |view, cx| view.on_selection_mouse_move(position, cx))
                            .ok();
                    },
                    move |app| {
                        drag_end_view
                            .update(app, |view, _cx| view.stop_selection_scroll())
                            .ok();
                    },
                ))
                .child(crate::automation::bounds_frame_reset())
                .child(result)
                .children(exit_copies)
                .into_any_element()
        };

        // Sync scroll handles to thread_local so napi methods (scrollTo,
        // getScrollOffset) can access them without an App context.
        SCROLL_HANDLES.with(|cell| {
            let mut handles = cell.borrow_mut();
            handles.clear();
            for (&id, handle) in &self.scroll_handles {
                handles.insert(id, handle.clone());
            }
        });
        VIRTUAL_LIST_STATES.with(|cell| {
            let mut states = cell.borrow_mut();
            states.clear();
            for (&id, entry) in &self.virtual_lists {
                states.insert(id, entry.state.clone());
            }
        });
        // One-shot: a queued scroll for a list that did not build this frame
        // would otherwise fire on some later frame, against child indices that
        // no longer match what JS meant.
        PENDING_VIRTUAL_LIST_SCROLLS.with(|cell| cell.borrow_mut().clear());

        if motion_active {
            window.request_animation_frame();
        }

        result
    }
}

// ── Event emission ───────────────────────────────────────────────────

/// Helper to convert a GPUI Point<Pixels> to (f64, f64).
pub(crate) fn point_to_xy(p: gpui::Point<gpui::Pixels>) -> (f64, f64) {
    (f64::from(f32::from(p.x)), f64::from(f32::from(p.y)))
}

/// Convert GPUI MouseButton to our u32 encoding: 0=left, 1=middle, 2=right.
pub(crate) fn mouse_button_to_u32(button: gpui::MouseButton) -> u32 {
    match button {
        gpui::MouseButton::Left => 0,
        gpui::MouseButton::Middle => 1,
        gpui::MouseButton::Right => 2,
        gpui::MouseButton::Navigate(_) => 3,
    }
}

/// General-purpose event emitter. Builds a default EventPayload, lets the
/// caller customize it via a closure, then sends it through the callback.
/// Production: queues on Node.js event loop via ThreadsafeFunction.
/// Tests: pushes to a synchronous Vec for drainEvents().
pub(crate) fn emit_event_full(
    callback: &Option<EventCallback>,
    element_id: u64,
    event_type: &str,
    build: impl FnOnce(&mut EventPayload),
) {
    if let Some(cb) = callback {
        let mut payload = EventPayload {
            element_id: element_id as f64,
            event_type: event_type.to_string(),
            ..Default::default()
        };
        build(&mut payload);
        cb(payload);
    }
}

// ── Types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
#[cfg_attr(not(all(target_arch = "wasm32", target_os = "unknown")), napi(object))]
pub struct WindowSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(not(all(target_arch = "wasm32", target_os = "unknown")), napi(object))]
pub struct EdgeInsets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(not(all(target_arch = "wasm32", target_os = "unknown")), napi(object))]
pub struct WindowInsets {
    pub safe_area: EdgeInsets,
    pub ime: EdgeInsets,
    pub effective: EdgeInsets,
}

impl WindowInsets {
    fn from_gpui(insets: gpui::WindowInsets) -> Self {
        let effective = insets.effective();
        Self {
            safe_area: EdgeInsets::from_gpui(insets.safe_area),
            ime: EdgeInsets::from_gpui(insets.ime),
            effective: EdgeInsets::from_gpui(effective),
        }
    }
}

impl EdgeInsets {
    fn from_gpui(insets: gpui::Edges<gpui::Pixels>) -> Self {
        Self {
            top: f32::from(insets.top) as f64,
            right: f32::from(insets.right) as f64,
            bottom: f32::from(insets.bottom) as f64,
            left: f32::from(insets.left) as f64,
        }
    }
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn edge_insets_js(
    insets: gpui::Edges<gpui::Pixels>,
) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
    let object = js_sys::Object::new();
    for (key, value) in [
        ("top", insets.top),
        ("right", insets.right),
        ("bottom", insets.bottom),
        ("left", insets.left),
    ] {
        js_sys::Reflect::set(
            &object,
            &wasm_bindgen::JsValue::from_str(key),
            &wasm_bindgen::JsValue::from_f64(f32::from(value) as f64),
        )?;
    }
    Ok(object.into())
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn window_insets_js(
    insets: gpui::WindowInsets,
) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
    let effective = insets.effective();
    let object = js_sys::Object::new();
    for (key, value) in [
        ("safeArea", edge_insets_js(insets.safe_area)?),
        ("ime", edge_insets_js(insets.ime)?),
        ("effective", edge_insets_js(effective)?),
    ] {
        js_sys::Reflect::set(&object, &wasm_bindgen::JsValue::from_str(key), &value)?;
    }
    Ok(object.into())
}

/// Recorded draw times from the debug frame overlay.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[derive(Debug, Clone)]
#[napi(object)]
pub struct DebugFrameOverlayStats {
    pub current_ms: Option<f64>,
    pub p90_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub max_ms: Option<f64>,
    pub frames: f64,
    pub samples: f64,
}

#[derive(Debug, Clone)]
#[cfg_attr(not(all(target_arch = "wasm32", target_os = "unknown")), napi(object))]
pub struct WindowOptions {
    pub title: Option<String>,
    /// The name used inside the macOS "Hide" and "Quit" menu items. Defaults to
    /// `title`. It does NOT set the title of the application menu itself: macOS
    /// takes that from the executable, and only a `.app` bundle changes it.
    pub app_name: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub min_width: Option<f64>,
    pub min_height: Option<f64>,
    pub resizable: Option<bool>,
    pub fullscreen: Option<bool>,
    /// Plain alpha transparency. Prefer `window_background` when you need blur.
    pub transparent: Option<bool>,
    /// Hide the native titlebar so the app can draw chrome under the traffic lights.
    pub titlebar_transparent: Option<bool>,
    /// `"opaque"` | `"transparent"` | `"blurred"`. `transparent: true` is the
    /// same as `"transparent"` when this is unset.
    pub window_background: Option<String>,
    pub traffic_light_x: Option<f64>,
    pub traffic_light_y: Option<f64>,
    /// Give the window key focus when it opens. `false` opens it behind the
    /// active app, like `open -g`. Ignored on Linux.
    pub focus: Option<bool>,
    /// Show the window when it opens. `false` opens it hidden; call
    /// `activateWindow()` to reveal it. Ignored on Linux.
    pub show: Option<bool>,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: Some("GPUIX".to_string()),
            app_name: None,
            width: Some(800.0),
            height: Some(600.0),
            min_width: None,
            min_height: None,
            resizable: Some(true),
            fullscreen: Some(false),
            transparent: Some(false),
            titlebar_transparent: Some(false),
            window_background: None,
            traffic_light_x: None,
            traffic_light_y: None,
            focus: Some(true),
            show: Some(true),
        }
    }
}

fn to_gpui_window_options(
    options: &WindowOptions,
    bounds: gpui::Bounds<gpui::Pixels>,
) -> gpui::WindowOptions {
    let title = options.title.clone().unwrap_or_else(|| "GPUIX".to_string());
    let titlebar_transparent = options.titlebar_transparent.unwrap_or(false);
    let traffic_light_position = match (options.traffic_light_x, options.traffic_light_y) {
        (Some(x), Some(y)) => Some(gpui::point(gpui::px(x as f32), gpui::px(y as f32))),
        _ => None,
    };
    let window_background = match options.window_background.as_deref() {
        Some("transparent") => gpui::WindowBackgroundAppearance::Transparent,
        Some("blurred") => gpui::WindowBackgroundAppearance::Blurred,
        Some("opaque") => gpui::WindowBackgroundAppearance::Opaque,
        _ if options.transparent.unwrap_or(false) => gpui::WindowBackgroundAppearance::Transparent,
        _ => gpui::WindowBackgroundAppearance::Opaque,
    };
    let window_min_size = match (options.min_width, options.min_height) {
        (Some(width), Some(height)) => {
            Some(gpui::size(gpui::px(width as f32), gpui::px(height as f32)))
        }
        _ => None,
    };
    let window_bounds = if options.fullscreen.unwrap_or(false) {
        gpui::WindowBounds::Fullscreen(bounds)
    } else {
        gpui::WindowBounds::Windowed(bounds)
    };
    gpui::WindowOptions {
        window_bounds: Some(window_bounds),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: titlebar_transparent,
            traffic_light_position,
        }),
        is_resizable: options.resizable.unwrap_or(true),
        window_background,
        window_min_size,
        focus: options.focus.unwrap_or(true),
        show: options.show.unwrap_or(true),
        ..Default::default()
    }
}

#[cfg(test)]
mod highlight_cache_tests {
    use super::*;

    fn tree_with_text() -> RetainedTree {
        let mut tree = RetainedTree::new();
        tree.create_element(1, "div".to_string());
        tree.create_element(2, "text".to_string());
        tree.append_child(1, 2);
        tree.set_text(2, "a fox and a fox".to_string());
        tree
    }

    fn query(text: &str) -> serde_json::Value {
        serde_json::json!({ "query": text })
    }

    fn declare(tree: &mut RetainedTree, value: &serde_json::Value) {
        tree.set_custom_prop(1, "highlight".to_string(), value.clone());
    }

    /// The whole reason `search_revision` exists. `highlight` is a custom prop,
    /// so keying the group list on `subtree_revision` means every keystroke
    /// re-walks and re-folds the subtree. The pointer comparison is the proof;
    /// a timing budget over a realistic app is far too coarse to catch it.
    #[test]
    fn a_query_change_reuses_the_group_list() {
        let theme = Theme::dark();
        let mut tree = tree_with_text();
        let mut cache = HashMap::new();

        declare(&mut tree, &query("f"));
        resolve_highlight(&mut cache, &tree, 1, &query("f"), &theme, false).expect("resolves");
        let first = Arc::as_ptr(&cache[&1].groups);

        declare(&mut tree, &query("fo"));
        resolve_highlight(&mut cache, &tree, 1, &query("fo"), &theme, false).expect("resolves");
        assert_eq!(
            Arc::as_ptr(&cache[&1].groups),
            first,
            "a query change must not rebuild the group list"
        );
    }

    /// Moving a find cursor changes no text and no matcher, so it must re-use
    /// the located matches. Colours and ordinals are decided at paint.
    #[test]
    fn a_cursor_move_reuses_the_located_matches() {
        let theme = Theme::dark();
        let mut tree = tree_with_text();
        let mut cache = HashMap::new();
        let spec = |active: u64| serde_json::json!({ "query": "fox", "activeIndex": active });

        declare(&mut tree, &spec(0));
        resolve_highlight(&mut cache, &tree, 1, &spec(0), &theme, true).expect("resolves");
        let matches = Arc::as_ptr(&cache[&1].context.matches);

        declare(&mut tree, &spec(1));
        let (context, changed) =
            resolve_highlight(&mut cache, &tree, 1, &spec(1), &theme, true).expect("resolves");
        assert_eq!(Arc::as_ptr(&context.matches), matches, "no rescan");
        assert_eq!(changed, None, "a cursor move is not a new result");
        assert_eq!(
            context.set.specs[0].active_index,
            Some(1),
            "spec still swapped"
        );
    }

    /// Editing the text must invalidate, or the wash paints over stale offsets.
    #[test]
    fn a_text_change_rebuilds_the_group_list() {
        let theme = Theme::dark();
        let mut tree = tree_with_text();
        let mut cache = HashMap::new();

        declare(&mut tree, &query("fox"));
        resolve_highlight(&mut cache, &tree, 1, &query("fox"), &theme, true).expect("resolves");
        let first = Arc::as_ptr(&cache[&1].groups);

        tree.set_text(2, "one fox only".to_string());
        let (_, changed) =
            resolve_highlight(&mut cache, &tree, 1, &query("fox"), &theme, true).expect("resolves");
        assert_ne!(Arc::as_ptr(&cache[&1].groups), first);
        assert_eq!(changed, Some(1), "two matches became one");
    }

    /// A review caught this: `reported` used to be written even with no
    /// listener, so mounting without `onHighlight` and adding it later reported
    /// nothing, forever.
    #[test]
    fn adding_the_listener_later_still_reports() {
        let theme = Theme::dark();
        let mut tree = tree_with_text();
        let mut cache = HashMap::new();

        declare(&mut tree, &query("fox"));
        let (_, changed) = resolve_highlight(&mut cache, &tree, 1, &query("fox"), &theme, false)
            .expect("resolves");
        assert_eq!(changed, None, "nothing to report without a listener");

        let (_, changed) =
            resolve_highlight(&mut cache, &tree, 1, &query("fox"), &theme, true).expect("resolves");
        assert_eq!(changed, Some(2), "the listener gets the current count");

        let (_, changed) =
            resolve_highlight(&mut cache, &tree, 1, &query("fox"), &theme, true).expect("resolves");
        assert_eq!(changed, None, "and only once");
    }
}

/// The `applyBatch` protocol. This is the surface JS talks to, so every rule it
/// relies on is asserted here against real JSON bytes rather than through a
/// hand-built `Vec<BatchOp>`.
#[cfg(test)]
mod batch_tests {
    use super::*;

    fn apply(tree: &mut RetainedTree, json: &str) -> batch::BatchResult<Vec<f64>> {
        apply_batch_to_tree(tree, json.as_bytes())
    }

    /// Everything a mutation can reach, so an unwanted partial apply shows up
    /// as a diff instead of hiding in a field the test forgot to read.
    fn describe(tree: &RetainedTree) -> String {
        let mut ids: Vec<_> = tree.elements.keys().copied().collect();
        ids.sort_unstable();
        let mut out = format!("root={:?}\n", tree.root_id);
        for id in ids {
            let element = &tree.elements[&id];
            let mut events: Vec<_> = element.events.iter().cloned().collect();
            events.sort();
            let mut props: Vec<_> = element.custom_props.iter().collect();
            props.sort_by(|(a, _), (b, _)| a.cmp(b));
            out += &format!(
                "{id} type={} text={:?} style={:?} children={:?} parent={:?} events={events:?} props={props:?} rev={}/{}\n",
                element.element_type,
                element.content,
                element.style.as_deref(),
                element.children,
                element.parent,
                element.subtree_revision,
                element.search_revision,
            );
        }
        out
    }

    /// The regression test for batch atomicity. `intern_style_payload` used to
    /// run inside the apply loop, so this batch created the element, set its
    /// text, and only then threw — leaving JS to retry against a tree that had
    /// already moved.
    #[test]
    fn a_malformed_style_applies_nothing_at_all() {
        let mut tree = RetainedTree::new();
        apply(&mut tree, r#"[["createElement",1,"div"],["setRoot",1]]"#).expect("valid batch");
        let before = describe(&tree);
        let styles_before = tree.styles.len();

        let error = apply(
            &mut tree,
            r#"[["createElement",2,"div"],["setText",2,"changed"],["setStyle",2,123]]"#,
        )
        .expect_err("a malformed style must reject the batch");

        assert_eq!(describe(&tree), before, "the tree must be untouched");
        assert_eq!(
            tree.styles.len(),
            styles_before,
            "the failed batch must not leave styles interned"
        );
        assert!(error.contains("setStyle"), "{error}");
    }

    /// A style that fails halfway through a long batch is unfindable without
    /// its index; serde reports a byte offset, which names nothing.
    #[test]
    fn a_style_error_names_its_op_index() {
        let mut tree = RetainedTree::new();
        let error = apply(
            &mut tree,
            r#"[["createElement",1,"div"],["setStyle",1,{"color":"red"}],["setStyle",1,{"color":5}]]"#,
        )
        .expect_err("a bad style rejects the batch");
        assert!(
            error.starts_with("Batch op 2 setStyle parse error:"),
            "{error}"
        );
    }

    /// `null` is not "no style". Treating it as `{}` would silently clear every
    /// declared property instead of telling JS it sent something wrong.
    #[test]
    fn a_null_style_is_an_error() {
        let mut tree = RetainedTree::new();
        let error = apply(
            &mut tree,
            r#"[["createElement",1,"div"],["setStyle",1,null]]"#,
        )
        .expect_err("null is not a style");
        assert!(
            error.contains("Batch op 1 setStyle parse error:"),
            "{error}"
        );
        assert!(tree.elements.is_empty(), "and the batch stays atomic");
    }

    /// Skipping an unknown opcode would let a JS/Rust version skew desync the
    /// tree quietly. It has to throw.
    #[test]
    fn an_unknown_opcode_is_an_error() {
        let mut tree = RetainedTree::new();
        let error = apply(&mut tree, r#"[["teleportElement",1]]"#).expect_err("unknown opcode");
        assert!(error.contains("unknown operation"), "{error}");
        assert!(tree.elements.is_empty());
    }

    /// Every op that takes an id must validate it. A fractional or oversized id
    /// would truncate into a *different* element, which is a silent desync.
    #[test]
    fn an_invalid_id_is_rejected_in_every_id_position() {
        let templates = [
            r#"[["createElement",ID,"div"]]"#,
            r#"[["destroyElement",ID]]"#,
            r#"[["appendChild",ID,2]]"#,
            r#"[["appendChild",1,ID]]"#,
            r#"[["insertBefore",ID,2,3]]"#,
            r#"[["insertBefore",1,ID,3]]"#,
            r#"[["insertBefore",1,2,ID]]"#,
            r#"[["setStyle",ID,{}]]"#,
            r#"[["setText",ID,"x"]]"#,
            r#"[["setEventListener",ID,"click",true]]"#,
            r#"[["setRoot",ID]]"#,
            r#"[["setCustomProp",ID,"k",1]]"#,
        ];
        // 1e999 overflows f64, 9007199254740992 is Number.MAX_SAFE_INTEGER + 1.
        let bad_ids = ["-1", "1.5", "9007199254740992", "1e999"];

        for template in templates {
            for bad in bad_ids {
                let json = template.replace("ID", bad);
                let mut tree = RetainedTree::new();
                let error = apply(&mut tree, &json).expect_err(&format!("{json} must be rejected"));
                assert!(error.contains("Batch op 0"), "{json}: {error}");
                assert!(tree.elements.is_empty(), "{json} mutated the tree");
                assert_eq!(tree.root_id, None, "{json} mutated the root");
            }
        }
    }

    #[test]
    fn has_handler_requires_a_bool() {
        for (payload, expected) in [("true", true), ("false", false)] {
            let mut tree = RetainedTree::new();
            let json =
                format!(r#"[["createElement",1,"div"],["setEventListener",1,"click",{payload}]]"#);
            apply(&mut tree, &json).expect("boolean handler state");
            assert_eq!(
                tree.elements[&1].events.contains("click"),
                expected,
                "hasHandler {payload}"
            );
        }

        for payload in ["0", "1", "0.5", r#""true""#] {
            let mut tree = RetainedTree::new();
            let json =
                format!(r#"[["createElement",1,"div"],["setEventListener",1,"click",{payload}]]"#);
            apply(&mut tree, &json).expect_err(&format!("hasHandler {payload} is not a boolean"));
        }
    }

    #[test]
    fn a_malformed_op_tuple_is_an_error() {
        let cases = [
            (r#"[42]"#, "a non-array op"),
            (r#"[["createElement",1]]"#, "a missing argument"),
            (r#"[[7,1,"div"]]"#, "a non-string op name"),
        ];
        for (json, what) in cases {
            let mut tree = RetainedTree::new();
            let error = apply(&mut tree, json).expect_err(what);
            assert!(
                error.starts_with("Failed to parse batch:"),
                "{what}: {error}"
            );
            assert!(tree.elements.is_empty(), "{what} mutated the tree");
        }
    }

    /// Interning keys on raw bytes, so re-ordered keys are two `Arc`s. They are
    /// still the same style, and a repaint per key order would be a real cost
    /// on any app that builds style objects conditionally.
    #[test]
    fn a_reordered_style_does_not_repaint() {
        let mut tree = RetainedTree::new();
        apply(
            &mut tree,
            r#"[["createElement",1,"div"],["setStyle",1,{"color":"red","left":10}]]"#,
        )
        .expect("valid batch");
        let revision = tree.elements[&1].subtree_revision;

        apply(&mut tree, r#"[["setStyle",1,{"left":10,"color":"red"}]]"#).expect("valid batch");
        assert_eq!(
            tree.elements[&1].subtree_revision, revision,
            "the same style in another key order is not a change"
        );
    }

    /// Three ways an interned style loses its last element reference.
    #[test]
    fn a_style_is_released_when_nothing_references_it() {
        let mut tree = RetainedTree::new();
        apply(&mut tree, r#"[["createElement",1,"div"]]"#).expect("valid batch");

        // Set on an id that does not exist: nothing keeps the style alive.
        apply(&mut tree, r#"[["setStyle",99,{"color":"red"}]]"#).expect("missing ids are ignored");
        tree.styles.sweep();
        assert_eq!(tree.styles.len(), 0, "a style nobody took must be released");

        apply(&mut tree, r#"[["setStyle",1,{"color":"red"}]]"#).expect("valid batch");
        tree.styles.sweep();
        assert_eq!(tree.styles.len(), 1);

        // Replaced.
        apply(&mut tree, r#"[["setStyle",1,{"color":"blue"}]]"#).expect("valid batch");
        tree.styles.sweep();
        assert_eq!(tree.styles.len(), 1, "the replaced style must be released");

        // Destroyed.
        apply(&mut tree, r#"[["destroyElement",1]]"#).expect("valid batch");
        tree.styles.sweep();
        assert_eq!(tree.styles.len(), 0);
    }
}

#[cfg(test)]
mod window_options_tests {
    use super::*;

    fn mapped(options: WindowOptions) -> gpui::WindowOptions {
        let bounds = gpui::Bounds {
            origin: gpui::point(gpui::px(0.0), gpui::px(0.0)),
            size: gpui::size(gpui::px(800.0), gpui::px(600.0)),
        };
        to_gpui_window_options(&options, bounds)
    }

    #[test]
    fn defaults_open_a_focused_visible_window() {
        let gpui_options = mapped(WindowOptions::default());
        assert!(gpui_options.focus);
        assert!(gpui_options.show);
    }

    #[test]
    fn unset_focus_and_show_still_default_to_true() {
        let gpui_options = mapped(WindowOptions {
            focus: None,
            show: None,
            ..WindowOptions::default()
        });
        assert!(gpui_options.focus);
        assert!(gpui_options.show);
    }

    #[test]
    fn focus_false_leaves_the_window_visible() {
        let gpui_options = mapped(WindowOptions {
            focus: Some(false),
            ..WindowOptions::default()
        });
        assert!(!gpui_options.focus);
        assert!(gpui_options.show);
    }

    #[test]
    fn show_false_keeps_focus_independent() {
        let gpui_options = mapped(WindowOptions {
            show: Some(false),
            ..WindowOptions::default()
        });
        assert!(!gpui_options.show);
        assert!(gpui_options.focus);
    }

    #[test]
    fn existing_options_are_still_mapped() {
        let gpui_options = mapped(WindowOptions {
            title: Some("Background".to_string()),
            resizable: Some(false),
            window_background: Some("blurred".to_string()),
            min_width: Some(320.0),
            min_height: Some(240.0),
            focus: Some(false),
            ..WindowOptions::default()
        });
        let titlebar = gpui_options.titlebar.expect("titlebar options");
        assert_eq!(titlebar.title.as_deref(), Some("Background"));
        assert!(!gpui_options.is_resizable);
        assert_eq!(
            gpui_options.window_background,
            gpui::WindowBackgroundAppearance::Blurred
        );
        assert_eq!(
            gpui_options.window_min_size,
            Some(gpui::size(gpui::px(320.0), gpui::px(240.0)))
        );
    }
}
