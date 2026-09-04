//! One place that turns a `StyleDesc` into a GPUI `StyleRefinement`.
//!
//! GPUI is immediate mode. It rebuilds the element tree every frame. Before
//! this module the renderer ran 52 `if let Some` branches for every element on
//! every frame, and it ran them again for styles that had not changed since the
//! last mutation from React.
//!
//! `StyleRefinement` is the type the whole GPUI style API already speaks.
//! `Styled::style()` returns `&mut StyleRefinement`, and `hover`, `active`,
//! `group_hover` and `group_active` all take
//! `impl FnOnce(StyleRefinement) -> StyleRefinement`. `Refineable` also merges a
//! refinement into another refinement, so one resolved value covers the base
//! style and every variant. That makes the resolved refinement a cache the
//! renderer can hold and reuse until the style changes.

use std::sync::atomic::{AtomicU64, Ordering};

use gpui::{Refineable, StyleRefinement};

use crate::inheritance::Inherited;
use crate::style::vars::Scope;
use crate::style::{BackgroundValue, StyleDesc};

/// How many times `resolve` ran since the last reset.
///
/// The performance tests assert on this counter instead of on wall-clock time.
/// A steady-state frame must add zero. One `setStyle` must add one. A wall-clock
/// budget flakes on a loaded machine and then someone mutes it. A counter does
/// not flake, and it fails loudly when a cache stops working.
static RESOLUTIONS: AtomicU64 = AtomicU64::new(0);

/// Read the resolve counter.
pub(crate) fn resolutions() -> u64 {
    RESOLUTIONS.load(Ordering::Relaxed)
}

/// Set the resolve counter back to zero.
pub(crate) fn reset_resolutions() {
    RESOLUTIONS.store(0, Ordering::Relaxed);
}

/// One state pseudo-class, which is one kind of condition.
///
/// GPUI evaluates `Hover` and `Active` itself at paint, with no re-render and
/// no second resolve, so a pointer moving over an element costs nothing in
/// this crate. The index states have no GPUI counterpart. The walk knows the
/// child index and the child count when it builds an element, so it merges an
/// index refinement in place, and a list mutation re-evaluates it on the next
/// frame the way a resize re-evaluates a media condition.
///
/// Conditions are an open set. Adding `:focus` is one variant here and one arm
/// at the paint site, not a new field on every resolution in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Hover,
    Active,
    First,
    Last,
    Odd,
    Even,
    Only,
}

impl State {
    /// Whether the child position decides this state.
    pub(crate) fn is_index(self) -> bool {
        !matches!(self, State::Hover | State::Active)
    }

    /// Whether this index state holds at a child position.
    ///
    /// `index` is zero based. `:nth-child` counts from one, so the first
    /// child is odd.
    pub(crate) fn holds_at(self, index: usize, count: usize) -> bool {
        match self {
            State::Hover | State::Active => false,
            State::First => index == 0,
            State::Last => index + 1 == count,
            State::Odd => index % 2 == 0,
            State::Even => index % 2 == 1,
            State::Only => count == 1,
        }
    }
}

/// Which children of the declaring element a child rule styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChildScope {
    /// `& > *`: every direct child.
    All,
    /// `& > :not(:last-child)`: every direct child except the last, the
    /// selector `space-x-*` and `divide-*` compile to.
    ExceptLast,
    /// `& *`: every element below, at any depth.
    Descendants,
}

/// What one `selectors` entry means.
pub(crate) enum Selector {
    State(State),
    Children(ChildScope),
}

impl Selector {
    /// Read the selector text the class resolver produced.
    ///
    /// The set is closed. The resolver writes these canonical spellings, and
    /// anything else warns once and drops, so a typo never fails silently.
    pub(crate) fn parse(on: &str) -> Option<Selector> {
        Some(match on {
            ":first-child" => Selector::State(State::First),
            ":last-child" => Selector::State(State::Last),
            ":nth-child(odd)" => Selector::State(State::Odd),
            ":nth-child(even)" => Selector::State(State::Even),
            ":only-child" => Selector::State(State::Only),
            "& > *" => Selector::Children(ChildScope::All),
            "& > :not(:last-child)" => Selector::Children(ChildScope::ExceptLast),
            "& *" => Selector::Children(ChildScope::Descendants),
            _ => return None,
        })
    }
}

/// Warn about a selector the engine does not know, once per spelling.
pub(crate) fn warn_unknown_selector(on: &str) {
    use std::collections::HashSet;
    use std::sync::Mutex;
    static WARNED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
    let mut warned = WARNED.lock().unwrap();
    if warned.get_or_insert_with(HashSet::new).insert(on.to_owned()) {
        log::warn!("unknown selector {on:?} in a style, dropped");
    }
}

/// A `StyleDesc` with every value turned into a GPUI value.
///
/// The renderer stores this on the retained element and drops it when the style
/// changes. Applying it to an element costs one `refine` call per state.
#[derive(Debug, Clone)]
pub(crate) struct Resolved {
    pub base: StyleRefinement,
    /// The states this style declares, in the order `StyleDesc::states` lists
    /// them.
    ///
    /// Almost every element declares none. An empty `Vec` allocates nothing,
    /// where the two inline `Option<StyleRefinement>` fields this replaced
    /// carried the full size of a refinement each whether or not anything used
    /// them.
    pub states: Vec<(State, StyleRefinement)>,
    /// Rules this element puts on its children, from selectors such as
    /// `& > :not(:last-child)`. The refinements sit behind an `Arc` because
    /// the walk hands them down to every child in the scope, and a child
    /// applies them under its own declarations, the zero specificity of
    /// `:where()`.
    pub children: Vec<(ChildScope, std::sync::Arc<StyleRefinement>)>,
    /// The cascade this resolution read, or `None` when it read nothing
    /// inherited.
    ///
    /// A style with no `var()` and no `currentColor` computes the same value
    /// under every cascade, so `None` means the cached resolution stays valid
    /// however the cascade changes above it. That is almost every element, and
    /// it keeps the cost of custom properties on the elements that use them.
    pub cascade: Option<Inherited>,
}

