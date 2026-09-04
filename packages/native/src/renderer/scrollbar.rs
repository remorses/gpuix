//! Scrollbars for scroll boxes.
//!
//! GPUI clips and scrolls a box but paints no bar, so this element paints
//! one. It sits last among the children of the box, takes no layout space,
//! and reads the box's `ScrollHandle` for the viewport, the content size
//! and the offset. The OS picks the kind of bar. Overlay bars float over
//! the content and fade out after a scroll. Classic bars keep a track and
//! reserve a gutter in the layout. `cx.should_auto_hide_scrollbars()`
//! tells the two apart, and `GPUIX_SCROLLBARS=overlay|classic` in the
//! environment overrides it, which keeps tests the same on every machine.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    hsla, point, px, size, Along, App, Axis, BorderStyle, Bounds, Corners, Edges, Element,
    ElementId, GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollHandle, Style, Window,
};

use crate::style::StyleDesc;

/// Overlay bars keep the mouse this long after the last scroll.
const HIDE_DELAY: Duration = Duration::from_secs(1);
/// Then they fade over this long.
const FADE: Duration = Duration::from_millis(400);
/// A thumb never gets shorter than this.
const MIN_THUMB: Pixels = px(20.0);

/// Which kind of bar the OS asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Floats over the content, fades out after a scroll, reserves nothing.
    Overlay,
    /// Always there, with a track, in a gutter reserved in the layout.
    Classic,
}

impl Mode {
    /// The mode for this window, with the environment override on top.
    pub(crate) fn current(cx: &App) -> Self {
        match crate::renderer::env_var("GPUIX_SCROLLBARS").as_deref() {
            Some("overlay") => Mode::Overlay,
            Some("classic") => Mode::Classic,
            _ if cx.should_auto_hide_scrollbars() => Mode::Overlay,
            _ => Mode::Classic,
        }
    }
}

/// CSS `scrollbar-width`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Thickness {
    Auto,
    Thin,
    None,
}

/// CSS `scrollbar-gutter`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gutter {
    Auto,
    Stable,
    StableBothEdges,
}

/// What one scroll box asks of its bars, resolved from its style.
#[derive(Clone, Debug)]
pub(crate) struct Spec {
    mode: Mode,
    thickness: Thickness,
    gutter: Gutter,
    thumb: Option<Hsla>,
    track: Option<Hsla>,
    /// Whether each axis scrolls at all.
    scrolls: Point<bool>,
    /// `overflow: scroll` on the axis, so a classic bar shows even when
    /// the content fits. `auto` shows one only when it overflows.
    always: Point<bool>,
}

/// The overflow words that make a scroll box on an axis.
/// The used per-axis `overflow` words. CSS never mixes `visible` or
/// `clip` on one axis with a scrolling word on the other: when one axis
/// is neither `visible` nor `clip`, `visible` on the other computes to
/// `auto` and `clip` computes to `hidden`.
pub(crate) fn used_overflow<'a>(
    x: Option<&'a str>,
    y: Option<&'a str>,
) -> (Option<&'a str>, Option<&'a str>) {
    let keeps_word = |word: Option<&str>| {
        matches!(word, None | Some("visible") | Some("clip"))
    };
    let coerce = |word: Option<&'a str>, other: Option<&'a str>| {
        if keeps_word(other) {
            return word;
        }
        match word {
            None | Some("visible") => Some("auto"),
            Some("clip") => Some("hidden"),
            word => word,
        }
    };
    (coerce(x, y), coerce(y, x))
}

pub(crate) fn scrolls(word: Option<&str>) -> bool {
    matches!(word, Some("scroll") | Some("auto"))
}

impl Spec {
    /// The spec for a box, or `None` when no axis scrolls.
    pub(crate) fn from_style(style: &StyleDesc, mode: Mode) -> Option<Self> {
        let x = style.overflow_x.as_deref().or(style.overflow.as_deref());
        let y = style.overflow_y.as_deref().or(style.overflow.as_deref());
        let (x, y) = used_overflow(x, y);
        let scrolls = point(scrolls(x), scrolls(y));
        if !scrolls.x && !scrolls.y {
            return None;
        }
        let thickness = match style.scrollbar_width.as_deref().map(str::trim) {
            Some("thin") => Thickness::Thin,
            Some("none") => Thickness::None,
            _ => Thickness::Auto,
        };
        let gutter = match style.scrollbar_gutter.as_deref().map(str::trim) {
            Some("stable") => Gutter::Stable,
            Some("stable both-edges") | Some("both-edges stable") => Gutter::StableBothEdges,
            _ => Gutter::Auto,
        };
        let (thumb, track) = style
            .scrollbar_color
            .as_deref()
            .map(scrollbar_colors)
            .unwrap_or((None, None));
        Some(Self {
            mode,
            thickness,
            gutter,
            thumb,
            track,
            scrolls,
            always: point(x == Some("scroll"), y == Some("scroll")),
        })
    }

