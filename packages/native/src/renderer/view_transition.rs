//! View transitions: freeze the named elements, swap the tree, then animate
//! each name from its old place to its new one.
//!
//! `viewTransitionCapture` clones the subtree and the painted bounds of every
//! element that has a `viewTransitionName`. `viewTransitionStart` parses the
//! options and starts the clock. While the transition runs, `build_element`
//! wraps each named live element in a `VtGroup`. The group takes the layout of
//! the live element, so the transition never disturbs the surrounding layout.
//! It paints the frozen copy at its captured place, then paints the live
//! element moved by this frame's offset. Opacity for the live element rides
//! the same style channel that `motion` uses, and the frozen copy carries its
//! opacity on a wrapper element.
//!
//! A name that disappears without a successor becomes an exit copy: the
//! renderer paints its frozen copy over the whole tree at its captured
//! place, and the group's `old` side drives it.
//!
//! Known limits, on purpose:
//! - An exit copy paints over the tree, so a former ancestor's clip or
//!   scroll no longer applies to it. Give both screens one name when the
//!   exit must stay inside the element's own area.
//! - When the named element survives the swap, its frozen copy takes fresh
//!   ids, so the copy paints without the old scroll offsets.
//! - The frozen copy keeps its event listeners, but their elements are gone
//!   on the React side, so input over the copy does nothing.

use std::collections::{HashMap, HashSet};

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, IsolatedLayout, LayoutId, Pixels, Point, Size, Window, point,
    px, size,
};
use serde::Deserialize;
use web_time::Instant;

use super::frame::{build_element, BuildCtx};
use crate::motion::{self, MotionEase};
use crate::retained_tree::{RetainedElement, RetainedTree};

// ── Options ──────────────────────────────────────────────────────────

/// A translation distance: pixels, or a share of the element's size.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(try_from = "LenWire")]
pub(crate) enum VtLen {
    Px(f64),
    Percent(f64),
}

impl VtLen {
    fn resolve(self, extent: f64) -> f64 {
        match self {
            Self::Px(value) => value,
            Self::Percent(value) => value / 100.0 * extent,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LenWire {
    Number(f64),
    Text(String),
}

impl TryFrom<LenWire> for VtLen {
    type Error = String;

    fn try_from(wire: LenWire) -> Result<Self, String> {
        let text = match wire {
            LenWire::Number(value) if value.is_finite() => return Ok(Self::Px(value)),
            LenWire::Number(value) => {
                return Err(format!("view transition length must be finite, got {value}"))
            }
            LenWire::Text(text) => text,
        };
        let trimmed = text.trim();
        let (number, percent) = match trimmed.strip_suffix('%') {
            Some(number) => (number, true),
            None => (trimmed.strip_suffix("px").unwrap_or(trimmed), false),
        };
        let value = number
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("bad view transition length: {text:?}"))?;
        Ok(if percent {
            Self::Percent(value)
        } else {
            Self::Px(value)
        })
    }
}

/// What one side of a pair does over the transition. Every field is a
/// `[from, to]` pair. A missing field holds still.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SideSpec {
    translate_x: Option<[VtLen; 2]>,
    translate_y: Option<[VtLen; 2]>,
    opacity: Option<[f64; 2]>,
    /// A `filter: blur()` sigma in pixels, as a `[from, to]` pair.
    blur: Option<[f64; 2]>,
    /// Paint this side over the other one. Only read on the old side.
    on_top: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct GroupSpec {
    duration: Option<f64>,
    delay: Option<f64>,
    ease: Option<MotionEase>,
    old: Option<SideSpec>,
    new: Option<SideSpec>,
}

/// The whole options payload of one `viewTransitionStart` call.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct VtOptions {
    /// Seconds, like the `motion` prop.
    duration: Option<f64>,
    delay: Option<f64>,
    ease: Option<MotionEase>,
    groups: HashMap<String, GroupSpec>,
}

const DEFAULT_DURATION: f64 = 0.3;

impl VtOptions {
    pub(crate) fn parse(json: &str) -> Result<Self, String> {
        let options: Self = serde_json::from_str(json).map_err(|error| error.to_string())?;
        for (ease, duration, delay) in std::iter::once((&options.ease, options.duration, options.delay))
            .chain(
                options
                    .groups
                    .values()
                    .map(|group| (&group.ease, group.duration, group.delay)),
            )
        {
            if let Some(ease) = ease {
                motion::validate_ease(ease)?;
            }
            for (name, value) in [("duration", duration), ("delay", delay)] {
                if let Some(value) = value {
                    if !value.is_finite() || value < 0.0 {
                        return Err(format!(
                            "view transition {name} must be a finite non-negative number"
                        ));
                    }
                }
            }
        }
        Ok(options)
    }