impl Resolved {
    /// Resolve a style and every state it declares against a cascade.
    pub fn build(style: &StyleDesc, cascade: &Inherited) -> Self {
        let scope = cascade.scope();
        let base = resolve(style, &scope);
        let states = style
            .states()
            .map(|(state, declared)| (state, resolve(declared, &scope)))
            .collect();
        let mut children = Vec::new();
        for rule in style.selectors.iter().flatten() {
            match Selector::parse(&rule.on) {
                Some(Selector::Children(which)) => {
                    children.push((which, std::sync::Arc::new(resolve(&rule.style, &scope))));
                }
                // `states()` already read these.
                Some(Selector::State(_)) => {}
                None => warn_unknown_selector(&rule.on),
            }
        }
        Self {
            base,
            states,
            children,
            cascade: scope.used_a_variable().then(|| cascade.clone()),
        }
    }

    /// The refinement for one state, or `None` when the style does not declare
    /// it.
    pub fn state(&self, state: State) -> Option<&StyleRefinement> {
        self.states
            .iter()
            .find(|(declared, _)| *declared == state)
            .map(|(_, refinement)| refinement)
    }

    /// Whether this resolution still holds under `cascade`.
    pub fn valid_under(&self, cascade: &Inherited) -> bool {
        match &self.cascade {
            None => true,
            Some(read) => read.same(cascade),
        }
    }
}

/// Turn one `StyleDesc` into a `StyleRefinement`.
///
/// `apply_styles` is generic over `E: Styled`, so the compiler proves it only
/// calls style setters. That makes this wrapper the same work the renderer did
/// before, moved off the frame path.
pub(crate) fn resolve(style: &StyleDesc, scope: &Scope) -> StyleRefinement {
    RESOLUTIONS.fetch_add(1, Ordering::Relaxed);
    apply_styles(StyleRefinement::default(), style, scope)
}

/// Merge a resolved refinement into any styled element.
///
/// This is the whole per-frame cost of styling one element.
pub(crate) fn apply_resolved<E: gpui::Styled>(mut el: E, resolved: &StyleRefinement) -> E {
    el.style().refine(resolved);
    el
}

/// Apply a motion frame on top of a resolved style.
///
/// Motion drives eight numbers, and none of them reads a variable,
/// `currentColor` or the font size. Every one of them lands on the element
/// here, so an animated element keeps the cached resolution of everything it
/// declared. Folding the numbers into a `StyleDesc` and resolving that instead
/// reparsed every declaration the element made, on every frame, to change one
/// value.
pub(crate) fn apply_motion<E: gpui::Styled>(
    mut el: E,
    frame: &crate::motion::MotionFrame,
    declared: Option<&StyleDesc>,
) -> E {
    let motion = frame.style;
    if let Some(width) = motion.width {
        el = el.w(gpui::px(width as f32));
    }
    match motion.height.map(crate::motion::MotionHeight::length) {
        Some(Some(height)) => el = el.h(gpui::px(height as f32)),
        // A height that still needs the content has no number yet.
        // `auto_height::wrap` measures this element to find one, so this
        // element must not declare a height of its own.
        Some(None) => el.style().size.height = Some(gpui::Length::Auto),
        None => {}
    }
    if let Some(top) = motion.top {
        el = el.top(gpui::px(top as f32));
    }
    if let Some(right) = motion.right {
        el = el.right(gpui::px(right as f32));
    }
    if let Some(bottom) = motion.bottom {
        el = el.bottom(gpui::px(bottom as f32));
    }
    if let Some(left) = motion.left {
        el = el.left(gpui::px(left as f32));
    }
    if let Some(radius) = motion.border_radius {
        // A declared corner longhand beats the shorthand, which is the order
        // `apply_styles` reads the two in, so motion leaves that corner alone.
        let radius = gpui::px(radius as f32);
        let free = |declares: fn(&StyleDesc) -> bool| !declared.is_some_and(declares);
        if free(|style| style.border_top_left_radius.is_some()) {
            el = el.rounded_tl(radius);
        }
        if free(|style| style.border_top_right_radius.is_some()) {
            el = el.rounded_tr(radius);
        }
        if free(|style| style.border_bottom_left_radius.is_some()) {
            el = el.rounded_bl(radius);
        }
        if free(|style| style.border_bottom_right_radius.is_some()) {
            el = el.rounded_br(radius);
        }
    }
    if let Some(shape) = motion.corner_shape {
        // Same rule as the radius: a property narrower than `cornerShape`
        // keeps its corner. `corner` and `cornerShape` are what motion drives.
        let narrow = declared.map(|style| {
            let wide = StyleDesc {
                corner: None,
                corner_shape: None,
                ..style.clone()
            };
            super::corners::resolve(&wide).shapes
        });
        let shape = gpui::CornerShape(shape.0 as f32);
        let free = |pick: fn(&gpui::Corners<Option<f32>>) -> Option<f32>| {
            narrow.as_ref().and_then(pick).is_none()
        };
        if free(|c| c.top_left) {
            el = el.corner_shape_tl(shape);
        }
        if free(|c| c.top_right) {
            el = el.corner_shape_tr(shape);
        }
        if free(|c| c.bottom_left) {
            el = el.corner_shape_bl(shape);
        }
        if free(|c| c.bottom_right) {
            el = el.corner_shape_br(shape);
        }
    }
    if let Some(opacity) = motion.opacity {
        el = el.opacity(opacity as f32);
    }
    el
}

// ── Style application ────────────────────────────────────────────────

/// The six sizing properties, each read the same way and each landing in its
/// own slot. `Auto` is what all six already default to, so writing it changes
/// nothing.
fn apply_sizes<E: gpui::Styled>(mut el: E, style: &StyleDesc, scope: &Scope) -> E {
    let sizes = el.style();
    for (declared, slot) in [
        (&style.width, &mut sizes.size.width),
        (&style.height, &mut sizes.size.height),
        (&style.min_width, &mut sizes.min_size.width),
        (&style.min_height, &mut sizes.min_size.height),
        (&style.max_width, &mut sizes.max_size.width),
        (&style.max_height, &mut sizes.max_size.height),
    ] {
        if let Some(value) = scope.dimension(declared) {
            *slot = Some(dimension(value));
        }
    }
    el
}