    /// The width of a classic gutter, or zero for overlay bars and
    /// `scrollbar-width: none`.
    fn gutter_width(&self) -> Pixels {
        match (self.mode, self.thickness) {
            (Mode::Overlay, _) | (_, Thickness::None) => px(0.0),
            (Mode::Classic, Thickness::Auto) => px(15.0),
            (Mode::Classic, Thickness::Thin) => px(8.0),
        }
    }

    /// The gutter to reserve at the end of each axis this frame, given
    /// which axes overflowed at the last one. `overflow: scroll` and
    /// `scrollbar-gutter: stable` reserve it at all times, `auto` only
    /// while a bar shows. Both are zero for overlay bars, as in CSS.
    pub(crate) fn reserved(&self, overflowed: Point<bool>) -> Point<Pixels> {
        let width = self.gutter_width();
        let reserve = |axis: Axis| {
            let scrolls = self.scrolls.along(axis);
            let stable = self.always.along(axis) || self.gutter != Gutter::Auto;
            if scrolls && (stable || overflowed.along(axis)) {
                width
            } else {
                px(0.0)
            }
        };
        point(reserve(Axis::Horizontal), reserve(Axis::Vertical))
    }

    /// Whether `scrollbar-gutter: stable both-edges` asks for a second
    /// gutter at the start of the axes.
    pub(crate) fn both_edges(&self) -> bool {
        self.gutter == Gutter::StableBothEdges && self.gutter_width() > px(0.0)
    }

    /// The thickness of the thumb, wider while the mouse is on the bar.
    fn thumb_thickness(&self, hovered: bool) -> Pixels {
        match (self.mode, self.thickness, hovered) {
            (_, Thickness::None, _) => px(0.0),
            (Mode::Overlay, Thickness::Auto, false) => px(7.0),
            (Mode::Overlay, Thickness::Auto, true) => px(11.0),
            (Mode::Overlay, Thickness::Thin, false) => px(4.0),
            (Mode::Overlay, Thickness::Thin, true) => px(6.0),
            (Mode::Classic, Thickness::Auto, _) => px(9.0),
            (Mode::Classic, Thickness::Thin, _) => px(6.0),
        }
    }

    /// The strip along one edge that a bar lives in. For classic bars it
    /// is the gutter. For overlay bars it is the widest the thumb gets plus
    /// its inset from the edge.
    fn strip_thickness(&self) -> Pixels {
        match self.mode {
            Mode::Classic => self.gutter_width(),
            Mode::Overlay => self.thumb_thickness(true) + px(2.0) * 2.0,
        }
    }

    fn thumb_color(&self, state: ThumbLook) -> Hsla {
        let base = self.thumb.unwrap_or(hsla(0.0, 0.0, 0.5, 0.55));
        let alpha = match state {
            ThumbLook::Rest => base.a,
            ThumbLook::Hovered => (base.a * 1.3).min(1.0),
            ThumbLook::Dragged => (base.a * 1.5).min(1.0),
        };
        Hsla { a: alpha, ..base }
    }

    fn track_color(&self) -> Hsla {
        self.track.unwrap_or(hsla(0.0, 0.0, 0.5, 0.12))
    }
}

#[derive(Clone, Copy)]
enum ThumbLook {
    Rest,
    Hovered,
    Dragged,
}

/// `scrollbar-color: <thumb> <track>`, or `auto`.
///
/// CSS takes `auto` or exactly two colours. One colour, three words or a
/// word that is not a colour drops the whole declaration, the way a browser
/// drops a value it cannot parse.
fn scrollbar_colors(value: &str) -> (Option<Hsla>, Option<Hsla>) {
    let words = split_top_level(value);
    if words.len() != 2 {
        return (None, None);
    }
    let color = |index: usize| {
        words
            .get(index)
            .and_then(|word| crate::color::parse_color_rgba(word))
            .map(Hsla::from)
    };
    match (color(0), color(1)) {
        (Some(thumb), Some(track)) => (Some(thumb), Some(track)),
        _ => (None, None),
    }
}