    fn timing(&self, name: &str) -> (f64, f64, MotionEase) {
        let group = self.groups.get(name);
        let duration = group
            .and_then(|group| group.duration)
            .or(self.duration)
            .unwrap_or(DEFAULT_DURATION);
        let delay = group.and_then(|group| group.delay).or(self.delay).unwrap_or(0.0);
        let ease = group
            .and_then(|group| group.ease.clone())
            .or_else(|| self.ease.clone())
            .unwrap_or(MotionEase::Name("easeInOut".to_string()));
        (duration, delay, ease)
    }

    /// When the last group comes to rest, in seconds from the start.
    fn longest_end(&self) -> f64 {
        let default_end =
            self.delay.unwrap_or(0.0) + self.duration.unwrap_or(DEFAULT_DURATION);
        self.groups
            .keys()
            .map(|name| {
                let (duration, delay, _) = self.timing(name);
                delay + duration
            })
            .fold(default_end, f64::max)
    }
}

// ── Capture ──────────────────────────────────────────────────────────

/// One frozen named element: its cloned subtree and its painted place.
pub(crate) struct VtCapture {
    pub(crate) tree: RetainedTree,
    pub(crate) root: u64,
    pub(crate) origin: Point<Pixels>,
    pub(crate) size: Size<Pixels>,
}

/// Clone every named element's subtree, with the bounds it painted at.
/// An element that never painted has no place to animate from, so it is
/// skipped and its name enters as a new element.
pub(crate) fn capture(tree: &RetainedTree) -> HashMap<String, VtCapture> {
    let bounds = crate::automation::all_bounds();
    let mut captures = HashMap::new();
    for (&id, element) in &tree.elements {
        let Some(name) = element
            .style
            .as_deref()
            .and_then(|style| style.view_transition_name.as_deref())
        else {
            continue;
        };
        if name.is_empty() || name == "none" {
            continue;
        }
        let Some(rect) = bounds.get(&id) else {
            continue;
        };
        let mut frozen = RetainedTree::new();
        clone_subtree(tree, id, None, &mut frozen);
        frozen.root_id = Some(id);
        captures.insert(
            name.to_string(),
            VtCapture {
                tree: frozen,
                root: id,
                origin: point(px(rect.x as f32), px(rect.y as f32)),
                size: size(px(rect.width as f32), px(rect.height as f32)),
            },
        );
    }
    captures
}

fn clone_subtree(source: &RetainedTree, id: u64, parent: Option<u64>, into: &mut RetainedTree) {
    let Some(element) = source.elements.get(&id) else {
        return;
    };
    let mut clone = RetainedElement::new(id, element.element_type.clone(), element.subtree_revision);
    clone.style = element.style.clone();
    clone.content = element.content.clone();
    clone.events = element.events.clone();
    clone.children = element.children.clone();
    clone.parent = parent;
    clone.custom_props = element.custom_props.clone();
    // The copy is a still image. Without this, a fresh `MotionState` would
    // replay the initial-to-animate run inside it.
    clone.custom_props.remove("motion");
    // Locators must find the live element, never the copy.
    clone.test_id = None;
    clone.search_revision = element.search_revision;
    let children = clone.children.clone();
    into.elements.insert(id, clone);
    for child in children {
        clone_subtree(source, child, Some(id), into);
    }
}

/// Fresh ids for clones whose original survives the swap. Far above what the
/// JS counter reaches, so the two ranges never meet.
const REMAP_BASE: u64 = 1 << 62;

/// Give a clone a fresh id when its original is still in the live tree.
/// Building both under one id would hand them one GPUI element state.
/// A clone of a destroyed element keeps its id, and with it its scroll
/// offsets, which is the common pair case.
fn remap_live_ids(captures: &mut HashMap<String, VtCapture>, live: &RetainedTree, next: &mut u64) {
    for capture in captures.values_mut() {
        let colliding: Vec<u64> = capture
            .tree
            .elements
            .keys()
            .copied()
            .filter(|id| live.elements.contains_key(id))
            .collect();
        for from in colliding {
            let to = *next;
            *next += 1;
            remap(&mut capture.tree, from, to);
            if capture.root == from {
                capture.root = to;
            }
        }
    }
}

fn remap(tree: &mut RetainedTree, from: u64, to: u64) {
    let Some(mut element) = tree.elements.remove(&from) else {
        return;
    };
    element.id = to;
    let parent = element.parent;
    let children = element.children.clone();
    tree.elements.insert(to, element);
    if let Some(parent) = parent.and_then(|id| tree.elements.get_mut(&id)) {
        for child in &mut parent.children {
            if *child == from {
                *child = to;
            }
        }
    }
    for child in children {
        if let Some(child) = tree.elements.get_mut(&child) {
            child.parent = Some(to);
        }
    }
    if tree.root_id == Some(from) {
        tree.root_id = Some(to);
    }
}

// ── State ────────────────────────────────────────────────────────────

/// One running transition. The view holds at most one. A new start replaces
/// the one before it.
pub(crate) struct VtState {
    captures: HashMap<String, VtCapture>,
    options: VtOptions,
    started: Option<Instant>,
    frame_now: Option<Instant>,
    /// Every id inside a frozen tree. The view keeps the scroll handles and
    /// custom element instances of these ids alive while the transition runs.
    ids: HashSet<u64>,
}

impl VtState {
    pub(crate) fn new(
        mut captures: HashMap<String, VtCapture>,
        options: VtOptions,
        live: &RetainedTree,
    ) -> Self {
        let mut next = REMAP_BASE;
        remap_live_ids(&mut captures, live, &mut next);
        let ids = captures
            .values()
            .flat_map(|capture| capture.tree.elements.keys().copied())
            .collect();
        Self {
            captures,
            options,
            started: None,
            frame_now: None,
            ids,
        }
    }