/// The GPUI length a resolved sizing value means.
fn dimension(value: crate::style::DimensionValue) -> gpui::Length {
    match value {
        crate::style::DimensionValue::Pixels(pixels) => gpui::px(pixels as f32).into(),
        // A hair under the whole is a rounded 100%, and a whole is what
        // `w_full` writes.
        crate::style::DimensionValue::Percentage(share) if share >= 0.999 => {
            gpui::relative(1.0).into()
        }
        crate::style::DimensionValue::Percentage(share) => gpui::relative(share as f32).into(),
        crate::style::DimensionValue::Auto => gpui::Length::Auto,
    }
}

/// The colour and the image a box paints, image over colour, as a browser
/// paints `background-color` under `background-image`.
///
/// `background` is the shorthand. A colour in it is the colour, a gradient in
/// it is the image, and a declared longhand wins over it either way. A
/// longhand that reads as nothing, like `backgroundImage: "none"` or a value
/// this build cannot paint, still wins, so the shorthand does not show
/// through it. The object form of a gradient is always an image.
pub(crate) fn background_fills(
    style: &StyleDesc,
    scope: &Scope,
) -> (Option<gpui::Background>, Option<gpui::Background>) {
    use crate::color::to_background;
    use gpuix_css::background::Fill;
    let (mut color, mut image) = match style.background.as_ref() {
        Some(BackgroundValue::Text(text)) => match scope.fill(text) {
            Some(Fill::Color(c)) => (Some(to_background(&Fill::Color(c))), None),
            Some(gradient) => (None, Some(to_background(&gradient))),
            None => (None, None),
        },
        Some(BackgroundValue::Gradient(gradient)) => (None, scope.gradient(gradient)),
        None => (None, None),
    };
    if let Some(text) = style.background_color.as_deref() {
        color = scope.fill(text).map(|fill| to_background(&fill));
    }
    if let Some(text) = style.background_image.as_deref() {
        image = scope.fill(text).map(|fill| to_background(&fill));
    }
    (color, image)
}

/// The `overscroll-behavior` of one axis, from its longhand or the
/// shorthand. The shorthand takes one word for both axes or two, x first.
fn overscroll(
    longhand: Option<&str>,
    shorthand: Option<&str>,
    axis: usize,
) -> Option<gpui::Overscroll> {
    let word = match longhand {
        Some(word) => word.trim(),
        None => {
            // The shorthand has one or two words. Walk it without a Vec,
            // because this runs for every element on every render.
            let mut words = shorthand?.split_whitespace();
            let first = words.next()?;
            match (words.next(), words.next()) {
                (None, _) => first,
                (Some(second), None) => {
                    if axis == 0 {
                        first
                    } else {
                        second
                    }
                }
                (Some(_), Some(_)) => return None,
            }
        }
    };
    match word {
        "auto" => Some(gpui::Overscroll::Auto),
        "contain" => Some(gpui::Overscroll::Contain),
        "none" => Some(gpui::Overscroll::None),
        _ => None,
    }
}

/// Base styles plus gpui's `hover` and `active` refinements.
///
/// Every stateful GPUI root must go through this, never `apply_styles` alone.
/// `StyleDesc` carries `hover` and `active` for every element type, so a custom
/// element that only applied the base styles accepted the prop, serialized it,
/// and dropped it. gpui reads both refinements from the element state behind the
/// element's `ElementId`, so the caller must have called `.id(..)` first.
pub(crate) fn apply_interactive_styles<E>(mut el: E, style: &StyleDesc, scope: &Scope) -> E
where
    E: gpui::Styled + gpui::StatefulInteractiveElement,
{
    el = apply_styles(el, style, scope);
    if let Some(hover_style) = style.hover.as_deref() {
        el = el.hover(|refinement| apply_styles(refinement, hover_style, scope));
    }
    if let Some(active_style) = style.active.as_deref() {
        el = el.active(|refinement| apply_styles(refinement, active_style, scope));
    }
    el
}

