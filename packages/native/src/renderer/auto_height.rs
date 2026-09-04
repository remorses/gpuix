//! Animating a `height` toward the height the content takes.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, IsolatedLayout, LayoutId, Pixels, Size, Style, Window, px,
    size,
};

use crate::motion::{MotionFrame, MotionHeight};
use crate::style::resolve::Resolved;

/// `built` inside the element that measures it, when its `height` animates
/// with `auto` at an end. Otherwise `built` as it was.
///
/// The inner element has to declare no height of its own for the measurement
/// to see the content. `apply_motion` writes `auto` on it for that reason.
///
/// A pixel width goes on the wrapper so taffy resolves the box straight to it.
/// Any other width reaches the measurement through taffy instead.
pub(super) fn wrap(
    id: u64,
    built: AnyElement,
    motion: Option<&MotionFrame>,
    resolved: Option<&Resolved>,
) -> AnyElement {
    let Some((frame, height)) = motion.and_then(|frame| Some((frame, frame.measured_height()?)))
    else {
        return built;
    };
    let width = frame
        .style
        .width
        .map(|value| px(value as f32))
        .or_else(|| absolute_pixels(resolved?.base.size.width));
    AutoHeight::new(id, built, frame.clone(), height, width).into_any_element()
}

/// The pixels a resolved length is, or `None` when it is a share or `auto`.
fn absolute_pixels(length: Option<gpui::Length>) -> Option<Pixels> {
    match length? {
        gpui::Length::Definite(gpui::DefiniteLength::Absolute(gpui::AbsoluteLength::Pixels(
            pixels,
        ))) => Some(pixels),
        _ => None,
    }
}

/// The content, and the layout tree it measures in.
///
/// The measure closure and the element phases both reach this, so it is shared.
struct Content {
    element: AnyElement,
    /// The content is laid out here rather than in the window's tree, because
    /// the measurement runs while the window's tree computes.
    layout: IsolatedLayout,
}

/// One element whose `height` animates with `auto` at an end of it.
///
/// `auto` is the height the content takes, and only layout knows that number.
/// This asks taffy for a measured box, and taffy calls back with the width the
/// parent gives it. The content is measured at that width, the interpolation
/// resolves against the measurement, and the measured box reports the result as
/// its height.
///
/// Taking the width from taffy is what makes this exact for a width that comes
/// from `flex`, from a percentage, or from a stretched cross axis. Text wraps at
/// the width it will really have.
///
/// The content keeps the height it measured, so the box clips while the animated
/// height is shorter than it. That is the `overflow: hidden` the web asks for on
/// a box whose height animates.
struct AutoHeight {
    id: u64,
    content: Rc<RefCell<Content>>,
    frame: MotionFrame,
    height: MotionHeight,
    width: Option<Pixels>,
}

impl AutoHeight {
    fn new(
        id: u64,
        element: AnyElement,
        frame: MotionFrame,
        height: MotionHeight,
        width: Option<Pixels>,
    ) -> Self {
        Self {
            id,
            content: Rc::new(RefCell::new(Content {
                element,
                layout: IsolatedLayout::new(),
            })),
            frame,
            height,
            width,
        }
    }

    /// Run `f` with the content and the tree it lives in.
    fn with_content<R>(
        &self,
        window: &mut Window,
        f: impl FnOnce(&mut AnyElement, &mut Window) -> R,
    ) -> R {
        let content = &mut *self.content.borrow_mut();
        let element = &mut content.element;
        content.layout.enter(window, |window| f(element, window))
    }
}

impl Element for AutoHeight {
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
        _cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        if let Some(width) = self.width {
            style.size.width = width.into();
        }

        let content = self.content.clone();
        let frame = self.frame.clone();
        let height = self.height;
        let layout_id = window.request_measured_layout(
            style,
            move |known: Size<Option<Pixels>>, available: Size<AvailableSpace>, window, cx| {
                // Taffy asks more than once, with a known width on the pass that
                // has resolved one. That pass is the answer that counts, and the
                // earlier ones are the intrinsic widths this box reports.
                let width = known
                    .width
                    .map_or(available.width, AvailableSpace::Definite);

                let content = &mut *content.borrow_mut();
                let element = &mut content.element;
                let measured = content
                    .layout
                    .enter(window, |window| {
                        element.layout_as_root(size(width, AvailableSpace::MaxContent), window, cx)
                    });
                let measured_height = f32::from(measured.height) as f64;

                // The intrinsic passes measure at a width the box will not get,
                // so only a pass with a real width reports.
                if matches!(width, AvailableSpace::Definite(_)) {
                    frame.measured.report(measured_height);
                }

                // While the animation runs, this frame resolves against the
                // height the state already knows, so content that changed just
                // now paints where the last frame did. The state takes the new
                // measurement in at the next frame and bends the curve there.
                // At rest, `auto` is the content and follows it at once.
                let content_height = match frame.content {
                    Some(known) if frame.active => known,
                    _ => measured_height,
                };

                size(measured.width, px(height.resolve(content_height) as f32))
            },
        );
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.with_content(window, |element, window| {
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                element.layout_as_root(
                    size(
                        AvailableSpace::Definite(bounds.size.width),
                        AvailableSpace::MaxContent,
                    ),
                    window,
                    cx,
                );
                element.prepaint_at(bounds.origin, window, cx);
            });
        });
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.with_content(window, |element, window| {
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                element.paint(window, cx);
            });
        });
        // The content painted its own tracker at the height it measured. The box
        // on screen is this one, so it records last and wins.
        crate::automation::record_bounds(self.id, bounds);
    }
}

impl IntoElement for AutoHeight {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}
