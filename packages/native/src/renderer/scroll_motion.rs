//! CSS `scroll-behavior`, `scroll-snap-*` and `scroll-initial-target`.
//!
//! A smooth programmatic scroll is an animation on the scroll offset. The
//! render loop steps every animation once per frame, and a wheel that moves
//! the box away from the animation cancels it, the way a browser does.
//!
//! Scroll snap watches each snap container. While the offset moves, the
//! container is active. When it has rested for `IDLE_SECONDS`, the container
//! picks the nearest snap position among its snap areas and glides to it.
//! `mandatory` always snaps. `proximity` snaps only when the position is
//! within half a viewport. `scroll-snap-stop: always` on an area stops a
//! scroll that would pass over it.
//!
//! `scroll-initial-target: nearest` on an element scrolls its ancestors to
//! it once, on the first frame after the element paints.
//!
//! All state lives in thread locals on the render thread, next to
//! `SCROLL_HANDLES`, so the napi methods reach it without an App context.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use std::time::Instant;

use gpui::{point, px, Pixels, Point, ScrollHandle};

use crate::motion::{ease, mix, MotionEase};
use crate::retained_tree::RetainedTree;
use crate::style::StyleDesc;

use super::scroll_into_view::{
    axis_delta, scroll_into_view, scroll_margin, scroll_padding, Align, Container,
};

/// `GPUIX_SNAP_DEBUG=1` logs every wheel phase, each lift with its
/// predicted landing and sample ages, and each chosen snap target.
static SNAP_DEBUG: LazyLock<bool> =
    LazyLock::new(|| std::env::var_os("GPUIX_SNAP_DEBUG").is_some());

/// How long a smooth scroll takes, in seconds.
const SMOOTH_SECONDS: f64 = 0.3;
/// How long the offset must rest before a snap container snaps. A step
/// under half a pixel does not reset the timer, so the glide starts
/// during the momentum tail of a wheel instead of after it.
const IDLE_SECONDS: f64 = 0.08;

fn smooth_ease() -> MotionEase {
    MotionEase::Name("easeInOut".to_string())
}

/// The `behavior` option of `scrollTo` and `scrollIntoView`. `Auto` reads
/// the `scroll-behavior` of each scroll box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Behavior {
    Auto,
    Instant,
    Smooth,
}

impl Behavior {
    pub(crate) fn parse(word: Option<&str>) -> Behavior {
        match word.map(str::trim) {
            Some("smooth") => Behavior::Smooth,
            Some("instant") => Behavior::Instant,
            _ => Behavior::Auto,
        }
    }

    /// Whether a scroll of the box with this style moves smoothly.
    pub(crate) fn smooth(self, style: Option<&StyleDesc>) -> bool {
        match self {
            Behavior::Smooth => true,
            Behavior::Instant => false,
            Behavior::Auto => style
                .and_then(|style| style.scroll_behavior.as_deref())
                .is_some_and(|word| word.trim() == "smooth"),
        }
    }
}

/// How a glide moves from its start to its end.
enum Curve {
    /// A programmatic smooth scroll: `easeInOut` over `SMOOTH_SECONDS`.
    Smooth,
    /// The end of a fling, from Chromium's `cc/input/snap_fling_curve.cc`:
    /// each 16ms frame moves `FLING_RATIO` of what the frame before it
    /// moved, and the first delta makes the series sum to the whole
    /// distance. It starts fast and slows, the way momentum does, and it
    /// takes longer over a longer distance.
    Fling {
        /// The frame count that ends the series, from the distance.
        frames: f64,
    },
}

/// One running scroll animation.
struct Animation {
    from: Point<Pixels>,
    to: Point<Pixels>,
    curve: Curve,
    /// Set on the first step, so a paused test clock drives it.
    started: Option<Instant>,
    /// The offset the last step wrote. The box sitting anywhere else means
    /// the user took over, which cancels the animation.
    written: Point<Pixels>,
}

/// What a snap container did lately.
struct SnapState {
    /// The offset at the end of the last frame.
    offset: Point<Pixels>,
    /// When the offset last moved, or `None` while the box rests.
    moved_at: Option<Instant>,
    /// The offset where the movement began. `scroll-snap-stop: always`
    /// stops the first such area the scroll passed from here.
    from: Point<Pixels>,
}