pub(crate) fn apply_styles<E: gpui::Styled>(mut el: E, style: &StyleDesc, scope: &Scope) -> E {
    // `visibility` reached StyleDesc but nothing read it, so `hideInstance`
    // hid nothing. GPUI's Visibility::Hidden has the CSS meaning: skip the
    // paint, keep the layout box.
    match style.visibility.as_deref() {
        Some("hidden") => el.style().visibility = Some(gpui::Visibility::Hidden),
        Some("visible") => el.style().visibility = Some(gpui::Visibility::Visible),
        _ => {}
    }
    match style.display.as_deref() {
        Some("flex") => el = el.flex(),
        Some("grid") => el = el.grid(),
        _ => {}
    }
    if let Some(cols) = scope.number(&style.grid_template_columns) {
        let count = cols.round().clamp(1.0, 64.0) as u16;
        el = match style.grid_column_min.as_deref() {
            Some("min-content") => el.grid_cols_min_content(count),
            Some("max-content") => el.grid_cols_max_content(count),
            _ => el.grid_cols(count),
        };
    }
    if let Some(rows) = scope.number(&style.grid_template_rows) {
        let count = rows.round().clamp(1.0, 64.0) as u16;
        el = match style.grid_row_min.as_deref() {
            Some("min-content") => el.grid_rows_min_content(count),
            Some("max-content") => el.grid_rows_max_content(count),
            _ => el.grid_rows(count),
        };
    }
    if style.flex_direction.as_deref() == Some("column") {
        el = el.flex_col();
    }
    if style.flex_direction.as_deref() == Some("row") {
        el = el.flex_row();
    }
    match style.flex_wrap.as_deref() {
        Some("wrap") => el = el.flex_wrap(),
        Some("wrap-reverse") => el = el.flex_wrap_reverse(),
        Some("nowrap") => el = el.flex_nowrap(),
        _ => {}
    }
    if let Some(grow) = scope.number(&style.flex_grow) {
        el.style().flex_grow = Some(grow as f32);
    }
    if let Some(shrink) = scope.number(&style.flex_shrink) {
        el.style().flex_shrink = Some(shrink as f32);
    }
    if let Some(basis) = scope.number(&style.flex_basis) {
        el = el.flex_basis(gpui::px(basis as f32));
    }
    match style.align_items.as_deref() {
        Some("center") => el = el.items_center(),
        Some("start") | Some("flex-start") => el = el.items_start(),
        Some("end") | Some("flex-end") => el = el.items_end(),
        _ => {}
    }
    match style.align_content.as_deref() {
        Some("center") => el = el.content_center(),
        Some("start") | Some("flex-start") => el = el.content_start(),
        Some("end") | Some("flex-end") => el = el.content_end(),
        Some("between") | Some("space-between") => el = el.content_between(),
        Some("around") | Some("space-around") => el = el.content_around(),
        Some("evenly") | Some("space-evenly") => el = el.content_evenly(),
        Some("stretch") => el = el.content_stretch(),
        Some("normal") => el = el.content_normal(),
        _ => {}
    }
    match style.justify_content.as_deref() {
        Some("center") => el = el.justify_center(),
        Some("start") | Some("flex-start") => el = el.justify_start(),
        Some("end") | Some("flex-end") => el = el.justify_end(),
        Some("between") | Some("space-between") => el = el.justify_between(),
        Some("around") | Some("space-around") => el = el.justify_around(),
        _ => {}
    }
    match style.align_self.as_deref() {
        Some("center") => {
            el.style().align_self = Some(gpui::AlignItems::Center);
        }
        Some("start") | Some("flex-start") => {
            el.style().align_self = Some(gpui::AlignItems::FlexStart);
        }
        Some("end") | Some("flex-end") => {
            el.style().align_self = Some(gpui::AlignItems::FlexEnd);
        }
        Some("stretch") => {
            el.style().align_self = Some(gpui::AlignItems::Stretch);
        }
        Some("baseline") => {
            el.style().align_self = Some(gpui::AlignItems::Baseline);
        }
        _ => {}
    }
    if let Some(gap) = scope.number(&style.gap) {
        el = el.gap(gpui::px(gap as f32));
    }
    // Per-axis gaps were in the style type and implemented nowhere. They come
    // after `gap` so the axis value wins, matching CSS shorthand order.
    if let Some(gap) = scope.number(&style.row_gap) {
        el = el.gap_y(gpui::px(gap as f32));
    }
    if let Some(gap) = scope.number(&style.column_gap) {
        el = el.gap_x(gpui::px(gap as f32));
    }
    el = apply_sizes(el, style, scope);
    if let Some(p) = scope.number(&style.padding) {
        el = el.p(gpui::px(p as f32));
    }
    if let Some(pt) = scope.number(&style.padding_top) {
        el = el.pt(gpui::px(pt as f32));
    }
    if let Some(pr) = scope.number(&style.padding_right) {
        el = el.pr(gpui::px(pr as f32));
    }
    if let Some(pb) = scope.number(&style.padding_bottom) {
        el = el.pb(gpui::px(pb as f32));
    }
    if let Some(pl) = scope.number(&style.padding_left) {
        el = el.pl(gpui::px(pl as f32));
    }
    if let Some(m) = scope.number(&style.margin) {
        el = el.m(gpui::px(m as f32));
    }
    if let Some(mt) = scope.number(&style.margin_top) {
        el = el.mt(gpui::px(mt as f32));
    }
    if let Some(mr) = scope.number(&style.margin_right) {
        el = el.mr(gpui::px(mr as f32));
    }
    if let Some(mb) = scope.number(&style.margin_bottom) {
        el = el.mb(gpui::px(mb as f32));
    }
    if let Some(ml) = scope.number(&style.margin_left) {
        el = el.ml(gpui::px(ml as f32));
    }
    // Taffy has no viewport-fixed position, and GPUI has no scrolling document,
    // so "fixed" lays out exactly like "absolute". `should_occlude` already
    // treats the two the same. Without this arm a "fixed" box stayed in flow.
    match style.position.as_deref() {
        Some("absolute") | Some("fixed") => el = el.absolute(),
        Some("relative") => el = el.relative(),
        _ => {}
    }
    if let Some(top) = scope.number(&style.top) {
        el = el.top(gpui::px(top as f32));
    }
    if let Some(right) = scope.number(&style.right) {
        el = el.right(gpui::px(right as f32));
    }
    if let Some(bottom) = scope.number(&style.bottom) {
        el = el.bottom(gpui::px(bottom as f32));
    }
    if let Some(left) = scope.number(&style.left) {
        el = el.left(gpui::px(left as f32));
    }
    let (background_color, background_image) = background_fills(style, scope);
    if let Some(background) = background_color {
        el = el.bg(background);
    }
    if let Some(image) = background_image {
        el = el.background_image(image);
    }
    if let Some(mode) = style
        .background_blend_mode
        .as_deref()
        .and_then(|text| scope.blend_mode(text))
    {
        el = el.background_blend_mode(mode);
    }
    if let Some(mode) = style
        .mix_blend_mode
        .as_deref()
        .and_then(|text| scope.blend_mode(text))
    {
        el = el.blend_mode(mode);
    }
    if let Some(filter) = style.filter.as_deref().and_then(|text| scope.filter(text)) {
        el = el
            .blur(gpui::px(filter.blur))
            .color_matrix(gpui::ColorMatrix(filter.matrix));
    }
    if let Some(filter) = style
        .backdrop_filter
        .as_deref()
        .and_then(|text| scope.filter(text))
    {
        el = el
            .backdrop_blur(gpui::px(filter.blur))
            .backdrop_matrix(gpui::ColorMatrix(filter.matrix));
    }
    if let Some(mask) = style
        .mask_image
        .as_deref()
        .and_then(|text| scope.fill(text))
    {
        el = el.mask(crate::color::to_background(&mask));
    }
    let overscroll_x = overscroll(
        style.overscroll_behavior_x.as_deref(),
        style.overscroll_behavior.as_deref(),
        0,
    );
    let overscroll_y = overscroll(
        style.overscroll_behavior_y.as_deref(),
        style.overscroll_behavior.as_deref(),
        1,
    );
    if overscroll_x.is_some() || overscroll_y.is_some() {
        el = el.overscroll_behavior(
            overscroll_x.unwrap_or_default(),
            overscroll_y.unwrap_or_default(),
        );
    }
    if let Some(color) = style.color.as_deref().and_then(|c| scope.color(c)) {
        el = el.text_color(crate::color::to_hsla(color));
    }
    if let Some(size) = scope.number(&style.font_size) {
        el = el.text_size(gpui::px(size as f32));
    }
    if let Some(ref family) = style.font_family {
        el = el.font_family(family.clone());
    }
    if let Some(ref weight) = style.font_weight {
        el = el.font_weight(parse_font_weight(weight));
    }
    // `textAlign` was in the style type but implemented nowhere.
    match style.text_align.as_deref() {
        Some("center") => el = el.text_center(),
        Some("right") => el = el.text_right(),
        Some("left") | Some("start") => el = el.text_left(),
        _ => {}
    }
    match style.white_space.as_deref() {
        Some("nowrap") => el = el.whitespace_nowrap(),
        Some("normal") => el = el.whitespace_normal(),
        _ => {}
    }
    match style.text_overflow.as_deref() {
        Some("ellipsis") => el = el.text_ellipsis(),
        Some("ellipsis-start") => el = el.text_ellipsis_start(),
        _ => {}
    }
    if let Some(clamp) = scope.number(&style.line_clamp) {
        if clamp >= 1.0 {
            el = el.line_clamp(clamp as usize);
        }
    }
    // A JS number is pixels, so `lineHeight: 20` is 20px as in the upstream
    // API. A string follows CSS: a bare number is a multiple of the font
    // size, and a length is that length.
    if let Some(crate::style::Numeric::Number(pixels)) = style.line_height {
        if pixels > 0.0 {
            el = el.line_height(gpui::px(pixels as f32));
        }
    } else if let Some(line_height) = scope.length(&style.line_height) {
        match line_height {
            gpuix_css::length::Length::Number(multiple)
            | gpuix_css::length::Length::Fraction(multiple)
                if multiple > 0.0 =>
            {
                el = el.line_height(gpui::relative(multiple));
            }
            gpuix_css::length::Length::Pixels(pixels) if pixels > 0.0 => {
                el = el.line_height(gpui::px(pixels));
            }
            _ => {}
        }
    }
    let corners = super::corners::resolve(style);
    if let Some(radius) = scope.number(&corners.radii.top_left) {
        el = el.rounded_tl(gpui::px(radius as f32));
    }
    if let Some(radius) = scope.number(&corners.radii.top_right) {
        el = el.rounded_tr(gpui::px(radius as f32));
    }
    if let Some(radius) = scope.number(&corners.radii.bottom_left) {
        el = el.rounded_bl(gpui::px(radius as f32));
    }
    if let Some(radius) = scope.number(&corners.radii.bottom_right) {
        el = el.rounded_br(gpui::px(radius as f32));
    }
    if let Some(shape) = corners.shapes.top_left {
        el = el.corner_shape_tl(gpui::CornerShape(shape));
    }
    if let Some(shape) = corners.shapes.top_right {
        el = el.corner_shape_tr(gpui::CornerShape(shape));
    }
    if let Some(shape) = corners.shapes.bottom_left {
        el = el.corner_shape_bl(gpui::CornerShape(shape));
    }
    if let Some(shape) = corners.shapes.bottom_right {
        el = el.corner_shape_br(gpui::CornerShape(shape));
    }
    // `borderWidth: 0` must clear a border, not be ignored: an element that
    // draws its own border needs a way for the caller to remove it.
    if let Some(width) = scope.number(&style.border_width) {
        el = el.border(gpui::px(width.max(0.0) as f32));
    }
    if let Some(width) = scope.number(&style.border_top_width) {
        el = el.border_t(gpui::px(width.max(0.0) as f32));
    }
    if let Some(width) = scope.number(&style.border_right_width) {
        el = el.border_r(gpui::px(width.max(0.0) as f32));
    }
    if let Some(width) = scope.number(&style.border_bottom_width) {
        el = el.border_b(gpui::px(width.max(0.0) as f32));
    }
    if let Some(width) = scope.number(&style.border_left_width) {
        el = el.border_l(gpui::px(width.max(0.0) as f32));
    }
    if let Some(color) = style.border_color.as_deref().and_then(|c| scope.color(c)) {
        el = el.border_color(crate::color::to_hsla(color));
    }
    if let Some(ref shadow) = style.box_shadow {
        if let Some(color) = scope.color(&shadow.color) {
            let shadow = gpui::BoxShadow::new(
                gpui::px(shadow.offset_x as f32),
                gpui::px(shadow.offset_y as f32),
                crate::color::to_hsla(color),
            )
            .blur_radius(gpui::px(shadow.blur_radius.max(0.0) as f32))
            .spread_radius(gpui::px(shadow.spread_radius as f32));
            el = el.shadow(vec![shadow]);
        }
    }
    if let Some(opacity) = scope.number(&style.opacity) {
        el = el.opacity(opacity as f32);
    }
    if let Some(cursor) = style.cursor.as_deref().and_then(cursor_style) {
        el = el.cursor(cursor);
    }
    // Overflow: hidden is on the Styled trait, so we handle it here.
    // overflow: "scroll" requires StatefulInteractiveElement — handled in build_host_container().
    // CSS precedence: axis-specific (overflowX/Y) overrides the shorthand (overflow).
    {
        let resolved_x = style.overflow_x.as_deref().or(style.overflow.as_deref());
        let resolved_y = style.overflow_y.as_deref().or(style.overflow.as_deref());
        // Only apply hidden here — scroll is handled in build_host_container.
        if resolved_x == Some("hidden") && resolved_y == Some("hidden") {
            el = el.overflow_hidden();
        } else if resolved_x == Some("hidden") {
            el = el.overflow_x_hidden();
        } else if resolved_y == Some("hidden") {
            el = el.overflow_y_hidden();
        }
    }

    el
}

