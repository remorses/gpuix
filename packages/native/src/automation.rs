//! Shared automation host: paint bounds and a controllable motion clock.
//!
//! Record bounds during **paint**, not prepaint. The frame reset canvas
//! clears the map in paint, and GPUI prepaint runs for the whole tree
//! before any paint. A prepaint recorder would be wiped by the reset.
//!
//! TestGpuixRenderer and GpuixRenderer both use this so locators, screenshots,
//! and clock control do not fork between headless tests and a live window.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{
    canvas, point, px, App, Bounds, InputEvent, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke,
    Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Styled, Window,
};
use web_time::Instant;

#[derive(Clone, Copy, Debug)]
pub struct ElementBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ElementBounds {
    fn from_gpui(bounds: Bounds<Pixels>) -> Self {
        Self {
            x: f64::from(f32::from(bounds.origin.x)),
            y: f64::from(f32::from(bounds.origin.y)),
            width: f64::from(f32::from(bounds.size.width)),
            height: f64::from(f32::from(bounds.size.height)),
        }
    }
}

thread_local! {
    static BOUNDS: RefCell<HashMap<u64, ElementBounds>> = RefCell::new(HashMap::new());
}

/// Zero-size canvas. Keep it ahead of the app subtree under the root.
///
/// Everything here is recorded during **paint**, never prepaint: gpui's
/// `List::prepaint` speculatively prepaints a row range and can roll the window
/// back and prepaint a different one, so a prepaint-recorded box can belong to a
/// row that never reached the screen.
pub fn bounds_frame_reset() -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |_, _, _, _| {
            BOUNDS.with(|cell| cell.borrow_mut().clear());
        },
    )
    .absolute()
    .w(px(0.0))
    .h(px(0.0))
}

/// Record this element's own painted box, with no extra element in the tree.
///
/// `bounds_tracker` needs a positioned parent and one canvas child, which a leaf
/// such as `gpui::img` cannot have. Wrapping the leaf in a div instead would
/// move the layout box: the wrapper would become the flex item and the image
/// would lose intrinsic sizing and corner clipping.
pub fn track_own_bounds<E: gpui::InteractiveElement>(el: E, id: u64) -> E {
    el.on_painted(move |bounds, _, _| record_bounds(id, bounds))
}

/// Record a host element's painted box, and its selection-start region, for
/// automation and text selection.
///
/// The element reports its own box through `on_painted`, so tracking it adds
/// nothing to the tree. The previous `bounds_tracker` child doubled the number
/// of nodes gpui laid out, prepainted and painted for every `<div>` and
/// `<text>`.
///
/// gpui paints nothing for `visibility: hidden`, not the element and not its
/// children, so a hidden element reports no box and fires no `onLayout`. A
/// canvas child cannot work around that: it is never painted either.
///
/// The box is the border box, which is what a DOM `getBoundingClientRect`
/// reports. `bounds_tracker` fills the padding box instead.
///
/// `report` sees the painted box right after it is recorded. `on_painted`
/// takes one listener, so the paint-driven events share this one.
pub fn track_host_bounds<E>(
    el: E,
    id: u64,
    selection_start: Option<bool>,
    report: Option<PaintReporter>,
) -> E
where
    E: gpui::InteractiveElement,
{
    el.on_painted(move |bounds, _, cx| {
        record_bounds(id, bounds);
        if let Some(selectable) = selection_start {
            crate::text::record_start_region(bounds, selectable);
        }
        if let Some(report) = &report {
            report(bounds, cx);
        }
    })
}

/// Called with an element's painted box on every paint, from inside paint. It
/// is how `onLayout` and `onScrollChange` see what the frame produced.
pub type PaintReporter = Rc<dyn Fn(Bounds<Pixels>, &mut App)>;

pub fn record_bounds(id: u64, bounds: Bounds<Pixels>) {
    BOUNDS.with(|cell| {
        cell.borrow_mut()
            .insert(id, ElementBounds::from_gpui(bounds));
    });
}

pub fn get_bounds(id: u64) -> Option<ElementBounds> {
    BOUNDS.with(|cell| cell.borrow().get(&id).copied())
}

pub fn all_bounds() -> HashMap<u64, ElementBounds> {
    BOUNDS.with(|cell| cell.borrow().clone())
}

/// What the last completed paint produced, readable from any thread.
///
/// `BOUNDS` is thread-local to the thread that paints. On Windows and Linux
/// that is the UI thread, so the JS thread used to ask it for a bounds read
/// through a UI command and wait a frame for the answer. If the UI thread was
/// blocked, for example by a native file dialog, the read timed out. The UI
/// thread publishes the finished frame here instead, so reads never wait on it.
#[derive(Default)]
struct PaintedFrame {
    bounds: HashMap<u64, ElementBounds>,
    viewport: Option<(f64, f64)>,
}

#[derive(Clone, Default)]
pub struct SharedPaintedFrame(Arc<Mutex<PaintedFrame>>);