/// One trackpad gesture on a snap container, from the fingers going down
/// to the momentum after they lift.
struct Gesture {
    /// The wheel deltas while the fingers are down, with their times.
    /// The lift reads its velocity from the newest of these.
    samples: Vec<(Instant, Point<Pixels>)>,
    /// The fingers are on the pad.
    down: bool,
    /// The fingers lifted and a snap glide runs. The OS keeps sending
    /// momentum events, and they are consumed so they cannot cancel it.
    coasting: bool,
    /// When the last consumed momentum event came in. The stream sends an
    /// event every few milliseconds, so a long gap means it ended.
    last_momentum: Instant,
}

/// A fling travels about this long at its lift velocity before the OS
/// decay ends it. macOS decays momentum near 0.998 per millisecond, and
/// that series sums to half a second of travel at the lift speed.
const MOMENTUM_SECONDS: f32 = 0.5;
/// Samples older than this play no part in the lift velocity.
const VELOCITY_WINDOW_SECONDS: f64 = 0.1;
/// The decay of the fling curve: each frame moves this share of what the
/// frame before it moved. Chromium uses 0.92 on desktop.
const FLING_RATIO: f64 = 0.92;
/// The frame length the fling curve counts in, as Chromium does.
const FLING_FRAME_SECONDS: f64 = 0.016;
/// No fling glide runs longer than this, whatever the distance says.
const FLING_MAX_SECONDS: f64 = 3.0;
/// A gap in the momentum stream longer than this ends it. The OS sends a
/// momentum event every few milliseconds while the stream runs.
const MOMENTUM_GAP_SECONDS: f64 = 0.1;

thread_local! {
    static ANIMATIONS: RefCell<HashMap<u64, Animation>> = RefCell::new(HashMap::new());
    static SNAP: RefCell<HashMap<u64, SnapState>> = RefCell::new(HashMap::new());
    static INITIAL_DONE: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
    static GESTURES: RefCell<HashMap<u64, Gesture>> = RefCell::new(HashMap::new());
    static DEFERRED: RefCell<HashMap<u64, Deferred>> = RefCell::new(HashMap::new());
}

/// An offset from a `scrollTo` that came before the first frame of its
/// element. A mount effect runs in the commit that creates the element,
/// before any frame builds a scroll handle for it. The offset waits here,
/// and the handle starts at it instead of at zero. The web behaves the
/// same way: a scroll set on a fresh element sticks.
struct Deferred {
    to: Point<Pixels>,
    /// On some backends the scroll command can arrive one frame before
    /// the batch that creates the element. The prune drops an entry only
    /// after it saw the element missing twice.
    seen_missing: bool,
}

/// Hold `to` for an element that has no scroll handle yet.
pub(crate) fn defer(id: u64, to: Point<Pixels>) {
    DEFERRED.with(|cell| {
        cell.borrow_mut().insert(
            id,
            Deferred {
                to,
                seen_missing: false,
            },
        );
    });
}

/// The held offset for a new scroll handle, if a `scrollTo` came before
/// the first frame of its element.
pub(crate) fn take_deferred(id: u64) -> Option<Point<Pixels>> {
    DEFERRED.with(|cell| cell.borrow_mut().remove(&id).map(|deferred| deferred.to))
}

/// Drop the held offset of an element that painted without a scroll box.
/// The web does the same: a scroll set on an element with no overflow
/// does not persist.
pub(crate) fn drop_deferred(id: u64) {
    DEFERRED.with(|cell| {
        cell.borrow_mut().remove(&id);
    });
}