/// Parse a CSS font-weight value (string or number) into a GPUI FontWeight.
/// Accepts named keywords ("bold", "semibold"), numeric strings ("700"),
/// and raw numbers (700). Falls back to 400 (normal) for unrecognized values.
pub(crate) fn parse_font_weight(value: &crate::style::FontWeightValue) -> gpui::FontWeight {
    match value {
        crate::style::FontWeightValue::Num(n) => gpui::FontWeight((*n as f32).clamp(1.0, 1000.0)),
        crate::style::FontWeightValue::Str(s) => {
            let lower = s.trim().to_ascii_lowercase();
            match lower.as_str() {
                "100" | "thin" => gpui::FontWeight(100.0),
                "200" | "extralight" | "extra-light" => gpui::FontWeight(200.0),
                "300" | "light" => gpui::FontWeight(300.0),
                "400" | "normal" => gpui::FontWeight(400.0),
                "500" | "medium" => gpui::FontWeight(500.0),
                "600" | "semibold" | "semi-bold" => gpui::FontWeight(600.0),
                "700" | "bold" => gpui::FontWeight(700.0),
                "800" | "extrabold" | "extra-bold" => gpui::FontWeight(800.0),
                "900" | "black" => gpui::FontWeight(900.0),
                _ => lower
                    .parse::<f32>()
                    .map(|n| gpui::FontWeight(n.clamp(1.0, 1000.0)))
                    .unwrap_or(gpui::FontWeight(400.0)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_overscroll_shorthand_splits_by_axis() {
        // One word covers both axes. Two words are x then y, as in CSS.
        // Three words declare nothing.
        assert_eq!(
            overscroll(None, Some("contain"), 1),
            Some(gpui::Overscroll::Contain)
        );
        assert_eq!(
            overscroll(None, Some("contain none"), 0),
            Some(gpui::Overscroll::Contain)
        );
        assert_eq!(
            overscroll(None, Some("contain none"), 1),
            Some(gpui::Overscroll::None)
        );
        assert_eq!(overscroll(None, Some("contain none auto"), 0), None);
        // The longhand wins over the shorthand.
        assert_eq!(
            overscroll(Some("auto"), Some("contain"), 0),
            Some(gpui::Overscroll::Auto)
        );
    }

    fn styled(color: &str) -> Box<StyleDesc> {
        Box::new(StyleDesc {
            background_color: Some(color.to_string()),
            ..Default::default()
        })
    }

    /// A cascade with nothing declared above it.
    fn no_variables() -> Inherited {
        let theme = crate::theme::Theme::default();
        Inherited::root(crate::color::from_gpui(theme.accent), theme.dark, 16.0)
    }

    /// A cascade with `pairs` declared one level down.
    fn variables(pairs: &[(&str, &str)]) -> Inherited {
        let custom = pairs
            .iter()
            .map(|(name, value)| (name.to_string(), serde_json::json!(value)))
            .collect();
        let style = StyleDesc {
            custom,
            ..Default::default()
        };
        no_variables().descend(Some(&style))
    }

    fn background_of(style: &StyleDesc, cascade: &Inherited) -> Option<gpui::Fill> {
        Resolved::build(style, cascade).base.background
    }

    fn fill(color: &str) -> Option<gpui::Fill> {
        Some(crate::color::parse_color_rgba(color).unwrap().into())
    }

    #[test]
    fn resolves_the_base_style_and_each_state() {
        let style = StyleDesc {
            background_color: Some("#111111".to_string()),
            hover: Some(styled("#ff0000")),
            active: Some(styled("#00ff00")),
            ..Default::default()
        };
        let cascade = no_variables();
        let resolved = Resolved::build(&style, &cascade);
        let plain = cascade.scope();
        assert_eq!(
            resolved.state(State::Hover),
            Some(&resolve(&styled("#ff0000"), &plain))
        );
        assert_eq!(
            resolved.state(State::Active),
            Some(&resolve(&styled("#00ff00"), &plain))
        );
        // The list keeps the order `StyleDesc::states` declares, which is the
        // order the paint dispatcher walks.
        assert_eq!(
            resolved.states.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
            vec![State::Hover, State::Active]
        );
    }

    fn image_of(style: &StyleDesc, cascade: &Inherited) -> Option<gpui::Background> {
        Resolved::build(style, cascade).base.background_image
    }

    #[test]
    fn index_selectors_resolve_as_states() {
        use crate::style::SelectorRule;
        let rule = |on: &str, color: &str| SelectorRule {
            on: on.to_string(),
            style: styled(color),
        };
        let style = StyleDesc {
            selectors: Some(vec![
                rule(":first-child", "#ff0000"),
                rule(":last-child", "#00ff00"),
                rule(":nth-child(odd)", "#0000ff"),
                rule(":nth-child(even)", "#ffff00"),
                rule(":only-child", "#00ffff"),
            ]),
            ..Default::default()
        };
        let cascade = no_variables();
        let resolved = Resolved::build(&style, &cascade);
        assert_eq!(
            resolved.states.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
            vec![State::First, State::Last, State::Odd, State::Even, State::Only]
        );
        let plain = cascade.scope();
        assert_eq!(
            resolved.state(State::First),
            Some(&resolve(&styled("#ff0000"), &plain))
        );
        assert!(resolved.children.is_empty());
    }

    #[test]
    fn child_selectors_resolve_as_child_rules_and_unknown_ones_drop() {
        use crate::style::SelectorRule;
        let rule = |on: &str, color: &str| SelectorRule {
            on: on.to_string(),
            style: styled(color),
        };
        let style = StyleDesc {
            selectors: Some(vec![
                rule("& > *", "#ff0000"),
                rule("& > :not(:last-child)", "#00ff00"),
                rule("& *", "#0000ff"),
                rule(":focus", "#ffffff"),
            ]),
            ..Default::default()
        };
        let resolved = Resolved::build(&style, &no_variables());
        assert_eq!(
            resolved.children.iter().map(|(scope, _)| *scope).collect::<Vec<_>>(),
            vec![ChildScope::All, ChildScope::ExceptLast, ChildScope::Descendants]
        );
        // `:focus` is not in the closed set yet: it warns once and drops,
        // and it must not leak into the states either.
        assert!(resolved.states.is_empty());
    }

    #[test]
    fn an_index_state_reads_the_child_position() {
        let table = [
            (State::First, 0, 3, true),
            (State::First, 1, 3, false),
            (State::Last, 2, 3, true),
            (State::Last, 1, 3, false),
            // `:nth-child` counts from one, so the first child is odd.
            (State::Odd, 0, 3, true),
            (State::Odd, 1, 3, false),
            (State::Even, 1, 3, true),
            (State::Even, 2, 3, false),
            (State::Only, 0, 1, true),
            (State::Only, 0, 2, false),
        ];
        for (state, index, count, holds) in table {
            assert_eq!(state.holds_at(index, count), holds, "{state:?} at {index} of {count}");
            assert!(state.is_index());
        }
        assert!(!State::Hover.is_index());
        assert!(!State::Active.is_index());
    }

    #[test]
    fn a_gradient_image_paints_over_the_colour() {
        let style = StyleDesc {
            background_color: Some("#111111".to_string()),
            background_image: Some("linear-gradient(to right, red, blue)".to_string()),
            ..Default::default()
        };
        assert_eq!(background_of(&style, &no_variables()), fill("#111111"));
        let image = image_of(&style, &no_variables()).expect("an image");
        assert!(
            image.as_solid().is_none(),
            "should be a gradient: {image:?}"
        );

        // `none` leaves the colour on its own.
        let style = StyleDesc {
            background_image: Some("none".to_string()),
            ..style
        };
        assert_eq!(background_of(&style, &no_variables()), fill("#111111"));
        assert_eq!(image_of(&style, &no_variables()), None);

        // The shorthand takes a gradient too, and it is the image.
        let shorthand = StyleDesc {
            background: Some(BackgroundValue::Text("linear-gradient(red, blue)".to_string())),
            ..Default::default()
        };
        assert_eq!(background_of(&shorthand, &no_variables()), None);
        assert!(image_of(&shorthand, &no_variables()).is_some());
    }

    #[test]
    fn effects_reach_the_style() {
        let style = StyleDesc {
            filter: Some("blur(4px) grayscale(1)".to_string()),
            backdrop_filter: Some("blur(20px)".to_string()),
            mask_image: Some("linear-gradient(black, transparent)".to_string()),
            mix_blend_mode: Some("multiply".to_string()),
            background_blend_mode: Some("screen".to_string()),
            overscroll_behavior: Some("contain auto".to_string()),
            overscroll_behavior_y: Some("none".to_string()),
            ..Default::default()
        };
        let base = Resolved::build(&style, &no_variables()).base;
        let effects = base.effects.expect("effects");
        assert_eq!(effects.blur, gpui::px(4.0));
        assert!(!effects.color_matrix.is_identity());
        assert_eq!(effects.backdrop_blur, gpui::px(20.0));
        assert!(effects.mask.is_some());
        assert_eq!(effects.blend_mode, gpui::BlendMode::Multiply);
        assert_eq!(base.background_blend_mode, Some(gpui::BlendMode::Screen));
        assert_eq!(base.overscroll_behavior.x, Some(gpui::Overscroll::Contain));
        assert_eq!(base.overscroll_behavior.y, Some(gpui::Overscroll::None));

        // `none` and a filter this build cannot paint both set nothing.
        let style = StyleDesc {
            filter: Some("drop-shadow(1px 1px red)".to_string()),
            backdrop_filter: Some("none".to_string()),
            ..Default::default()
        };
        let base = Resolved::build(&style, &no_variables()).base;
        assert!(base.effects.is_none());
    }

    #[test]
    fn the_object_form_of_a_gradient_paints_in_its_colour_space() {
        let style: StyleDesc = serde_json::from_str(
            r##"{"background":{"type":"linear-gradient","angle":90,"stops":[{"color":"var(--from)","position":0},{"color":"#0000ff","position":1}],"colorSpace":"oklab"}}"##,
        )
        .unwrap();
        let scope = variables(&[("--from", "#ff0000")]);
        assert_eq!(background_of(&style, &scope), None);
        let background = image_of(&style, &scope).expect("an image");
        let stop = |hex: &str, position: f32| gpui::LinearColorStop {
            color: crate::color::to_hsla(crate::color::from_gpui(
                crate::color::parse_color_rgba(hex).unwrap(),
            )),
            percentage: position,
            hint: 0.0,
            easing: [0.0; 4],
        };
        let stops = [stop("#ff0000", 0.0), stop("#0000ff", 1.0)];
        let in_oklab = gpui::linear_gradient_stops(gpui::GradientLine::Angle(90.0), &stops)
            .color_space(gpui::ColorSpace::Oklab);
        assert_eq!(background, in_oklab);
        assert_ne!(
            background,
            gpui::linear_gradient_stops(gpui::GradientLine::Angle(90.0), &stops)
        );

        let out_of_range: StyleDesc = serde_json::from_str(
            r##"{"background":{"type":"linear-gradient","angle":90,"stops":[{"color":"red","position":0},{"color":"blue","position":2}]}}"##,
        )
        .unwrap();
        assert!(image_of(&out_of_range, &no_variables()).is_none());
    }

    #[test]
    fn a_style_with_no_states_resolves_to_none() {
        let resolved = Resolved::build(&styled("#111111"), &no_variables());
        assert!(resolved.states.is_empty());
        assert!(resolved.state(State::Hover).is_none());
        assert!(resolved.state(State::Active).is_none());
    }

    #[test]
    fn an_unknown_style_field_does_not_fail_the_whole_style() {
        // A newer client must lose one declaration, not its element.
        let json = r##"{ "backgroundColor": "#111111", "someFutureThing": 4 }"##;
        let style: StyleDesc = serde_json::from_str(json).expect("style should still parse");
        assert_eq!(style.background_color.as_deref(), Some("#111111"));
    }

    #[test]
    fn a_variable_reaches_a_colour() {
        let scope = variables(&[("--brand", "#ff0000")]);
        assert_eq!(
            background_of(&styled("var(--brand)"), &scope),
            fill("#ff0000")
        );
    }

    #[test]
    fn a_variable_reaches_a_state_colour() {
        let style = StyleDesc {
            hover: Some(styled("var(--brand)")),
            ..Default::default()
        };
        let scope = variables(&[("--brand", "#ff0000")]);
        let resolved = Resolved::build(&style, &scope);
        assert_eq!(
            resolved.state(State::Hover).and_then(|h| h.background.clone()),
            fill("#ff0000")
        );
    }

    #[test]
    fn a_missing_variable_leaves_the_colour_unset() {
        // CSS calls this invalid at computed-value time. The property takes the
        // value it would have had, which here is no background at all.
        assert_eq!(background_of(&styled("var(--nope)"), &no_variables()), None);
    }

    #[test]
    fn a_fallback_paints_when_the_variable_is_missing() {
        assert_eq!(
            background_of(&styled("var(--nope, #00ff00)"), &no_variables()),
            fill("#00ff00")
        );
    }

    #[test]
    fn a_style_that_reads_nothing_holds_under_every_cascade() {
        // This is what keeps custom properties off the cost of every other
        // element. A resolution that read nothing is never invalidated.
        let resolved = Resolved::build(&styled("#111111"), &no_variables());
        assert!(resolved.cascade.is_none());
        assert!(resolved.valid_under(&variables(&[("--brand", "#ff0000")])));
    }

    #[test]
    fn a_style_that_read_a_variable_only_holds_under_that_cascade() {
        let cascade = variables(&[("--brand", "#ff0000")]);
        let resolved = Resolved::build(&styled("var(--brand)"), &cascade);
        assert!(resolved.valid_under(&cascade));
        assert!(!resolved.valid_under(&variables(&[("--brand", "#ff0000")])));
    }

    #[test]
    fn a_var_that_falls_back_still_counts_as_reading_the_cascade() {
        // The fallback won because nothing declared the variable. A different
        // cascade could declare one, so the resolution has to be bound to it.
        let resolved = Resolved::build(&styled("var(--brand, #00ff00)"), &no_variables());
        assert!(resolved.cascade.is_some());
    }

    #[test]
    fn current_color_reads_the_inherited_colour() {
        let cascade = no_variables().descend(Some(&StyleDesc {
            color: Some("#ff0000".to_string()),
            ..Default::default()
        }));
        let style = StyleDesc {
            border_color: Some("currentColor".to_string()),
            ..Default::default()
        };
        let resolved = Resolved::build(&style, &cascade);
        assert_eq!(
            resolved.base.border_color,
            crate::color::parse_color_rgba("#ff0000").map(Into::into)
        );
        // It read the cascade, so it must not survive a cascade change.
        assert!(resolved.cascade.is_some());
    }

    #[test]
    fn current_color_takes_the_declaration_on_the_element_itself() {
        let style = StyleDesc {
            color: Some("#00ff00".to_string()),
            border_color: Some("currentColor".to_string()),
            ..Default::default()
        };
        // The walk descends before it resolves, so the element's own colour is
        // already in the cascade by the time `currentColor` reads it.
        let cascade = no_variables().descend(Some(&style));
        let resolved = Resolved::build(&style, &cascade);
        assert_eq!(
            resolved.base.border_color,
            crate::color::parse_color_rgba("#00ff00").map(Into::into)
        );
    }

    fn line_height_of(text: &str) -> Option<gpui::DefiniteLength> {
        let style = StyleDesc {
            line_height: Some(crate::style::Numeric::Text(text.to_string())),
            ..Default::default()
        };
        Resolved::build(&style, &no_variables()).base.text.line_height
    }

    #[test]
    fn a_bare_line_height_in_a_string_is_a_multiple_of_the_font_size() {
        // CSS reads `line-height: 1.5` as one and a half times the font size.
        // Reading it as 1.5 pixels would collapse every line onto the last.
        assert_eq!(line_height_of("1.5"), Some(gpui::relative(1.5)));
    }

    #[test]
    fn a_numeric_line_height_is_pixels() {
        // The upstream API reads `lineHeight: 20` as 20 pixels, like React
        // Native. Only a string value gets the CSS multiple reading.
        let numeric = StyleDesc {
            line_height: Some(crate::style::Numeric::Number(20.0)),
            ..Default::default()
        };
        assert_eq!(
            Resolved::build(&numeric, &no_variables()).base.text.line_height,
            Some(gpui::px(20.0).into())
        );
    }

    #[test]
    fn a_line_height_with_a_unit_is_that_length() {
        assert_eq!(line_height_of("24px"), Some(gpui::px(24.0).into()));
        assert_eq!(line_height_of("1.5rem"), Some(gpui::px(24.0).into()));
        assert_eq!(line_height_of("150%"), Some(gpui::relative(1.5)));
    }

    #[test]
    fn a_line_height_of_zero_or_less_declares_nothing() {
        assert_eq!(line_height_of("0"), None);
        assert_eq!(line_height_of("-1"), None);
        assert_eq!(line_height_of("-4px"), None);
    }

    #[test]
    fn calc_reaches_a_length() {
        let style = StyleDesc {
            padding: Some(crate::style::Numeric::Text(
                "calc(var(--spacing) * 6)".to_string(),
            )),
            ..Default::default()
        };
        let cascade = variables(&[("--spacing", "0.25rem")]);
        let resolved = Resolved::build(&style, &cascade);
        assert_eq!(resolved.base.padding.top, Some(gpui::px(24.0).into()));
    }
}

/// The GPUI cursor for a CSS `cursor` keyword. `auto` and unknown words set
/// nothing, so the element keeps the cursor of whatever it sits in.
pub(crate) fn cursor_style(name: &str) -> Option<gpui::CursorStyle> {
    use gpui::CursorStyle::*;
    Some(match name.trim() {
        "default" => Arrow,
        "pointer" => PointingHand,
        "text" => IBeam,
        "vertical-text" => IBeamCursorForVerticalLayout,
        "crosshair" => Crosshair,
        "grab" => OpenHand,
        "grabbing" => ClosedHand,
        "not-allowed" | "no-drop" => OperationNotAllowed,
        "col-resize" => ResizeColumn,
        "row-resize" => ResizeRow,
        "e-resize" => ResizeRight,
        "w-resize" => ResizeLeft,
        "n-resize" => ResizeUp,
        "s-resize" => ResizeDown,
        "ew-resize" => ResizeLeftRight,
        "ns-resize" => ResizeUpDown,
        "nesw-resize" | "ne-resize" | "sw-resize" => ResizeUpRightDownLeft,
        "nwse-resize" | "nw-resize" | "se-resize" => ResizeUpLeftDownRight,
        "alias" => DragLink,
        "copy" => DragCopy,
        "context-menu" => ContextualMenu,
        _ => return None,
    })
}