// macOS reads BOUNDS directly, so only Windows and Linux call these.
#[cfg_attr(target_os = "macos", allow(dead_code))]
impl SharedPaintedFrame {
    fn publish(&self, bounds: HashMap<u64, ElementBounds>, viewport: gpui::Size<Pixels>) {
        let mut frame = self.0.lock().unwrap();
        frame.bounds = bounds;
        frame.viewport = Some((
            f64::from(f32::from(viewport.width)),
            f64::from(f32::from(viewport.height)),
        ));
    }

    pub fn bounds(&self, id: u64) -> Option<ElementBounds> {
        self.0.lock().unwrap().bounds.get(&id).copied()
    }

    pub fn all_bounds(&self) -> HashMap<u64, ElementBounds> {
        self.0.lock().unwrap().bounds.clone()
    }

    /// `None` until the first frame has painted.
    pub fn viewport(&self) -> Option<(f64, f64)> {
        self.0.lock().unwrap().viewport
    }
}

/// Publishes the finished frame to `frame`. Keep it as a child of the root.
///
/// It is deferred with the highest priority so it paints after every other
/// deferred draw, such as `<anchored>`, which records bounds during its own
/// paint. Publishing from an ordinary trailing child would miss those.
pub fn bounds_frame_publish(frame: SharedPaintedFrame) -> impl IntoElement {
    gpui::deferred(
        canvas(
            |_, _, _| (),
            move |_, _, window, _| frame.publish(all_bounds(), window.viewport_size()),
        )
        .absolute()
        .w(px(0.0))
        .h(px(0.0)),
    )
    .with_priority(usize::MAX)
}

pub fn bounds_tracker(id: u64, selection_start: Option<bool>) -> impl IntoElement {
    canvas(
        |bounds, _, _| bounds,
        move |bounds, _, _, _| {
            record_bounds(id, bounds);
            if let Some(selectable) = selection_start {
                crate::text::record_start_region(bounds, selectable);
            }
        },
    )
    // Inset 0, or Taffy places an absolute child at the content box and the
    // box, click target and selection region all shift by the padding.
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

enum ClockMode {
    Live,
    Frozen { now: Instant },
}

struct ClockInner {
    origin: Instant,
    mode: ClockMode,
}

#[derive(Clone)]
pub struct AutomationClock {
    inner: Arc<Mutex<ClockInner>>,
}

impl Default for AutomationClock {
    fn default() -> Self {
        Self::new()
    }
}

impl AutomationClock {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ClockInner {
                origin: Instant::now(),
                mode: ClockMode::Live,
            })),
        }
    }

    pub fn now(&self) -> Instant {
        let inner = self.inner.lock().unwrap();
        match inner.mode {
            ClockMode::Live => Instant::now(),
            ClockMode::Frozen { now } => now,
        }
    }

    #[allow(dead_code)]
    pub fn now_ms(&self) -> f64 {
        let inner = self.inner.lock().unwrap();
        current_instant(&inner)
            .saturating_duration_since(inner.origin)
            .as_secs_f64()
            * 1000.0
    }

    pub fn pause(&self) -> f64 {
        let mut inner = self.inner.lock().unwrap();
        let now = current_instant(&inner);
        inner.mode = ClockMode::Frozen { now };
        now.saturating_duration_since(inner.origin).as_secs_f64() * 1000.0
    }

    pub fn set_ms(&self, now_ms: f64) -> f64 {
        let mut inner = self.inner.lock().unwrap();
        let now = inner.origin + duration_ms(now_ms);
        inner.mode = ClockMode::Frozen { now };
        now_ms
    }

    pub fn fast_forward_ms(&self, delta_ms: f64) -> f64 {
        let mut inner = self.inner.lock().unwrap();
        let now = current_instant(&inner) + duration_ms(delta_ms);
        inner.mode = ClockMode::Frozen { now };
        now.saturating_duration_since(inner.origin).as_secs_f64() * 1000.0
    }

    pub fn resume(&self) -> f64 {
        let mut inner = self.inner.lock().unwrap();
        let elapsed = current_instant(&inner).saturating_duration_since(inner.origin);
        inner.origin = Instant::now() - elapsed;
        inner.mode = ClockMode::Live;
        elapsed.as_secs_f64() * 1000.0
    }
}

fn current_instant(inner: &ClockInner) -> Instant {
    match inner.mode {
        ClockMode::Live => Instant::now(),
        ClockMode::Frozen { now } => now,
    }
}

fn duration_ms(ms: f64) -> Duration {
    Duration::from_secs_f64((ms / 1000.0).max(0.0))
}

pub fn mouse_button(button: u32) -> MouseButton {
    match button {
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        _ => MouseButton::Left,
    }
}