/// Splits on spaces outside parentheses, so `rgb(0 0 0 / 0.5) white` is
/// two words.
fn split_top_level(value: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    for (index, ch) in value.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if ch.is_whitespace() && depth == 0 {
            if let Some(from) = start.take() {
                words.push(&value[from..index]);
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(from) = start {
        words.push(&value[from..]);
    }
    words
}

/// What a bar remembers between frames.
#[derive(Default)]
pub(crate) struct State {
    /// The offset at the last frame, to notice a scroll.
    last_offset: Option<Point<Pixels>>,
    /// When the box last scrolled. Overlay bars show for a while after.
    last_scroll: Option<Instant>,
    /// Which axes had more content than room at the last frame.
    pub(crate) overflowed: Point<bool>,
    /// The axis whose strip the mouse is on.
    hovered: Option<Axis>,
    /// A drag of the thumb: the axis and where the mouse took hold of
    /// the thumb, from its start.
    drag: Option<(Axis, Pixels)>,
}

/// The shared states, one per scroll box, kept on the view.
pub(crate) type States = HashMap<u64, Rc<RefCell<State>>>;

/// Where a bar's parts are this frame.
#[derive(Clone, Copy)]
struct Geometry {
    axis: Axis,
    /// The strip along the edge that takes the mouse.
    strip: Bounds<Pixels>,
    /// The part of the strip a thumb can move in.
    track: Bounds<Pixels>,
    /// The thumb, or `None` when the content fits.
    thumb: Option<Bounds<Pixels>>,
}

impl Geometry {
    /// The offset for a thumb start at `along` in the track.
    fn offset_for_thumb_start(&self, along: Pixels, max_offset: Pixels) -> Pixels {
        let Some(thumb) = self.thumb else {
            return px(0.0);
        };
        let room = self.track.size.along(self.axis) - thumb.size.along(self.axis);
        if room <= px(0.0) {
            return px(0.0);
        }
        let fraction = (f32::from(along - self.track.origin.along(self.axis)) / f32::from(room))
            .clamp(0.0, 1.0);
        -max_offset * fraction
    }
}

/// The element the box adopts as its last child. It takes no layout
/// space and hands the bar to a deferred draw, which paints after the
/// whole tree. A sibling that overlaps the box, such as a blurred
/// header, then cannot cover or blur the bar. The deferred draw keeps
/// the content mask of the box, so an ancestor still clips the bar.
pub(crate) struct Scrollbar {
    bar: Option<gpui::AnyElement>,
}

impl Scrollbar {
    pub(crate) fn new(
        spec: Spec,
        handle: ScrollHandle,
        state: Rc<RefCell<State>>,
        now: Instant,
    ) -> Self {
        Self {
            bar: Some(
                Bar {
                    spec,
                    handle,
                    state,
                    now,
                }
                .into_any_element(),
            ),
        }
    }
}

impl Element for Scrollbar {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.bar.as_mut().unwrap().request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        _cx: &mut App,
    ) {
        let bar = self.bar.take().unwrap();
        let mask = window.content_mask();
        window.defer_draw(bar, window.element_offset(), 0, Some(mask));
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }
}

/// The bar itself: the strips, the thumbs and the mouse handling.
struct Bar {
    spec: Spec,
    handle: ScrollHandle,
    state: Rc<RefCell<State>>,
    now: Instant,
}

impl Bar {

    /// How much of an overlay bar shows, from 0 to 1. Classic bars are
    /// always 1.
    fn opacity(&self, state: &State) -> f32 {
        if self.spec.mode == Mode::Classic || state.drag.is_some() || state.hovered.is_some() {
            return 1.0;
        }
        let Some(last) = state.last_scroll else {
            return 0.0;
        };
        let since = self.now.saturating_duration_since(last);
        if since < HIDE_DELAY {
            1.0
        } else if since < HIDE_DELAY + FADE {
            1.0 - (since - HIDE_DELAY).as_secs_f32() / FADE.as_secs_f32()
        } else {
            0.0
        }
    }

    /// Whether the bar on `axis` takes any part in this frame.
    fn shows(&self, axis: Axis, overflowed: Point<bool>) -> bool {
        if !self.spec.scrolls.along(axis) || self.spec.thickness == Thickness::None {
            return false;
        }
        match self.spec.mode {
            Mode::Classic => overflowed.along(axis) || self.spec.always.along(axis),
            Mode::Overlay => overflowed.along(axis),
        }
    }

    /// The parts of the bar on `axis`, given whether the other axis also
    /// shows one and takes the corner.
    fn geometry(&self, axis: Axis, other_shows: bool, hovered: bool) -> Geometry {
        let bounds = self.handle.bounds();
        let offset = self.handle.offset();
        let max_offset = self.handle.max_offset();
        let strip_thickness = self.spec.strip_thickness();
        let thumb_thickness = self.spec.thumb_thickness(hovered);
        // Where the thumb sits across the axis. Classic thumbs centre in
        // the gutter. Overlay thumbs keep a 2px inset from the edge and
        // grow inward when hovered.
        let inset = match self.spec.mode {
            Mode::Classic => (strip_thickness - thumb_thickness) / 2.0,
            Mode::Overlay => px(2.0),
        };
        let corner = if other_shows {
            strip_thickness
        } else {
            px(0.0)
        };
        let end_inset = match self.spec.mode {
            Mode::Classic => px(0.0),
            Mode::Overlay => px(2.0),
        };
        let (strip, track) = match axis {
            Axis::Vertical => {
                let strip = Bounds::new(
                    point(bounds.right() - strip_thickness, bounds.top()),
                    size(strip_thickness, bounds.size.height - corner),
                );
                let track = Bounds::new(
                    point(
                        bounds.right() - inset - thumb_thickness,
                        strip.top() + end_inset,
                    ),
                    size(thumb_thickness, strip.size.height - end_inset * 2.0),
                );
                (strip, track)
            }
            Axis::Horizontal => {
                let strip = Bounds::new(
                    point(bounds.left(), bounds.bottom() - strip_thickness),
                    size(bounds.size.width - corner, strip_thickness),
                );
                let track = Bounds::new(
                    point(
                        strip.left() + end_inset,
                        bounds.bottom() - inset - thumb_thickness,
                    ),
                    size(strip.size.width - end_inset * 2.0, thumb_thickness),
                );
                (strip, track)
            }
        };
        let max = max_offset.along(axis);
        let thumb = (max > px(0.0)).then(|| {
            let viewport = bounds.size.along(axis);
            let content = viewport + max;
            let track_len = track.size.along(axis);
            let len = (track_len * (f32::from(viewport) / f32::from(content)))
                .max(MIN_THUMB)
                .min(track_len);
            let scrolled = (f32::from(-offset.along(axis)) / f32::from(max)).clamp(0.0, 1.0);
            let start = (track_len - len) * scrolled;
            Bounds::new(
                track.origin.apply_along(axis, |origin| origin + start),
                track.size.apply_along(axis, |_| len),
            )
        });
        Geometry {
            axis,
            strip,
            track,
            thumb,
        }
    }
}

/// The mouse listeners for one frame. They hold the frame's geometry, so
/// a hit test is a `contains` on the bounds. The scroll offset they set
/// is clamped by the box at its next prepaint.
fn register_mouse(
    bars: Vec<Geometry>,
    handle: ScrollHandle,
    state: Rc<RefCell<State>>,
    window: &mut Window,
) {
    let viewport = handle.bounds();
    let strips: Vec<(Axis, Bounds<Pixels>)> = bars.iter().map(|g| (g.axis, g.strip)).collect();

    // Take hold of a thumb, or jump a page on a click in the track.
    let down_bars = bars.clone();
    let down_handle = handle.clone();
    let down_state = state.clone();
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if !phase.bubble() || event.button != MouseButton::Left {
            return;
        }
        for bar in &down_bars {
            if !bar.strip.contains(&event.position) {
                continue;
            }
            let along = event.position.along(bar.axis);
            let offset = down_handle.offset();
            if let Some(thumb) = bar.thumb.filter(|thumb| thumb.contains(&event.position)) {
                down_state.borrow_mut().drag =
                    Some((bar.axis, along - thumb.origin.along(bar.axis)));
            } else if let Some(thumb) = bar.thumb {
                let page = viewport.size.along(bar.axis) * 0.9;
                let max = down_handle.max_offset().along(bar.axis);
                let current = offset.along(bar.axis);
                let next = if along < thumb.origin.along(bar.axis) {
                    current + page
                } else {
                    current - page
                };
                let offset = offset.apply_along(bar.axis, |_| next.clamp(-max, px(0.0)));
                down_handle.set_offset(offset);
            }
            cx.stop_propagation();
            window.refresh();
            return;
        }
    });

    // Move the thumb, and notice the mouse coming onto or leaving a strip.
    let move_bars = bars;
    let move_strips = strips.clone();
    let move_handle = handle.clone();
    let move_state = state.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _cx| {
        if !phase.bubble() {
            return;
        }
        let drag = move_state.borrow().drag;
        if let Some((axis, grab)) = drag {
            if let Some(bar) = move_bars.iter().find(|bar| bar.axis == axis) {
                let start = event.position.along(axis) - grab;
                let max = move_handle.max_offset().along(axis);
                let offset = move_handle
                    .offset()
                    .apply_along(axis, |_| bar.offset_for_thumb_start(start, max));
                move_handle.set_offset(offset);
                window.refresh();
            }
            return;
        }
        let hovered = move_strips
            .iter()
            .find(|(_, strip)| strip.contains(&event.position))
            .map(|(axis, _)| *axis);
        let mut state = move_state.borrow_mut();
        if state.hovered != hovered {
            state.hovered = hovered;
            window.refresh();
        }
    });

    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, _cx| {
        if phase.bubble() && event.button == MouseButton::Left {
            let mut state = state.borrow_mut();
            if state.drag.take().is_some() {
                state.hovered = strips
                    .iter()
                    .find(|(_, strip)| strip.contains(&event.position))
                    .map(|(axis, _)| *axis);
                window.refresh();
            }
        }
    });
}