    /// Bring the clock up to this frame. Returns whether the transition still
    /// runs. Called once per frame, before the tree builds.
    pub(crate) fn tick(&mut self, now: Instant) -> bool {
        let started = *self.started.get_or_insert(now);
        self.frame_now = Some(now);
        let elapsed = now.duration_since(started).as_secs_f64();
        elapsed < self.options.longest_end()
    }

    /// Whether the view must keep per-id state alive for a frozen clone.
    pub(crate) fn keeps(&self, id: u64) -> bool {
        self.ids.contains(&id)
    }

    fn capture(&self, name: &str) -> Option<&VtCapture> {
        self.captures.get(name)
    }

    /// This frame's animation values for one name, or `None` before the first
    /// tick.
    pub(crate) fn frame_for(&self, name: &str) -> Option<VtElementFrame> {
        let started = self.started?;
        let now = self.frame_now?;
        let (duration, delay, ease_spec) = self.options.timing(name);
        let elapsed = now.duration_since(started).as_secs_f64();
        let raw = if duration <= 0.0 {
            1.0
        } else {
            ((elapsed - delay) / duration).clamp(0.0, 1.0)
        };
        let t = motion::ease(raw, &ease_spec);

        let group = self.options.groups.get(name);
        // A group that names neither side crossfades, like the web default.
        // A group that names a side animates only what that side says.
        let explicit = group.is_some_and(|group| group.old.is_some() || group.new.is_some());
        let old = group.and_then(|group| group.old.clone()).unwrap_or_else(|| SideSpec {
            opacity: (!explicit).then_some([1.0, 0.0]),
            ..SideSpec::default()
        });
        let new = group.and_then(|group| group.new.clone()).unwrap_or_else(|| SideSpec {
            opacity: (!explicit).then_some([0.0, 1.0]),
            ..SideSpec::default()
        });
        let old_on_top = old.on_top.unwrap_or(false);
        Some(VtElementFrame {
            t,
            old,
            new,
            old_on_top,
        })
    }
}

/// The values one named element animates with on one frame.
pub(crate) struct VtElementFrame {
    t: f64,
    old: SideSpec,
    new: SideSpec,
    old_on_top: bool,
}

impl VtElementFrame {
    /// The live element's opacity this frame, or `None` when it holds still.
    pub(crate) fn new_opacity(&self) -> Option<f64> {
        self.new
            .opacity
            .map(|[from, to]| motion::mix(from, to, self.t))
    }