/// A wheel event over a snap container, seen in the capture phase before
/// the box scrolls. The web snaps a fling the moment the fingers lift, at
/// the snap position nearest the predicted landing point. css-scroll-snap-1
/// calls that point the intended end position. Returns true when the event
/// must be consumed: the OS momentum stream after a lift, while the glide
/// that lift started still runs.
pub(crate) fn gesture_wheel(
    tree: &RetainedTree,
    handles: &HashMap<u64, ScrollHandle>,
    id: u64,
    handle: &ScrollHandle,
    delta: Point<Pixels>,
    phase: gpui::TouchPhase,
    now: Instant,
) -> bool {
    use gpui::TouchPhase;
    if *SNAP_DEBUG {
        eprintln!(
            "[snap] id={} phase={:?} delta=({:.1},{:.1})",
            id,
            phase,
            f32::from(delta.x),
            f32::from(delta.y),
        );
    }
    GESTURES.with(|cell| {
        let mut gestures = cell.borrow_mut();
        match phase {
            TouchPhase::Started => {
                // The fingers caught the box. A running glide stops at
                // once, the way Chromium clears its snap fling on a
                // gesture begin.
                ANIMATIONS.with(|cell| cell.borrow_mut().remove(&id));
                gestures.insert(
                    id,
                    Gesture {
                        samples: Vec::new(),
                        down: true,
                        coasting: false,
                        last_momentum: now,
                    },
                );
                false
            }
            TouchPhase::Moved => {
                let Some(gesture) = gestures.get_mut(&id) else {
                    // A wheel with no phases, such as a mouse. The idle
                    // watcher in `snap_containers` covers it.
                    return false;
                };
                if gesture.down {
                    gesture.samples.push((now, delta));
                    gesture
                        .samples
                        .retain(|(at, _)| (now - *at).as_secs_f64() <= VELOCITY_WINDOW_SECONDS);
                    return false;
                }
                if gesture.coasting {
                    // Chromium consumes the momentum stream while its snap
                    // fling is active and after it lands, until the next
                    // gesture begin. Without the second part, the tail of
                    // the stream pushes the box off the snap position the
                    // moment the glide ends. The stream is continuous, so
                    // a long gap means it ended and this wheel is new.
                    if (now - gesture.last_momentum).as_secs_f64() <= MOMENTUM_GAP_SECONDS {
                        gesture.last_momentum = now;
                        return true;
                    }
                    gesture.coasting = false;
                }
                false
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let Some(gesture) = gestures.get_mut(&id) else {
                    return false;
                };
                if !gesture.down {
                    return false;
                }
                gesture.down = false;
                let style = tree.elements.get(&id).and_then(|el| el.style.as_deref());
                let Some(snap) = snap_type(style) else {
                    return false;
                };
                let landing = predicted_landing(handle, &gesture.samples, now);
                if *SNAP_DEBUG {
                    let ages: Vec<u128> = gesture
                        .samples
                        .iter()
                        .map(|(at, _)| (now - *at).as_millis())
                        .collect();
                    eprintln!(
                        "[snap] lift id={} offset=({:.1},{:.1}) landing=({:.1},{:.1}) ages_ms={:?}",
                        id,
                        f32::from(handle.offset().x),
                        f32::from(handle.offset().y),
                        f32::from(landing.x),
                        f32::from(landing.y),
                        ages,
                    );
                }
                // The `scroll-snap-stop: always` scan starts at the offset
                // of the lift, not at the offset where the fingers went
                // down. The fling passes only the positions between the
                // lift and the landing. An `always` area the drag already
                // passed must not pull the box back, and Blink's fling
                // strategy also measures from the fling start.
                if let Some(target) =
                    snap_target(tree, id, snap, handle, handle.offset(), landing, handles)
                {
                    if *SNAP_DEBUG {
                        eprintln!(
                            "[snap] target id={} to=({:.1},{:.1})",
                            id,
                            f32::from(target.x),
                            f32::from(target.y),
                        );
                    }
                    if target != handle.offset() {
                        animate_fling(id, handle, target);
                        gesture.coasting = true;
                        gesture.last_momentum = now;
                    }
                }
                false
            }
        }
    })
}

/// Where the momentum of a lift would land the offset, clamped to the
/// scrollable range.
fn predicted_landing(
    handle: &ScrollHandle,
    samples: &[(Instant, Point<Pixels>)],
    now: Instant,
) -> Point<Pixels> {
    let offset = handle.offset();
    // Only fresh deltas count. The fingers resting on the pad before the
    // lift means zero velocity, not the speed of the pulls before the rest.
    let samples: Vec<_> = samples
        .iter()
        .filter(|(at, _)| (now - *at).as_secs_f64() <= VELOCITY_WINDOW_SECONDS)
        .collect();
    let span = samples
        .first()
        .map(|(at, _)| (now - *at).as_secs_f32())
        .unwrap_or(0.0);
    if span <= 0.001 {
        return offset;
    }
    let sum = samples
        .iter()
        .fold(point(px(0.0), px(0.0)), |sum, (_, delta)| {
            point(sum.x + delta.x, sum.y + delta.y)
        });
    let max = handle.max_offset();
    let travel = |sum: Pixels| sum * (MOMENTUM_SECONDS / span);
    point(
        (offset.x + travel(sum.x)).max(-max.x).min(px(0.0)),
        (offset.y + travel(sum.y)).max(-max.y).min(px(0.0)),
    )
}

