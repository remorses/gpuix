//! CSS `scroll-marker-group`.
//!
//! `scroll-marker-group: before | after` on a scroll box adds a group of
//! round markers, one per snap area, along the start or the end edge of
//! the box. The marker of the area nearest the current offset paints
//! stronger. A click on a marker scrolls the box to that area, with a
//! glide when `scroll-behavior: smooth` applies.
//!
//! CSS builds the group from `::scroll-marker` pseudo-elements on the
//! children. GPUIX has no pseudo-elements, so the group makes one marker
//! for each snap area instead.
//!
//! Like the scrollbar, the element takes no layout space and hands its
//! painting to a deferred draw, so the markers paint over the content and
//! take the mouse before it.

use gpui::{
    hsla, point, px, size, App, Bounds, Corners, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent, Pixels, ScrollHandle,
    Style, Window,
};

use crate::style::StyleDesc;

/// The size of one marker.
const DOT: Pixels = px(6.0);
/// The space between two markers.
const GAP: Pixels = px(6.0);
/// The space between the markers and the edge of the box. Wide enough
/// that the markers clear an overlay scrollbar strip on the same edge.
const INSET: Pixels = px(16.0);
/// How far past a marker a click still counts.
const HIT_SLOP: Pixels = px(4.0);

/// Which edge of the box the group sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edge {
    Before,
    After,
}

/// The edge `scroll-marker-group` asks for, or `None` for no group.
pub(crate) fn edge(style: &StyleDesc) -> Option<Edge> {
    match style.scroll_marker_group.as_deref().map(str::trim) {
        Some("before") => Some(Edge::Before),
        Some("after") => Some(Edge::After),
        _ => None,
    }
}

/// The bounds of each marker, centred along the chosen edge.
fn dot_bounds(
    bounds: Bounds<Pixels>,
    count: usize,
    horizontal: bool,
    edge: Edge,
) -> Vec<Bounds<Pixels>> {
    if count == 0 {
        return Vec::new();
    }
    let length = DOT * count as f32 + GAP * count.saturating_sub(1) as f32;
    (0..count)
        .map(|index| {
            let along = (DOT + GAP) * index as f32;
            let origin = if horizontal {
                let left = bounds.left() + (bounds.size.width - length) / 2.0 + along;
                let top = match edge {
                    Edge::Before => bounds.top() + INSET,
                    Edge::After => bounds.bottom() - INSET - DOT,
                };
                point(left, top)
            } else {
                let top = bounds.top() + (bounds.size.height - length) / 2.0 + along;
                let left = match edge {
                    Edge::Before => bounds.left() + INSET,
                    Edge::After => bounds.right() - INSET - DOT,
                };
                point(left, top)
            };
            Bounds::new(origin, size(DOT, DOT))
        })
        .collect()
}

/// The index of the target nearest to `current`.
fn nearest(targets: &[Pixels], current: Pixels) -> Option<usize> {
    targets
        .iter()
        .enumerate()
        .min_by(|a, b| {
            f32::from(*a.1 - current)
                .abs()
                .total_cmp(&f32::from(*b.1 - current).abs())
        })
        .map(|(index, _)| index)
}

/// The element the box adopts as its last child. It takes no layout
/// space and hands the markers to a deferred draw, which paints after
/// the whole tree, over the content.
pub(crate) struct MarkerGroup {
    dots: Option<gpui::AnyElement>,
}

impl MarkerGroup {
    pub(crate) fn new(
        id: u64,
        handle: ScrollHandle,
        targets: Vec<Pixels>,
        horizontal: bool,
        edge: Edge,
        smooth: bool,
    ) -> Self {
        Self {
            dots: Some(
                Dots {
                    id,
                    handle,
                    targets,
                    horizontal,
                    edge,
                    smooth,
                }
                .into_any_element(),
            ),
        }
    }
}