impl Element for Bar {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        // Out of the flow and empty, so the box lays out as if the bar
        // were not there. The bar paints from the handle's bounds instead.
        let mut style = Style::default();
        style.position = gpui::Position::Absolute;
        style.inset.top = px(0.0).into();
        style.inset.left = px(0.0).into();
        style.size = size(px(0.0).into(), px(0.0).into());
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        _cx: &mut App,
    ) {
        // The box set its bounds and max offset on the handle just before
        // its children prepaint, so they are this frame's.
        let offset = self.handle.offset();
        let max_offset = self.handle.max_offset();
        let mut state = self.state.borrow_mut();
        if state.last_offset.is_some_and(|last| last != offset) {
            state.last_scroll = Some(self.now);
        }
        state.last_offset = Some(offset);
        state.overflowed = point(max_offset.x > px(0.0), max_offset.y > px(0.0));
        let opacity = self.opacity(&state);
        if self.spec.mode == Mode::Overlay && opacity > 0.0 && opacity < 1.0 {
            window.request_animation_frame();
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        _cx: &mut App,
    ) {
        let state = self.state.borrow();
        let opacity = self.opacity(&state);
        if opacity <= 0.0 {
            return;
        }
        let overflowed = state.overflowed;
        let shows = point(
            self.shows(Axis::Horizontal, overflowed),
            self.shows(Axis::Vertical, overflowed),
        );
        let mut bars = Vec::new();
        for axis in [Axis::Vertical, Axis::Horizontal] {
            if !shows.along(axis) {
                continue;
            }
            let other = match axis {
                Axis::Vertical => Axis::Horizontal,
                Axis::Horizontal => Axis::Vertical,
            };
            let hovered =
                state.hovered == Some(axis) || matches!(state.drag, Some((a, _)) if a == axis);
            let bar = self.geometry(axis, shows.along(other), hovered);

            // The track. Classic bars always have one. Overlay bars show
            // one while the mouse is on the strip, like macOS.
            let track_shows = self.spec.mode == Mode::Classic || hovered;
            if track_shows {
                let mut color = self.spec.track_color();
                color.a *= opacity;
                let (radius, border) = match self.spec.mode {
                    Mode::Classic => (px(0.0), px(1.0)),
                    Mode::Overlay => (bar.strip.size.along(other) / 2.0, px(0.0)),
                };
                let mut border_widths = Edges::default();
                match axis {
                    Axis::Vertical => border_widths.left = border,
                    Axis::Horizontal => border_widths.top = border,
                }
                let mut border_color = color;
                border_color.a = (border_color.a * 1.5).min(1.0);
                window.paint_quad(gpui::quad(
                    bar.strip,
                    Corners::all(radius),
                    color,
                    border_widths,
                    border_color,
                    BorderStyle::default(),
                ));
            }

            if let Some(thumb) = bar.thumb {
                let look = match state.drag {
                    Some((a, _)) if a == axis => ThumbLook::Dragged,
                    _ if hovered => ThumbLook::Hovered,
                    _ => ThumbLook::Rest,
                };
                let mut color = self.spec.thumb_color(look);
                color.a *= opacity;
                let radius = thumb.size.along(other) / 2.0;
                window.paint_quad(gpui::fill(thumb, color).corner_radii(Corners::all(radius)));
            }
            bars.push(bar);
        }
        // The square where two classic bars meet, in the track colour,
        // as a browser paints it.
        if self.spec.mode == Mode::Classic && shows.x && shows.y {
            let bounds = self.handle.bounds();
            let thickness = self.spec.strip_thickness();
            let corner = Bounds::new(
                point(bounds.right() - thickness, bounds.bottom() - thickness),
                size(thickness, thickness),
            );
            window.paint_quad(gpui::fill(corner, self.spec.track_color()));
        }
        drop(state);
        if !bars.is_empty() {
            register_mouse(bars, self.handle.clone(), self.state.clone(), window);
        }
    }
}