/// Glide the box from where it is to `to`. A second call replaces the
/// first, so the glide re-targets rather than queues. The target clamps
/// to the scrollable range, so a glide never runs past the end.
pub(crate) fn animate(id: u64, handle: &ScrollHandle, to: Point<Pixels>) {
    insert_animation(id, handle, to, Curve::Smooth);
}

/// Glide the box to `to` the way a fling ends: fast at first and slowing
/// down. The frame count comes from Chromium's estimate: the deltas form
/// a geometric series with ratio `FLING_RATIO` whose last term is one
/// pixel, so `distance = (1 - ratio^-frames) / (1 - 1 / ratio)`, solved
/// for `frames`.
fn animate_fling(id: u64, handle: &ScrollHandle, to: Point<Pixels>) {
    let from = handle.offset();
    let distance = (f32::from(to.x - from.x) as f64).hypot(f32::from(to.y - from.y) as f64);
    let frames = (-(1.0 - distance * (1.0 - 1.0 / FLING_RATIO)).ln() / FLING_RATIO.ln())
        .ceil()
        .min(FLING_MAX_SECONDS / FLING_FRAME_SECONDS)
        .max(1.0);
    insert_animation(id, handle, to, Curve::Fling { frames });
}

fn insert_animation(id: u64, handle: &ScrollHandle, to: Point<Pixels>, curve: Curve) {
    let max = handle.max_offset();
    let to = point(
        to.x.max(-max.x).min(px(0.0)),
        to.y.max(-max.y).min(px(0.0)),
    );
    let from = handle.offset();
    if *SNAP_DEBUG {
        eprintln!(
            "[snap] glide id={} from=({:.1},{:.1}) to=({:.1},{:.1})",
            id,
            f32::from(from.x),
            f32::from(from.y),
            f32::from(to.x),
            f32::from(to.y),
        );
    }
    ANIMATIONS.with(|cell| {
        cell.borrow_mut().insert(
            id,
            Animation {
                from,
                to,
                curve,
                started: None,
                written: from,
            },
        );
    });
}

/// Step every animation and every snap container once. Returns true while
/// anything still moves or waits, so the caller keeps frames coming.
pub(crate) fn frame(
    tree: &RetainedTree,
    handles: &HashMap<u64, ScrollHandle>,
    now: Instant,
) -> bool {
    prune(tree);
    let mut active = step_animations(handles, now);
    active |= initial_targets(tree, handles);
    active |= snap_containers(tree, handles, now);
    active
}

fn prune(tree: &RetainedTree) {
    ANIMATIONS.with(|cell| {
        cell.borrow_mut()
            .retain(|id, _| tree.elements.contains_key(id))
    });
    SNAP.with(|cell| {
        cell.borrow_mut()
            .retain(|id, _| tree.elements.contains_key(id))
    });
    // An id that left the tree re-arms, so a remounted element scrolls into
    // view again.
    INITIAL_DONE.with(|cell| {
        cell.borrow_mut()
            .retain(|id| tree.elements.contains_key(id))
    });
    GESTURES.with(|cell| {
        cell.borrow_mut()
            .retain(|id, _| tree.elements.contains_key(id))
    });
    DEFERRED.with(|cell| {
        cell.borrow_mut().retain(|id, deferred| {
            if tree.elements.contains_key(id) {
                deferred.seen_missing = false;
                return true;
            }
            if deferred.seen_missing {
                return false;
            }
            deferred.seen_missing = true;
            true
        })
    });
}