    /// The live element's blur sigma this frame, or `None` when it holds
    /// still.
    pub(crate) fn new_blur(&self) -> Option<f64> {
        self.new
            .blur
            .map(|[from, to]| motion::mix(from, to, self.t).max(0.0))
    }

    fn old_blur(&self) -> Option<f64> {
        self.old
            .blur
            .map(|[from, to]| motion::mix(from, to, self.t).max(0.0))
    }

    /// How far past the group's bounds this frame's blur reaches: three
    /// sigmas of the widest blur among the two sides. The group's mask
    /// grows by this, so the halo paints instead of clipping at the edge.
    fn mask_inflation(&self) -> Pixels {
        let sigma = self
            .new_blur()
            .unwrap_or(0.0)
            .max(self.old_blur().unwrap_or(0.0));
        px((3.0 * sigma).ceil() as f32)
    }

    fn offset(x: Option<[VtLen; 2]>, y: Option<[VtLen; 2]>, t: f64, extent: Size<Pixels>) -> Point<Pixels> {
        let resolve = |lens: Option<[VtLen; 2]>, extent: f32| {
            lens.map_or(0.0, |[from, to]| {
                motion::mix(from.resolve(extent as f64), to.resolve(extent as f64), t)
            })
        };
        point(
            px(resolve(x, f32::from(extent.width)) as f32),
            px(resolve(y, f32::from(extent.height)) as f32),
        )
    }

    fn new_offset(&self, extent: Size<Pixels>) -> Point<Pixels> {
        Self::offset(self.new.translate_x, self.new.translate_y, self.t, extent)
    }

    fn old_offset(&self, extent: Size<Pixels>) -> Point<Pixels> {
        Self::offset(self.old.translate_x, self.old.translate_y, self.t, extent)
    }