impl IntoElement for Scrollbar {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl IntoElement for Bar {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_split_outside_parentheses() {
        let words = split_top_level("rgb(0 0 0 / 0.5)  white");
        assert_eq!(words, vec!["rgb(0 0 0 / 0.5)", "white"]);
        let (thumb, track) = scrollbar_colors("rgb(0 0 0 / 0.5) white");
        assert!(thumb.is_some_and(|c| (c.a - 0.5).abs() < 0.01));
        assert!(track.is_some_and(|c| c.l > 0.99));
        assert_eq!(scrollbar_colors("auto"), (None, None));
    }

    #[test]
    fn scrollbar_color_takes_auto_or_exactly_two_colors() {
        // One colour is not valid CSS, so the declaration drops.
        assert_eq!(scrollbar_colors("red"), (None, None));
        assert_eq!(scrollbar_colors("red white blue"), (None, None));
        // A word that is not a colour drops both, not just itself.
        assert_eq!(scrollbar_colors("red nonsense"), (None, None));
    }

    fn spec(overflow: &str, extra: impl FnOnce(&mut StyleDesc), mode: Mode) -> Spec {
        let mut style = StyleDesc::default();
        style.overflow = Some(overflow.to_string());
        extra(&mut style);
        Spec::from_style(&style, mode).expect("a scroll box")
    }