fn step_animations(handles: &HashMap<u64, ScrollHandle>, now: Instant) -> bool {
    ANIMATIONS.with(|cell| {
        let mut animations = cell.borrow_mut();
        animations.retain(|id, animation| {
            let Some(handle) = handles.get(id) else {
                return false;
            };
            if handle.offset() != animation.written {
                if *SNAP_DEBUG {
                    eprintln!(
                        "[snap] cancel id={} offset=({:.2},{:.2}) written=({:.2},{:.2})",
                        id,
                        f32::from(handle.offset().x),
                        f32::from(handle.offset().y),
                        f32::from(animation.written.x),
                        f32::from(animation.written.y),
                    );
                }
                return false;
            }
            let started = *animation.started.get_or_insert(now);
            let elapsed = (now - started).as_secs_f64();
            if *SNAP_DEBUG {
                eprintln!(
                    "[snap] step id={} elapsed_ms={} offset=({:.1},{:.1})",
                    id,
                    (now - started).as_millis(),
                    f32::from(handle.offset().x),
                    f32::from(handle.offset().y),
                );
            }
            let t = match animation.curve {
                Curve::Smooth => {
                    let raw = elapsed / SMOOTH_SECONDS;
                    if raw >= 1.0 {
                        handle.set_offset(animation.to);
                        if *SNAP_DEBUG {
                            eprintln!("[snap] land id={}", id);
                        }
                        return false;
                    }
                    ease(raw.max(0.0), &smooth_ease())
                }
                Curve::Fling { frames } => {
                    // The sum of the series up to this frame, over the sum
                    // of the whole series, as in `GetCurrentCurveDistance`.
                    let frame = elapsed / FLING_FRAME_SECONDS + 1.0;
                    if frame >= frames {
                        handle.set_offset(animation.to);
                        if *SNAP_DEBUG {
                            eprintln!("[snap] land id={}", id);
                        }
                        return false;
                    }
                    (1.0 - FLING_RATIO.powf(frame)) / (1.0 - FLING_RATIO.powf(frames))
                }
            };
            let at = point(
                px(mix(f32::from(animation.from.x) as f64, f32::from(animation.to.x) as f64, t) as f32),
                px(mix(f32::from(animation.from.y) as f64, f32::from(animation.to.y) as f64, t) as f32),
            );
            handle.set_offset(at);
            animation.written = at;
            true
        });
        !animations.is_empty()
    })
}

/// Scroll to every new `scroll-initial-target` element. Returns true while
/// one still waits for its first painted bounds.
fn initial_targets(tree: &RetainedTree, handles: &HashMap<u64, ScrollHandle>) -> bool {
    let mut waiting = false;
    INITIAL_DONE.with(|cell| {
        let mut done = cell.borrow_mut();
        for (&id, element) in &tree.elements {
            let declared = element
                .style
                .as_deref()
                .and_then(|style| style.scroll_initial_target.as_deref())
                .is_some_and(|word| word.trim() == "nearest");
            if !declared || done.contains(&id) {
                continue;
            }
            if crate::automation::get_bounds(id).is_none() {
                // The element has not painted yet. Ask for one more frame
                // and read its bounds then.
                waiting = true;
                continue;
            }
            scroll_into_view(
                tree,
                id,
                Align::Start,
                Align::Nearest,
                Behavior::Auto,
                Container::All,
                |id| handles.get(&id).cloned(),
            );
            done.insert(id);
        }
    });
    waiting
}

/// `mandatory` or `proximity`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Strictness {
    Mandatory,
    Proximity,
}

/// A parsed `scroll-snap-type`.
#[derive(Clone, Copy)]
struct SnapType {
    x: bool,
    y: bool,
    strictness: Strictness,
}

fn snap_type(style: Option<&StyleDesc>) -> Option<SnapType> {
    let words = style?.scroll_snap_type.as_deref()?;
    let mut parts = words.split_whitespace();
    let (x, y) = match parts.next()? {
        "x" | "inline" => (true, false),
        "y" | "block" => (false, true),
        "both" => (true, true),
        _ => return None,
    };
    let strictness = match parts.next() {
        Some("mandatory") => Strictness::Mandatory,
        _ => Strictness::Proximity,
    };
    Some(SnapType { x, y, strictness })
}

/// A parsed `scroll-snap-align`, as the block word then the inline word.
fn snap_align(style: Option<&StyleDesc>) -> [Option<Align>; 2] {
    let Some(words) = style.and_then(|style| style.scroll_snap_align.as_deref()) else {
        return [None; 2];
    };
    let word = |text: &str| match text {
        "start" => Some(Align::Start),
        "center" => Some(Align::Center),
        "end" => Some(Align::End),
        _ => None,
    };
    let mut parts = words.split_whitespace();
    let block = parts.next().and_then(word);
    match parts.next() {
        Some(inline) => [block, word(inline)],
        None => [block, block],
    }
}

fn snap_stop_always(style: Option<&StyleDesc>) -> bool {
    style
        .and_then(|style| style.scroll_snap_stop.as_deref())
        .is_some_and(|word| word.trim() == "always")
}