    fn old_opacity(&self) -> f64 {
        self.old
            .opacity
            .map_or(1.0, |[from, to]| motion::mix(from, to, self.t))
    }
}

// ── The transition element ───────────────────────────────────────────

/// Wrap one named live element for this frame of the transition.
pub(super) fn wrap(
    element: &RetainedElement,
    built: AnyElement,
    frame: VtElementFrame,
    ctx: &mut BuildCtx,
    window: &mut Window,
    cx: &mut gpui::Context<super::GpuixView>,
) -> AnyElement {
    use gpui::prelude::*;

    let name = element
        .style
        .as_deref()
        .and_then(|style| style.view_transition_name.as_deref())
        .unwrap_or_default();
    let vt = ctx.vt;
    let old = vt.and_then(|vt| vt.capture(name)).map(|capture| OldCopy {
        element: build_frozen(capture, &frame, ctx, window, cx),
        layout: IsolatedLayout::new(),
        origin: capture.origin + frame.old_offset(capture.size),
        size: capture.size,
    });
    VtGroup {
        child: built,
        old,
        frame,
    }
    .into_any_element()
}

/// Build the frozen copy of one capture, held at its captured size and faded
/// and blurred for this frame.
fn build_frozen(
    capture: &VtCapture,
    frame: &VtElementFrame,
    ctx: &mut BuildCtx,
    window: &mut Window,
    cx: &mut gpui::Context<super::GpuixView>,
) -> AnyElement {
    use gpui::prelude::*;

    // The frozen tree builds through the same walk as the live one. The
    // nested context clears `vt`, so a name inside the copy never starts
    // a transition of its own.
    let mut frozen_ctx = BuildCtx {
        tree: &capture.tree,
        event_callback: ctx.event_callback,
        focus_handles: ctx.focus_handles,
        scroll_handles: &mut *ctx.scroll_handles,
        custom_registry: &mut *ctx.custom_registry,
        virtual_lists: &mut *ctx.virtual_lists,
        motion_states: &mut *ctx.motion_states,
        scrollbars: &mut *ctx.scrollbars,
        now: ctx.now,
        motion_active: &mut *ctx.motion_active,
        selection: ctx.selection.clone(),
        cascade: ctx.cascade.clone(),
        highlight: None,
        highlights: &mut *ctx.highlights,
        highlight_events: &mut *ctx.highlight_events,
        vt: None,
        frozen: true,
        direct_rules: Vec::new(),
        descendant_rules: Vec::new(),
    };
    // A frozen copy builds outside the tree walk, so it has no child position
    // and the index states do not apply to it.
    let content = build_element(capture.root, None, &mut frozen_ctx, window, cx);
    // The shell fixes the copy at its captured size and carries this
    // frame's opacity down the whole copy.
    let mut shell = gpui::div()
        .w(capture.size.width)
        .h(capture.size.height)
        .overflow_hidden();
    shell.style().opacity = Some(frame.old_opacity() as f32);
    let shell = shell.child(content);
    // The blur rides a wrapper that does not clip. On the shell itself,
    // its `overflow: hidden` would clip the halo at the captured edge.
    match frame.old_blur() {
        Some(blur) => gpui::div()
            .w(capture.size.width)
            .h(capture.size.height)
            .blur(px(blur as f32))
            .child(shell)
            .into_any_element(),
        None => shell.into_any_element(),
    }
}

/// Build a frozen copy for every captured name that has no live element this
/// frame. The renderer appends these to the root wrapper, so they paint over
/// the tree at their captured place while the group's `old` side fades or
/// moves them out.
pub(super) fn exit_copies(
    ctx: &mut BuildCtx,
    window: &mut Window,
    cx: &mut gpui::Context<super::GpuixView>,
) -> Vec<AnyElement> {
    use gpui::prelude::*;

    let Some(vt) = ctx.vt else {
        return Vec::new();
    };
    let live: HashSet<&str> = ctx
        .tree
        .elements
        .values()
        .filter_map(|element| element.style.as_deref()?.view_transition_name.as_deref())
        .collect();
    // Sorted, so two exit copies paint in the same order on every frame.
    let mut names: Vec<&String> = vt
        .captures
        .keys()
        .filter(|name| !live.contains(name.as_str()))
        .collect();
    names.sort();
    let mut copies = Vec::new();
    for name in names {
        let Some(capture) = vt.capture(name) else {
            continue;
        };
        let Some(frame) = vt.frame_for(name) else {
            continue;
        };
        copies.push(
            ExitCopy {
                element: build_frozen(capture, &frame, ctx, window, cx),
                layout: IsolatedLayout::new(),
                origin: capture.origin + frame.old_offset(capture.size),
                size: capture.size,
            }
            .into_any_element(),
        );
    }
    copies
}

/// The frozen copy of one name, ready to paint at its captured place.
struct OldCopy {
    element: AnyElement,
    /// The copy lays out here rather than in the window's tree, because its
    /// captured size is fixed and must not join the live layout.
    layout: IsolatedLayout,
    origin: Point<Pixels>,
    size: Size<Pixels>,
}

/// One named element while the transition runs.
///
/// The group hands the live child's layout through untouched, so the page
/// around a transition lays out exactly as it will at rest. Movement happens
/// at paint: the child prepaints under an element offset, and the frozen copy
/// prepaints at its captured bounds. Both paint inside the group's bounds as
/// a mask, so a slide stays inside the element's own area. When a side blurs,
/// the mask grows by the blur's support, so the halo shows instead of
/// clipping at the edge.
struct VtGroup {
    child: AnyElement,
    old: Option<OldCopy>,
    frame: VtElementFrame,
}

impl Element for VtGroup {
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
        (self.child.request_layout(window, cx), ())
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
        let offset = self.frame.new_offset(bounds.size);
        let mask = bounds.dilate(self.frame.mask_inflation());
        window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
            if let Some(old) = &mut self.old {
                let element = &mut old.element;
                let origin = old.origin;
                let extent = old.size;
                old.layout.enter(window, |window| {
                    element.layout_as_root(
                        size(
                            AvailableSpace::Definite(extent.width),
                            AvailableSpace::Definite(extent.height),
                        ),
                        window,
                        cx,
                    );
                    element.prepaint_at(origin, window, cx);
                });
            }
            window.with_element_offset(offset, |window| {
                self.child.prepaint(window, cx);
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
        let mask = bounds.dilate(self.frame.mask_inflation());
        window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
            let paint_old = |old: &mut Option<OldCopy>, window: &mut Window, cx: &mut App| {
                if let Some(old) = old {
                    let element = &mut old.element;
                    old.layout.enter(window, |window| element.paint(window, cx));
                }
            };
            if self.frame.old_on_top {
                self.child.paint(window, cx);
                paint_old(&mut self.old, window, cx);
            } else {
                paint_old(&mut self.old, window, cx);
                self.child.paint(window, cx);
            }
        });
    }
}