impl Element for MarkerGroup {
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
        (self.dots.as_mut().unwrap().request_layout(window, cx), ())
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
        let dots = self.dots.take().unwrap();
        let mask = window.content_mask();
        window.defer_draw(dots, window.element_offset(), 0, Some(mask));
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

/// The markers themselves: the circles and the click handling.
struct Dots {
    /// The scroll box, the key `scroll_motion::animate` files a glide under.
    id: u64,
    handle: ScrollHandle,
    /// The rest offset of each snap area on the axis, from the start.
    targets: Vec<Pixels>,
    horizontal: bool,
    edge: Edge,
    smooth: bool,
}

impl Element for Dots {
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
        // Out of the flow and empty, like the scrollbar, so the box lays
        // out as if the group were not there.
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
        _window: &mut Window,
        _cx: &mut App,
    ) {
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
        let bounds = self.handle.bounds();
        let dots = dot_bounds(bounds, self.targets.len(), self.horizontal, self.edge);
        if dots.is_empty() {
            return;
        }
        let offset = self.handle.offset();
        let current = nearest(
            &self.targets,
            if self.horizontal { offset.x } else { offset.y },
        );
        for (index, dot) in dots.iter().enumerate() {
            let color = if current == Some(index) {
                hsla(0.0, 0.0, 0.5, 1.0)
            } else {
                hsla(0.0, 0.0, 0.5, 0.4)
            };
            window.paint_quad(gpui::fill(*dot, color).corner_radii(Corners::all(DOT / 2.0)));
        }

        let hits: Vec<(Bounds<Pixels>, Pixels)> = dots
            .into_iter()
            .zip(self.targets.iter().copied())
            .collect();
        let id = self.id;
        let handle = self.handle.clone();
        let horizontal = self.horizontal;
        let smooth = self.smooth;
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if !phase.bubble() || event.button != MouseButton::Left {
                return;
            }
            for (dot, target) in &hits {
                if !dot.dilate(HIT_SLOP).contains(&event.position) {
                    continue;
                }
                let mut to = handle.offset();
                if horizontal {
                    to.x = *target;
                } else {
                    to.y = *target;
                }
                if smooth {
                    super::scroll_motion::animate(id, &handle, to);
                } else {
                    handle.set_offset(to);
                }
                cx.stop_propagation();
                window.refresh();
                return;
            }
        });
    }
}

impl IntoElement for MarkerGroup {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl IntoElement for Dots {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_group_word_picks_an_edge() {
        let style = |word: &str| {
            let mut style = StyleDesc::default();
            style.scroll_marker_group = Some(word.to_string());
            style
        };
        assert_eq!(edge(&style("before")), Some(Edge::Before));
        assert_eq!(edge(&style(" after ")), Some(Edge::After));
        assert_eq!(edge(&style("none")), None);
        assert_eq!(edge(&StyleDesc::default()), None);
    }

    #[test]
    fn dots_centre_along_the_chosen_edge() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(120.0), px(80.0)));
        let dots = dot_bounds(bounds, 3, true, Edge::After);
        // Three dots and two gaps are 30px, centred in 120px.
        assert_eq!(dots[0].origin, point(px(45.0), px(58.0)));
        assert_eq!(dots[2].origin, point(px(69.0), px(58.0)));
        let dots = dot_bounds(bounds, 3, false, Edge::Before);
        assert_eq!(dots[0].origin, point(px(16.0), px(25.0)));
        assert!(dot_bounds(bounds, 0, true, Edge::After).is_empty());
    }

    #[test]
    fn the_nearest_target_marks_the_current_dot() {
        let targets = [px(0.0), px(-100.0), px(-200.0)];
        assert_eq!(nearest(&targets, px(-40.0)), Some(0));
        assert_eq!(nearest(&targets, px(-60.0)), Some(1));
        assert_eq!(nearest(&targets, px(-300.0)), Some(2));
        assert_eq!(nearest(&[], px(0.0)), None);
    }
}