/// Watch every snap container and snap the ones that came to rest.
fn snap_containers(
    tree: &RetainedTree,
    handles: &HashMap<u64, ScrollHandle>,
    now: Instant,
) -> bool {
    let mut active = false;
    SNAP.with(|cell| {
        let mut states = cell.borrow_mut();
        for (&id, element) in &tree.elements {
            let Some(snap) = snap_type(element.style.as_deref()) else {
                continue;
            };
            let Some(handle) = handles.get(&id) else {
                continue;
            };
            let animating = ANIMATIONS.with(|cell| cell.borrow().contains_key(&id));
            let offset = handle.offset();
            let state = states.entry(id).or_insert(SnapState {
                offset,
                moved_at: None,
                from: offset,
            });
            if animating {
                // The glide is ours. Track it without arming the idle timer.
                state.offset = offset;
                state.moved_at = None;
                continue;
            }
            let down = GESTURES.with(|cell| {
                cell.borrow().get(&id).is_some_and(|gesture| gesture.down)
            });
            if down {
                // The fingers are on the pad. The web never snaps during
                // the drag, however long the box rests. The lift picks
                // the target.
                state.offset = offset;
                state.moved_at = None;
                continue;
            }
            if offset != state.offset {
                let step = f32::from(offset.x - state.offset.x)
                    .abs()
                    .max(f32::from(offset.y - state.offset.y).abs());
                if state.moved_at.is_none() {
                    state.from = state.offset;
                    state.moved_at = Some(now);
                } else if step >= 0.5 {
                    state.moved_at = Some(now);
                }
                state.offset = offset;
                if step >= 0.5 {
                    active = true;
                    continue;
                }
                // A step under half a pixel is the momentum tail. Fall
                // through to the idle check, so the snap starts early.
            }
            let Some(moved_at) = state.moved_at else {
                continue;
            };
            if (now - moved_at).as_secs_f64() < IDLE_SECONDS {
                active = true;
                continue;
            }
            state.moved_at = None;
            if let Some(target) = snap_target(tree, id, snap, handle, state.from, offset, handles) {
                if target != offset {
                    animate(id, handle, target);
                    active = true;
                }
            }
        }
    });
    active
}

/// The snap areas of a container: every descendant with a
/// `scroll-snap-align`, without looking inside nested scroll boxes.
fn snap_areas(
    tree: &RetainedTree,
    container: u64,
    handles: &HashMap<u64, ScrollHandle>,
) -> Vec<u64> {
    let mut areas = Vec::new();
    let mut stack: Vec<u64> = tree
        .elements
        .get(&container)
        .map(|element| element.children.clone())
        .unwrap_or_default();
    while let Some(id) = stack.pop() {
        let Some(element) = tree.elements.get(&id) else {
            continue;
        };
        let style = element.style.as_deref();
        if snap_align(style) != [None, None] {
            areas.push(id);
        }
        if handles.contains_key(&id) {
            continue;
        }
        stack.extend(element.children.iter().copied());
    }
    areas
}

/// The rest offset of each snap area on one axis, for a scroll marker
/// group. One offset per area, sorted from the start of the content, so
/// the nth marker stands for the nth stop. Areas that clamp onto the same
/// offset keep their own marker, the way `::scroll-marker` keeps one per
/// element. A container that has not painted yet has no offsets.
pub(crate) fn marker_targets(
    tree: &RetainedTree,
    container: u64,
    handle: &ScrollHandle,
    handles: &HashMap<u64, ScrollHandle>,
    horizontal: bool,
) -> Vec<Pixels> {
    let Some(bounds) = crate::automation::get_bounds(container) else {
        return Vec::new();
    };
    let style = |id: u64| tree.elements.get(&id).and_then(|el| el.style.as_deref());
    let padding = scroll_padding(style(container));
    let (port_start, port_end) = if horizontal {
        (
            bounds.x as f32 + padding[3],
            (bounds.x + bounds.width) as f32 - padding[1],
        )
    } else {
        (
            bounds.y as f32 + padding[0],
            (bounds.y + bounds.height) as f32 - padding[2],
        )
    };
    let offset = handle.offset();
    let max = handle.max_offset();
    let (current, max) = if horizontal {
        (f32::from(offset.x), max.x)
    } else {
        (f32::from(offset.y), max.y)
    };

    let mut targets: Vec<Pixels> = snap_areas(tree, container, handles)
        .into_iter()
        .filter_map(|id| {
            let area = crate::automation::get_bounds(id)?;
            let words = snap_align(style(id));
            let align = if horizontal {
                words[1].or(words[0])
            } else {
                words[0].or(words[1])
            }
            .unwrap_or(Align::Start);
            let margin = scroll_margin(style(id));
            let (start, end) = if horizontal {
                (
                    area.x as f32 - margin[3],
                    (area.x + area.width) as f32 + margin[1],
                )
            } else {
                (
                    area.y as f32 - margin[0],
                    (area.y + area.height) as f32 + margin[2],
                )
            };
            let delta = axis_delta(align, start, end, port_start, port_end);
            Some(px((current - delta).max(-f32::from(max)).min(0.0)))
        })
        .collect();
    targets.sort_by(|a, b| f32::from(*b).total_cmp(&f32::from(*a)));
    targets
}