impl IntoElement for VtGroup {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

/// One exit-only frozen copy, painted over the tree at its captured place.
///
/// The element asks for no layout space of its own. The copy lays out in its
/// own isolated tree at its captured size, so the live layout never sees it.
struct ExitCopy {
    element: AnyElement,
    layout: IsolatedLayout,
    origin: Point<Pixels>,
    size: Size<Pixels>,
}

impl Element for ExitCopy {
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
        (
            window.request_layout(gpui::Style::default(), None::<LayoutId>, cx),
            (),
        )
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let element = &mut self.element;
        let origin = self.origin;
        let extent = self.size;
        self.layout.enter(window, |window| {
            element.layout_as_root(
                size(
                    AvailableSpace::Definite(extent.width),
                    AvailableSpace::Definite(extent.height),
                ),
                window,
                cx,
            );
            element.prepaint_at(origin, window, cx);
        });
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let element = &mut self.element;
        self.layout.enter(window, |window| element.paint(window, cx));
    }
}

impl IntoElement for ExitCopy {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A state with no captures, halted at `elapsed_ms` into the transition.
    fn state_at(options: &str, elapsed_ms: u64) -> VtState {
        let options = VtOptions::parse(options).unwrap();
        let started = Instant::now();
        VtState {
            captures: HashMap::new(),
            options,
            started: Some(started),
            frame_now: Some(started + Duration::from_millis(elapsed_ms)),
            ids: HashSet::new(),
        }
    }

    #[test]
    fn blur_mixes_on_both_sides() {
        let state = state_at(
            r#"{"groups":{"screen":{"duration":0.3,"ease":"linear","old":{"blur":[0,6]},"new":{"blur":[6,0],"opacity":[0,1]}}}}"#,
            150,
        );
        let frame = state.frame_for("screen").unwrap();
        assert_eq!(frame.new_blur(), Some(3.0));
        assert_eq!(frame.old_blur(), Some(3.0));
        assert_eq!(frame.new_opacity(), Some(0.5));
    }

    #[test]
    fn a_side_without_blur_holds_still() {
        let state = state_at(r#"{"groups":{"screen":{"old":{"opacity":[1,0]}}}}"#, 150);
        let frame = state.frame_for("screen").unwrap();
        assert_eq!(frame.new_blur(), None);
        assert_eq!(frame.old_blur(), None);
    }
}