    #[test]
    fn classic_gutter_follows_overflow_and_gutter_words() {
        let no = point(false, false);
        let yes = point(true, true);
        assert_eq!(
            spec("scroll", |_| {}, Mode::Classic).reserved(no),
            point(px(15.0), px(15.0))
        );
        assert_eq!(
            spec("auto", |_| {}, Mode::Classic).reserved(no),
            point(px(0.0), px(0.0))
        );
        assert_eq!(
            spec("auto", |_| {}, Mode::Classic).reserved(yes),
            point(px(15.0), px(15.0))
        );
        let stable = spec(
            "auto",
            |s| s.scrollbar_gutter = Some("stable".into()),
            Mode::Classic,
        );
        assert_eq!(stable.reserved(no), point(px(15.0), px(15.0)));
        let thin = spec(
            "scroll",
            |s| s.scrollbar_width = Some("thin".into()),
            Mode::Classic,
        );
        assert_eq!(thin.reserved(no), point(px(8.0), px(8.0)));
        let none = spec(
            "scroll",
            |s| s.scrollbar_width = Some("none".into()),
            Mode::Classic,
        );
        assert_eq!(none.reserved(yes), point(px(0.0), px(0.0)));
        assert_eq!(
            spec("scroll", |_| {}, Mode::Overlay).reserved(yes),
            point(px(0.0), px(0.0))
        );
    }

    #[test]
    fn a_hidden_axis_has_no_bar() {
        let mut style = StyleDesc::default();
        style.overflow_y = Some("scroll".into());
        style.overflow_x = Some("hidden".into());
        let spec = Spec::from_style(&style, Mode::Classic).unwrap();
        assert_eq!(spec.reserved(point(true, true)), point(px(0.0), px(15.0)));
        style.overflow_y = Some("clip".into());
        assert!(Spec::from_style(&style, Mode::Classic).is_none());
    }
}