/// One candidate snap position on one axis.
struct Candidate {
    offset: f32,
    always: bool,
}

/// Where the container should come to rest, or `None` to stay put.
/// The offset a rested container should sit at, or `None` to stay put.
/// `at` is the natural end point of the scroll: the current offset for a
/// box that came to rest, or the predicted landing of a fling. The spec
/// calls this the "intended end position".
fn snap_target(
    tree: &RetainedTree,
    container: u64,
    snap: SnapType,
    handle: &ScrollHandle,
    from: Point<Pixels>,
    at: Point<Pixels>,
    handles: &HashMap<u64, ScrollHandle>,
) -> Option<Point<Pixels>> {
    let bounds = crate::automation::get_bounds(container)?;
    let style = |id: u64| tree.elements.get(&id).and_then(|el| el.style.as_deref());
    let padding = scroll_padding(style(container));
    let port_start = point(bounds.x as f32 + padding[3], bounds.y as f32 + padding[0]);
    let port_end = point(
        (bounds.x + bounds.width) as f32 - padding[1],
        (bounds.y + bounds.height) as f32 - padding[2],
    );

    let offset = handle.offset();
    let max = handle.max_offset();
    let clamp = |value: f32, max: Pixels| value.max(-f32::from(max)).min(0.0);

    let mut on_x: Vec<Candidate> = Vec::new();
    let mut on_y: Vec<Candidate> = Vec::new();
    for id in snap_areas(tree, container, handles) {
        let Some(area) = crate::automation::get_bounds(id) else {
            continue;
        };
        let align = snap_align(style(id));
        let always = snap_stop_always(style(id));
        let margin = scroll_margin(style(id));
        let start = point(area.x as f32 - margin[3], area.y as f32 - margin[0]);
        let end = point(
            (area.x + area.width) as f32 + margin[1],
            (area.y + area.height) as f32 + margin[2],
        );
        if snap.x {
            if let Some(align) = align[1] {
                let delta = axis_delta(align, start.x, end.x, port_start.x, port_end.x);
                on_x.push(Candidate {
                    offset: clamp(f32::from(offset.x) - delta, max.x),
                    always,
                });
            }
        }
        if snap.y {
            if let Some(align) = align[0] {
                let delta = axis_delta(align, start.y, end.y, port_start.y, port_end.y);
                on_y.push(Candidate {
                    offset: clamp(f32::from(offset.y) - delta, max.y),
                    always,
                });
            }
        }
    }

    let x = axis_target(
        &on_x,
        f32::from(at.x),
        f32::from(from.x),
        (port_end.x - port_start.x) / 2.0,
        snap.strictness,
    );
    let y = axis_target(
        &on_y,
        f32::from(at.y),
        f32::from(from.y),
        (port_end.y - port_start.y) / 2.0,
        snap.strictness,
    );
    if x.is_none() && y.is_none() {
        return None;
    }
    Some(point(
        px(x.unwrap_or(f32::from(offset.x))),
        px(y.unwrap_or(f32::from(offset.y))),
    ))
}