/// Parse the held modifiers of a simulated mouse event from the same
/// hyphenated syntax `press("cmd-a")` already uses: `"cmd"`, `"cmd-shift"`,
/// `"alt"`. `None` and `""` mean no modifier. Unknown names are ignored, so a
/// typo weakens the gesture instead of failing the whole automation call.
///
/// A string, not an object, because the same value has to cross the napi, the
/// wasm and the stdio boundary, and only wasm makes objects awkward.
pub fn parse_modifiers(modifiers: Option<&str>) -> Modifiers {
    let mut parsed = Modifiers::default();
    let Some(text) = modifiers else {
        return parsed;
    };
    for part in text.split('-') {
        match part.trim().to_ascii_lowercase().as_str() {
            "cmd" | "meta" | "super" | "win" | "platform" => parsed.platform = true,
            "ctrl" | "control" => parsed.control = true,
            "alt" | "option" => parsed.alt = true,
            "shift" => parsed.shift = true,
            "fn" | "function" => parsed.function = true,
            _ => {}
        }
    }
    parsed
}

pub fn dispatch_keystrokes(
    window: &mut Window,
    cx: &mut App,
    keystrokes: &str,
) -> Result<(), String> {
    for keystroke in keystrokes.split(' ') {
        window.dispatch_keystroke(parse_keystroke(keystroke)?, cx);
    }
    Ok(())
}

pub fn dispatch_key_down(
    window: &mut Window,
    cx: &mut App,
    keystroke: &str,
    is_held: bool,
) -> Result<(), String> {
    window.dispatch_event(
        KeyDownEvent {
            keystroke: parse_keystroke(keystroke)?,
            is_held,
            prefer_character_input: false,
        }
        .to_platform_input(),
        cx,
    );
    Ok(())
}

pub fn dispatch_key_up(window: &mut Window, cx: &mut App, keystroke: &str) -> Result<(), String> {
    window.dispatch_event(
        KeyUpEvent {
            keystroke: parse_keystroke(keystroke)?,
        }
        .to_platform_input(),
        cx,
    );
    Ok(())
}

fn parse_keystroke(keystroke: &str) -> Result<Keystroke, String> {
    Keystroke::parse(keystroke).map_err(|error| format!("Invalid keystroke '{keystroke}': {error}"))
}

/// Every automation mouse dispatcher takes modifiers, so a test can drive
/// cmd-wheel zoom, shift-click range selection, or alt-drag duplication.
pub fn dispatch_click(
    window: &mut Window,
    cx: &mut App,
    x: f64,
    y: f64,
    button: u32,
    modifiers: Modifiers,
) {
    let position = point(px(x as f32), px(y as f32));
    let button = mouse_button(button);
    window.dispatch_event(
        MouseDownEvent {
            button,
            position,
            modifiers,
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
    window.dispatch_event(
        MouseUpEvent {
            button,
            position,
            modifiers,
            click_count: 1,
        }
        .to_platform_input(),
        cx,
    );
}

pub fn dispatch_mouse_down(
    window: &mut Window,
    cx: &mut App,
    x: f64,
    y: f64,
    button: u32,
    modifiers: Modifiers,
) {
    window.dispatch_event(
        MouseDownEvent {
            button: mouse_button(button),
            position: point(px(x as f32), px(y as f32)),
            modifiers,
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
}

pub fn dispatch_mouse_up(
    window: &mut Window,
    cx: &mut App,
    x: f64,
    y: f64,
    button: u32,
    modifiers: Modifiers,
) {
    window.dispatch_event(
        MouseUpEvent {
            button: mouse_button(button),
            position: point(px(x as f32), px(y as f32)),
            modifiers,
            click_count: 1,
        }
        .to_platform_input(),
        cx,
    );
}

pub fn dispatch_mouse_move(
    window: &mut Window,
    cx: &mut App,
    x: f64,
    y: f64,
    pressed_button: Option<u32>,
    modifiers: Modifiers,
) {
    window.dispatch_event(
        MouseMoveEvent {
            position: point(px(x as f32), px(y as f32)),
            pressed_button: pressed_button.map(mouse_button),
            modifiers,
        }
        .to_platform_input(),
        cx,
    );
}

pub fn dispatch_scroll_wheel(
    window: &mut Window,
    cx: &mut App,
    x: f64,
    y: f64,
    delta_x: f64,
    delta_y: f64,
    modifiers: Modifiers,
) {
    window.dispatch_event(
        gpui::ScrollWheelEvent {
            position: point(px(x as f32), px(y as f32)),
            delta: gpui::ScrollDelta::Pixels(point(px(delta_x as f32), px(delta_y as f32))),
            modifiers,
            touch_phase: gpui::TouchPhase::Moved,
        }
        .to_platform_input(),
        cx,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_clock_holds_and_fast_forwards() {
        let clock = AutomationClock::new();
        clock.set_ms(0.0);
        assert!((clock.now_ms() - 0.0).abs() < 0.001);
        clock.fast_forward_ms(150.0);
        assert!((clock.now_ms() - 150.0).abs() < 0.001);
        let later = clock.now();
        clock.fast_forward_ms(150.0);
        assert_eq!(
            clock.now().saturating_duration_since(later),
            Duration::from_millis(150)
        );
    }
}