/// The resting offset on one axis, or `None` to stay put. `proximity`
/// gives up beyond half a viewport. An `always` candidate between the
/// start of the scroll and the nearest position wins, so a long scroll
/// stops at it.
fn axis_target(
    candidates: &[Candidate],
    current: f32,
    from: f32,
    reach: f32,
    strictness: Strictness,
) -> Option<f32> {
    let nearest = candidates
        .iter()
        .min_by(|a, b| {
            (a.offset - current)
                .abs()
                .total_cmp(&(b.offset - current).abs())
        })?
        .offset;
    if strictness == Strictness::Proximity && (nearest - current).abs() > reach.max(0.0) {
        return None;
    }
    let (low, high) = if from <= nearest { (from, nearest) } else { (nearest, from) };
    let stop = candidates
        .iter()
        .filter(|candidate| {
            candidate.always && candidate.offset > low && candidate.offset < high
        })
        .min_by(|a, b| {
            (a.offset - from)
                .abs()
                .total_cmp(&(b.offset - from).abs())
        });
    Some(stop.map_or(nearest, |candidate| candidate.offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(offset: f32) -> Candidate {
        Candidate {
            offset,
            always: false,
        }
    }

    fn always(offset: f32) -> Candidate {
        Candidate {
            offset,
            always: true,
        }
    }

    #[test]
    fn the_nearest_candidate_wins() {
        let candidates = [candidate(0.0), candidate(-100.0), candidate(-200.0)];
        let target = axis_target(&candidates, -80.0, 0.0, 500.0, Strictness::Mandatory);
        assert_eq!(target, Some(-100.0));
    }

    #[test]
    fn proximity_gives_up_beyond_half_a_viewport() {
        let candidates = [candidate(-300.0)];
        assert_eq!(
            axis_target(&candidates, -100.0, -100.0, 150.0, Strictness::Proximity),
            None
        );
        assert_eq!(
            axis_target(&candidates, -200.0, -200.0, 150.0, Strictness::Proximity),
            Some(-300.0)
        );
    }

    #[test]
    fn an_always_stop_catches_a_long_scroll() {
        let candidates = [candidate(0.0), always(-100.0), candidate(-200.0)];
        // A fling from 0 that would rest near -200 stops at the -100 area.
        let target = axis_target(&candidates, -190.0, 0.0, 500.0, Strictness::Mandatory);
        assert_eq!(target, Some(-100.0));
        // A short move from -90 rests at -100 without a fight, because the
        // stop is not strictly between the start and the target.
        let target = axis_target(&candidates, -110.0, -90.0, 500.0, Strictness::Mandatory);
        assert_eq!(target, Some(-100.0));
    }

    #[test]
    fn snap_type_reads_axis_and_strictness() {
        let style = |text: &str| {
            let mut style = StyleDesc::default();
            style.scroll_snap_type = Some(text.to_string());
            style
        };
        let both = snap_type(Some(&style("both mandatory"))).unwrap();
        assert!(both.x && both.y);
        assert!(both.strictness == Strictness::Mandatory);
        let x = snap_type(Some(&style("x"))).unwrap();
        assert!(x.x && !x.y);
        assert!(x.strictness == Strictness::Proximity);
        let block = snap_type(Some(&style("block proximity"))).unwrap();
        assert!(!block.x && block.y);
        assert!(snap_type(Some(&style("none"))).is_none());
    }

    #[test]
    fn snap_align_reads_one_or_two_words() {
        let style = |text: &str| {
            let mut style = StyleDesc::default();
            style.scroll_snap_align = Some(text.to_string());
            style
        };
        assert_eq!(snap_align(Some(&style("start"))), [Some(Align::Start); 2]);
        assert_eq!(
            snap_align(Some(&style("center end"))),
            [Some(Align::Center), Some(Align::End)]
        );
        assert_eq!(snap_align(Some(&style("none"))), [None, None]);
        assert_eq!(snap_align(None), [None, None]);
    }

    fn tree_with(ids: &[u64]) -> RetainedTree {
        let mut tree = RetainedTree::new();
        for &id in ids {
            tree.elements
                .insert(id, crate::retained_tree::RetainedElement::new(id, "div".to_string(), 0));
        }
        tree
    }

    #[test]
    fn a_deferred_offset_applies_once() {
        defer(701, point(px(0.0), px(-150.0)));
        assert_eq!(take_deferred(701), Some(point(px(0.0), px(-150.0))));
        assert_eq!(take_deferred(701), None);
    }

    #[test]
    fn a_dropped_deferred_offset_does_not_apply() {
        defer(702, point(px(0.0), px(-150.0)));
        drop_deferred(702);
        assert_eq!(take_deferred(702), None);
    }

    #[test]
    fn the_prune_gives_a_missing_element_one_frame_of_grace() {
        // On some backends the scroll command arrives one frame before the
        // batch that creates the element. The first prune with the element
        // missing keeps the offset, and the second one drops it.
        defer(703, point(px(0.0), px(-90.0)));
        prune(&tree_with(&[]));
        assert_eq!(
            DEFERRED.with(|cell| cell.borrow().get(&703).map(|deferred| deferred.to)),
            Some(point(px(0.0), px(-90.0)))
        );
        prune(&tree_with(&[]));
        assert_eq!(take_deferred(703), None);
    }

    #[test]
    fn the_prune_keeps_the_offset_of_a_present_element() {
        defer(704, point(px(0.0), px(-90.0)));
        prune(&tree_with(&[704]));
        prune(&tree_with(&[704]));
        assert_eq!(take_deferred(704), Some(point(px(0.0), px(-90.0))));
    }
}
