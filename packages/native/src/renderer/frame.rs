//! Building one frame of GPUI elements from the retained tree.
//!
//! GPUI is immediate mode, so every frame walks the retained tree and returns a
//! fresh element for each node. This module is that walk. It holds the context
//! the recursion threads through, the virtual list windowing that decides which
//! rows exist this frame, and the builder for each element type.

use std::collections::{HashMap, HashSet};

use super::virtual_list::{window_start_from_element, VirtualListConfig, VirtualListEntry};
use super::{emit_event_full, mouse_button_to_u32, point_to_xy, EventCallback, GpuixView};
use crate::custom_elements::{CustomElementRegistry, CustomRenderContext};
use crate::retained_tree::RetainedTree;
use crate::style::StyleDesc;
use crate::text::{selectable_text, SharedSelection};

/// Everything `build_element` threads through the tree.
///
/// Split into a struct because the recursion needs eight-plus shared references
/// and adding one more to every call site is how this file rots. `window` and
/// `cx` stay separate parameters: they are `&mut` and gpui reborrows them.
pub(super) struct BuildCtx<'a> {
    pub tree: &'a RetainedTree,
    pub event_callback: &'a Option<EventCallback>,
    pub focus_handles: &'a HashMap<u64, gpui::FocusHandle>,
    pub scroll_handles: &'a mut HashMap<u64, gpui::ScrollHandle>,
    pub custom_registry: &'a mut CustomElementRegistry,
    pub virtual_lists: &'a mut HashMap<u64, VirtualListEntry>,
    pub motion_states: &'a mut HashMap<u64, crate::motion::MotionState>,
    pub scrollbars: &'a mut super::scrollbar::States,
    pub now: std::time::Instant,
    pub motion_active: &'a mut bool,
    pub selection: SharedSelection,
    /// What this element inherits from its ancestors, resolved the way CSS
    /// inherits it. The renderer's own theme only seeds the root selection
    /// wash. Custom elements resolve their own theme from their `theme` prop.
    pub cascade: crate::inheritance::Inherited,
    /// The nearest ancestor's `highlight`, resolved. `None` in every app that
    /// does not use search. It carries the declaring element id, which is what
    /// a virtual-list row re-resolves against: that row is built after the root
    /// render returns, and on Windows and Linux the Node thread can edit text
    /// in between, so a stale range would paint over the wrong glyphs.
    pub highlight: Option<std::sync::Arc<crate::text::HighlightContext>>,
    /// Persistent `highlight` caches, keyed by the declaring element.
    pub highlights: &'a mut HashMap<u64, super::HighlightCacheEntry>,
    /// `onHighlight` payloads queued during the build.
    ///
    /// Never emitted inline: a handler that calls `setState` repaints, which
    /// would re-enter the build and emit again. They are flushed once the root
    /// build has returned.
    pub highlight_events: &'a mut Vec<(u64, usize)>,
    /// The running view transition, or `None`. Cleared inside the build of a
    /// frozen copy, so a name inside the copy never starts a nested one.
    pub vt: Option<&'a super::view_transition::VtState>,
    /// Whether this build is the frozen copy of a view transition. A copy is
    /// a still image, so it gets no scrollbar. The scrollbar defers its draw,
    /// and a deferred draw from inside the copy's isolated layout would read
    /// its layout ids against the window's tree and panic.
    pub frozen: bool,
    /// Rules the parent puts on its direct children, `& > *` and
    /// `& > :not(:last-child)`. They reach one depth only, so every element
    /// swaps in its own set, possibly empty, before it builds its children.
    /// The `bool` is the except-last flag.
    pub direct_rules: Vec<(bool, std::sync::Arc<gpui::StyleRefinement>)>,
    /// Rules for a whole subtree, `& *`. Pushed going down, cut back on
    /// return, so an element sees the rules of every ancestor above it.
    pub descendant_rules: Vec<std::sync::Arc<gpui::StyleRefinement>>,
}

// ── Element builders ─────────────────────────────────────────────────

pub(super) fn build_element(
    id: u64,
    // The child index and the child count under the parent, for the index
    // states. `None` at the root and under a virtual list, whose rows build
    // outside this walk.
    position: Option<(usize, usize)>,
    ctx: &mut BuildCtx,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    use gpui::IntoElement;

    let Some(element) = ctx.tree.elements.get(&id) else {
        return gpui::Empty.into_any_element();
    };

    // The motion frame for this element, or `None` when it does not animate.
    let motion = if let Some(source) = element.custom_props.get("motion") {
        let state = match ctx.motion_states.entry(id) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                match crate::motion::MotionState::new(source, ctx.now) {
                    Ok(state) => entry.insert(state),
                    Err(error) => {
                        log::warn!("Invalid motion description for element {id}: {error}");
                        entry.insert(crate::motion::MotionState::invalid(source, ctx.now))
                    }
                }
            }
        };
        if let Err(error) = state.sync(source, ctx.now) {
            log::warn!("Invalid motion update for element {id}: {error}");
        }
        state.is_valid().then(|| {
            let frame = state.frame(ctx.now);
            *ctx.motion_active |= frame.active;
            frame
        })
    } else {
        ctx.motion_states.remove(&id);
        None
    };
    let style = element.style.as_deref();

    // This frame of the view transition, when one runs and the element
    // carries a name. The opacity and blur of the arriving side fold into the
    // motion channel here, and the movement applies at paint in the wrapper
    // below.
    let vt_frame = ctx.vt.and_then(|vt| {
        let name = style?.view_transition_name.as_deref()?;
        if name.is_empty() || name == "none" {
            return None;
        }
        vt.frame_for(name)
    });
    let motion = match vt_frame
        .as_ref()
        .map(|frame| (frame.new_opacity(), frame.new_blur()))
    {
        Some((opacity, blur)) if opacity.is_some() || blur.is_some() => Some(match motion {
            Some(frame) => frame.with_view_transition(opacity, blur),
            None => crate::motion::MotionFrame::view_transition_frame(opacity, blur),
        }),
        _ => motion,
    };

    // Inheritable style resolves before the element's own style, because a
    // custom property declared here is in scope for the `var()` next to it.
    let parent_cascade = ctx.cascade.clone();
    ctx.cascade = element.descend(&parent_cascade);

    // A `highlight` here replaces any ancestor's: the nearest declaration wins,
    // and `GroupList::collect` skips nested declarations so an ancestor never
    // resolves or counts matches that will not paint.
    let parent_highlight = ctx.highlight.clone();
    if let Some(value) = element.custom_props.get("highlight") {
        let has_listener = element.events.contains("highlight");
        let resolved = super::resolve_highlight(
            ctx.highlights,
            ctx.tree,
            id,
            value,
            &crate::theme::Theme::dark(),
            has_listener,
        );
        if let Some((_, Some(total))) = &resolved {
            ctx.highlight_events.push((id, *total));
        }
        ctx.highlight = resolved.map(|(context, _)| context);
    }

    // Resolve the style into a GPUI StyleRefinement. GPUI rebuilds its element
    // tree every frame, so this is the work that used to repeat every frame for
    // styles that had not changed. An animated element reads the same cache,
    // because its motion frame lands on the sink rather than on the style it
    // resolves from.
    let resolved = element.resolved_style(&ctx.cascade);

    let built = match element.element_type.as_str() {
        // `<text>` is a `<div>` that happens to carry a string. Giving it its
        // own builder meant every interaction prop on the shared `Props` type
        // (onClick, hover, focus, tabIndex) type-checked, registered a JS
        // listener, and then silently did nothing.
        "div" | "text" => {
            ctx.custom_registry.destroy(id);
            build_host_container(
                element,
                style,
                resolved.clone(),
                motion.as_ref(),
                position,
                ctx,
                window,
                cx,
            )
        }
        "virtual-list" => {
            ctx.custom_registry.destroy(id);
            build_virtual_list(element, ctx, window, cx)
        }

        // Polymorphic dispatch for all custom elements.
        custom_type => {
            // Custom renderers take a `StyleDesc` and resolve it themselves, so
            // a motion frame reaches them folded into one. They are the only
            // callers that still pay for that fold.
            // `Arc<StyleDesc>` is shared, so the animated frame is applied to a
            // copy. Mutating through the pointer would restyle every element
            // that declared the same style.
            let animated = motion.as_ref().map(|frame| {
                let mut declared = element.style.as_deref().cloned().unwrap_or_default();
                frame.style.apply_to(&mut declared);
                declared
            });
            let style = animated.as_ref().or(style);
            // A custom element renders its own box, so a parent's direct
            // child rules stop here, and its children start a new depth.
            let saved_direct = std::mem::take(&mut ctx.direct_rules);
            let present: Vec<u64> = element
                .children
                .iter()
                .copied()
                .filter(|child_id| ctx.tree.elements.contains_key(child_id))
                .collect();
            let positions = child_positions(ctx.tree, &present);
            let custom_children: Vec<gpui::AnyElement> = present
                .into_iter()
                .zip(positions)
                .map(|(child_id, position)| build_element(child_id, position, ctx, window, cx))
                .collect();
            ctx.direct_rules = saved_direct;
            let cascade = ctx.cascade.clone();
            let render_ctx = CustomRenderContext {
                id,
                events: &element.events,
                event_callback: ctx.event_callback,
                focus_handle: ctx.focus_handles.get(&id),
                style,
                children: custom_children,
                selection: ctx.selection.clone(),
                selectable: cascade.selectable(),
                selection_wash: crate::color::to_hsla(cascade.selection_wash()),
                highlight_set: ctx.highlight.clone(),
                cascade: cascade.clone(),
            };
            ctx.custom_registry
                .render(custom_type, &element.custom_props, render_ctx, window, cx)
        }
    };

    let built = super::auto_height::wrap(id, built, motion.as_ref(), resolved.as_deref());
    let built = match vt_frame {
        Some(frame) => super::view_transition::wrap(element, built, frame, ctx, window, cx),
        None => built,
    };

    ctx.cascade = parent_cascade;
    ctx.highlight = parent_highlight;
    built
}

fn build_virtual_list(
    element: &crate::retained_tree::RetainedElement,
    ctx: &mut BuildCtx,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    let child_ids: Vec<u64> = element
        .children
        .iter()
        .copied()
        .filter(|child_id| ctx.tree.elements.contains_key(child_id))
        .collect();
    let child_revisions: Vec<u64> = child_ids
        .iter()
        .filter_map(|child_id| {
            ctx.tree
                .elements
                .get(child_id)
                .map(|child| child.subtree_revision)
        })
        .collect();
    let focusable_rows: HashSet<u64> = ctx
        .focus_handles
        .keys()
        .filter_map(|element_id| virtual_row_ancestor(ctx.tree, element.id, *element_id))
        .collect();
    let focused_row = ctx
        .focus_handles
        .iter()
        .find_map(|(element_id, handle)| {
            handle
                .is_focused(window)
                .then(|| virtual_row_ancestor(ctx.tree, element.id, *element_id))
                .flatten()
        })
        .or_else(|| {
            ctx.focus_handles.keys().find_map(|element_id| {
                ctx.tree
                    .elements
                    .get(element_id)
                    .is_some_and(|element| element.auto_focus)
                    .then(|| virtual_row_ancestor(ctx.tree, element.id, *element_id))
                    .flatten()
            })
        });
    let config = VirtualListConfig::from_element(element);
    let window_start = if config.item_count.is_some() {
        window_start_from_element(element)
    } else {
        0
    };
    let list_state = match ctx.virtual_lists.entry(element.id) {
        std::collections::hash_map::Entry::Occupied(mut entry) => {
            entry.get_mut().sync(
                config,
                window_start,
                child_ids.clone(),
                child_revisions,
                &focusable_rows,
                cx,
            );
            let entry = entry.into_mut();
            if let Some(row_id) = focused_row.filter(|row_id| !entry.seen_rows.contains(row_id)) {
                if let Some(index) = entry.logical_index_of(row_id) {
                    entry.state.scroll_to(gpui::ListOffset {
                        item_ix: index,
                        offset_in_item: gpui::px(0.0),
                    });
                }
            }
            entry.state.clone()
        }
        std::collections::hash_map::Entry::Vacant(entry) => {
            let row_focus_handles = child_ids
                .iter()
                .map(|id| focusable_rows.contains(id).then(|| cx.focus_handle()))
                .collect();
            let entry = entry.insert(VirtualListEntry::new(
                config,
                window_start,
                child_ids.clone(),
                child_revisions,
                row_focus_handles,
            ));
            if let Some(row_id) = focused_row {
                if let Some(index) = entry.logical_index_of(row_id) {
                    entry.state.scroll_to(gpui::ListOffset {
                        item_ix: index,
                        offset_in_item: gpui::px(0.0),
                    });
                }
            }
            entry.state.clone()
        }
    };

    // Queued scrolls apply here, after `sync` spliced this frame's child
    // changes, so the indices JS computed against its committed child list are
    // the indices the splice-adjusted ListState sees.
    if let Some(offset) =
        super::PENDING_VIRTUAL_LIST_SCROLLS.with(|cell| cell.borrow_mut().remove(&element.id))
    {
        list_state.scroll_to(offset);
    }

    if element.events.contains("visibleRange") {
        let callback = ctx.event_callback.clone();
        let list_id = element.id;
        list_state.set_scroll_handler(move |event, _window, _cx| {
            emit_event_full(&callback, list_id, "visibleRange", |payload| {
                payload.start_index = Some(event.visible_range.start as f64);
                payload.end_index = Some(event.visible_range.end as f64);
            });
        });
    }

    let list_id = element.id;
    // Cloned, not copied: gpui runs this processor once per requested row, so
    // the captured value must survive every call.
    let cascade = ctx.cascade.clone();
    let highlight = ctx.highlight.clone();
    let render_item = cx.processor(move |view, index: usize, window, cx| {
        let Some(entry) = view.virtual_lists.get(&list_id) else {
            return unmounted_virtual_row(1.0);
        };
        let Some(child_id) = entry.child_at(index) else {
            // Empty measures as 0 and poisons ListState. Keep the estimate.
            return unmounted_virtual_row(entry.config.estimated_item_height.unwrap_or(1.0));
        };
        view.build_virtual_child(
            list_id,
            index,
            child_id,
            cascade.clone(),
            highlight.clone(),
            window,
            cx,
        )
    });
    let mut list =
        gpui::list(list_state, render_item).with_sizing_behavior(gpui::ListSizingBehavior::Auto);
    if let Some(resolved) = element.resolved_style(&ctx.cascade) {
        list = crate::style::resolve::apply_resolved(list, &resolved.base);
    }
    list.into_any_element()
}

pub(super) fn unmounted_virtual_row(height: f32) -> gpui::AnyElement {
    use gpui::prelude::*;
    gpui::div().h(gpui::px(height.max(1.0))).w_full().into_any()
}

fn virtual_row_ancestor(tree: &RetainedTree, list_id: u64, element_id: u64) -> Option<u64> {
    let mut current = element_id;
    loop {
        let parent = tree.elements.get(&current)?.parent?;
        if parent == list_id {
            return Some(current);
        }
        current = parent;
    }
}

/// The one builder for `<div>` and `<text>`.
///
/// Both get the same stable GPUI id, so gpui keeps their interactive element
/// state (hover, active, pointer capture, scroll, accessibility node) across
/// frames, and both wire the whole shared `Props` surface.
pub(crate) fn build_host_container(
    element: &crate::retained_tree::RetainedElement,
    style: Option<&StyleDesc>,
    resolved: Option<std::sync::Arc<crate::style::resolve::Resolved>>,
    motion: Option<&crate::motion::MotionFrame>,
    position: Option<(usize, usize)>,
    ctx: &mut BuildCtx,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    // `ElementId::Integer` rather than a formatted name: host ids are already
    // unique per renderer, and every `<div>` and `<text>` builds one of these on
    // every frame, so the string allocation was pure overhead. Custom elements
    // use `ElementId::Name`, which is a different variant and cannot collide.
    let mut el = gpui::div().id(gpui::ElementId::Integer(element.id));

    // A raw text node is not an element to CSS, so no rule reaches it.
    if !is_raw_text(element) {
        el = apply_child_rules(el, position, ctx);
    }

    if let Some(resolved) = resolved.as_ref() {
        el = crate::style::resolve::apply_resolved(el, &resolved.base);

        // State pseudo-classes. GPUI evaluates hover and active itself, so
        // neither waits for React. Each takes a closure that receives a
        // StyleRefinement and returns it, and the closure has to be 'static,
        // so each one holds a clone of the shared resolved style.
        //
        // This loop is the dispatcher for states. Adding one is a variant on
        // `State` and an arm here.
        use crate::style::resolve::State;
        // Collect the tags first. Iterating `states` directly would borrow
        // `resolved` across the closures, and cloning the pairs would copy a
        // whole refinement per state to read a one-byte tag.
        let states: Vec<State> = resolved.states.iter().map(|(state, _)| *state).collect();
        for state in states {
            // An index state is a fact of the child position, decided here
            // rather than through a GPUI variant. Without a position (the
            // root, a virtual list row) there is nothing to decide against,
            // so it does not apply.
            if state.is_index() {
                let holds =
                    position.is_some_and(|(index, count)| state.holds_at(index, count));
                if holds {
                    if let Some(declared) = resolved.state(state) {
                        el = crate::style::resolve::apply_resolved(el, declared);
                    }
                }
                continue;
            }
            let held = resolved.clone();
            let apply = move |refinement: gpui::StyleRefinement| match held.state(state) {
                Some(declared) => crate::style::resolve::apply_resolved(refinement, declared),
                None => refinement,
            };
            el = match state {
                State::Hover => el.hover(apply),
                State::Active => el.active(apply),
                State::First | State::Last | State::Odd | State::Even | State::Only => el,
            };
        }
    }

    if let Some(motion) = motion {
        el = crate::style::resolve::apply_motion(el, motion, style);
    }

    if let Some(style) = style {
        if crate::style::should_occlude(style) {
            // BlockMouse (occlude) stops the hit test, so the parent scroller
            // never sees the wheel. HTML does not work that way: a wheel over
            // an absolutely positioned card still scrolls the ancestor. Only
            // `pointerEvents: "auto"` opts into stealing it. Everything else
            // uses BlockMouseExceptScroll.
            //
            // Absolute used to steal it too. That made a pannable canvas
            // impossible: every absolutely placed item (a timeline clip, a
            // graph node) ended the hit test before the pan listener ran.
            // `<anchored>` still occludes through its own `occlude` prop, so
            // menus and tooltips are unaffected.
            el = if style.pointer_events.as_deref() == Some("auto") {
                el.occlude()
            } else {
                el.block_mouse_except_scroll()
            };
        }
    }

    // ── Overflow: scroll ─────────────────────────────────────────────
    // overflow_scroll() requires StatefulInteractiveElement (only on Stateful<Div>),
    // so we handle it here rather than in apply_styles (which takes E: Styled).
    //
    // CSS precedence: axis-specific props (overflowX/Y) override the shorthand
    // (overflow). E.g. { overflow: "scroll", overflowY: "hidden" } → scroll X only.
    //
    // overflow-x only works as a flex viewport. Default display is Block, so a
    // wide child fills the parent instead of overflowing. Zed's code-block path:
    // flex + min_w_0 on the scroller, flex_none on the child.
    let mut overflow_x_only = false;
    let mut scrollbar = None;
    if let Some(style) = style {
        // Resolve each axis: axis-specific overrides shorthand.
        let resolved_x = style.overflow_x.as_deref().or(style.overflow.as_deref());
        let resolved_y = style.overflow_y.as_deref().or(style.overflow.as_deref());
        let (resolved_x, resolved_y) = super::scrollbar::used_overflow(resolved_x, resolved_y);

        let needs_scroll_x = super::scrollbar::scrolls(resolved_x);
        let needs_scroll_y = super::scrollbar::scrolls(resolved_y);

        if needs_scroll_x && needs_scroll_y {
            el = el.overflow_scroll();
            // GPUI zeroes the smaller of the two deltas by default, so one
            // diagonal wheel moves one axis. A browser moves both, and a
            // two-axis container is exactly where a user expects that.
            el.style().allow_concurrent_scroll = Some(true);
        } else if needs_scroll_x {
            overflow_x_only = true;
            el = el
                .flex()
                .min_w_0()
                .overflow_x_scroll()
                .restrict_scroll_to_axis();
        } else if needs_scroll_y {
            el = el.overflow_y_scroll();
        }

        // Attach a persistent ScrollHandle when scrolling is enabled.
        // The handle persists across renders (stored in GpuixView::scroll_handles)
        // so GPUI maintains the scroll offset between frames.
        if needs_scroll_x || needs_scroll_y {
            let handle = ctx
                .scroll_handles
                .entry(element.id)
                .or_insert_with(gpui::ScrollHandle::new);
            el = el.track_scroll(handle);

            // The scrollbar. Classic bars reserve a gutter in the layout,
            // which taffy takes as one width for both axes. A frozen view
            // transition copy gets none: see `BuildCtx::frozen`.
            let mode = super::scrollbar::Mode::current(cx);
            if let Some(spec) =
                super::scrollbar::Spec::from_style(style, mode).filter(|_| !ctx.frozen)
            {
                let state = ctx.scrollbars.entry(element.id).or_default().clone();
                let reserved = spec.reserved(state.borrow().overflowed);
                let gutter = reserved.x.max(reserved.y);
                if gutter > gpui::px(0.0) {
                    el = el.scrollbar_width(gutter);
                    if spec.both_edges() {
                        let padding = &mut el.style().padding;
                        if needs_scroll_y {
                            padding.left = Some(add_pixels(padding.left, gutter));
                        }
                        if needs_scroll_x {
                            padding.top = Some(add_pixels(padding.top, gutter));
                        }
                    }
                }
                scrollbar = Some(super::scrollbar::Scrollbar::new(
                    spec,
                    handle.clone(),
                    state,
                    ctx.now,
                ));
            }
        } else {
            // Element is no longer scrollable — remove stale handle.
            ctx.scroll_handles.remove(&element.id);
            ctx.scrollbars.remove(&element.id);
        }
    } else {
        // No style at all — remove stale handle if it existed.
        ctx.scroll_handles.remove(&element.id);
        ctx.scrollbars.remove(&element.id);
    }

    // If a FocusHandle was pre-created for this element (by sync_focus_handles),
    // attach it via track_focus. This makes the element focusable — clicking it
    // or tabbing to it gives it keyboard focus. The handle persists across renders
    // because it's stored in GpuixView::focus_handles.
    if style.and_then(|style| style.position.as_deref()).is_none() {
        el = el.relative();
    }
    el = el.child(crate::automation::bounds_tracker(
        element.id,
        selection_start_flag(style),
        ctx.scroll_handles.get(&element.id).cloned(),
    ));

    if let Some(handle) = ctx.focus_handles.get(&element.id) {
        el = el.track_focus(handle);
    }
    if let Some(tab_index) = element
        .custom_props
        .get("tabIndex")
        .and_then(|value| value.as_i64())
        .and_then(|index| isize::try_from(index).ok())
    {
        el = el.tab_index(tab_index).tab_stop(tab_index >= 0);
    }

    if element.events.contains("click") {
        let id = element.id;
        let callback = ctx.event_callback.clone();
        // GPUI's higher-level on_click gesture is not finalized by the
        // embedded macOS pump. Bubble listeners run in reverse registration
        // order, so attach click first to keep onMouseUp ahead of onClick.
        el = el.on_mouse_up(gpui::MouseButton::Left, move |mouse_event, _window, _cx| {
            emit_event_full(&callback, id, "click", |p| {
                let (x, y) = point_to_xy(mouse_event.position);
                p.x = Some(x);
                p.y = Some(y);
                p.button = Some(0);
                p.modifiers = Some(mouse_event.modifiers.into());
                p.click_count = Some(mouse_event.click_count as u32);
                p.is_right_click = Some(false);
            });
        });
    }

    // Wire up events.
    // Some events (on_hover, on_aux_click) require a stateful element (.id()),
    // which we already set above. Others (on_mouse_down, on_key_down) work
    // on any InteractiveElement.
    for event_type in &element.events {
        let id = element.id;
        let callback = ctx.event_callback.clone();
        match event_type.as_str() {
            // ── Aux click (non-primary), like the DOM `auxclick` ──
            "auxClick" => {
                el = el.on_aux_click(move |click_event, _window, _cx| {
                    emit_event_full(&callback, id, "auxClick", |p| {
                        let (x, y) = point_to_xy(click_event.position());
                        p.x = Some(x);
                        p.y = Some(y);
                        p.modifiers = Some(click_event.modifiers().into());
                        p.click_count = Some(click_event.click_count() as u32);
                        p.is_right_click = Some(click_event.is_right_click());
                    });
                });
            }

            // ── Mouse down (all buttons) ─────────────────────────
            "mouseDown" => {
                // Wire all three buttons so JS gets right-click, middle-click, etc.
                for &button in &[
                    gpui::MouseButton::Left,
                    gpui::MouseButton::Middle,
                    gpui::MouseButton::Right,
                ] {
                    let callback = callback.clone();
                    el = el.on_mouse_down(button, move |mouse_event, _window, _cx| {
                        emit_event_full(&callback, id, "mouseDown", |p| {
                            let (x, y) = point_to_xy(mouse_event.position);
                            p.x = Some(x);
                            p.y = Some(y);
                            p.button = Some(mouse_button_to_u32(mouse_event.button));
                            p.click_count = Some(mouse_event.click_count as u32);
                            p.modifiers = Some(mouse_event.modifiers.into());
                        });
                    });
                }
            }

            // ── Mouse up (all buttons) ───────────────────────────
            "mouseUp" => {
                for &button in &[
                    gpui::MouseButton::Left,
                    gpui::MouseButton::Middle,
                    gpui::MouseButton::Right,
                ] {
                    let callback = callback.clone();
                    el = el.on_mouse_up(button, move |mouse_event, _window, _cx| {
                        emit_event_full(&callback, id, "mouseUp", |p| {
                            let (x, y) = point_to_xy(mouse_event.position);
                            p.x = Some(x);
                            p.y = Some(y);
                            p.button = Some(mouse_button_to_u32(mouse_event.button));
                            p.click_count = Some(mouse_event.click_count as u32);
                            p.modifiers = Some(mouse_event.modifiers.into());
                        });
                    });
                }
            }

            // ── Mouse move ───────────────────────────────────────
            "mouseMove" => {
                el = el.on_mouse_move(move |mouse_event, _window, _cx| {
                    emit_event_full(&callback, id, "mouseMove", |p| {
                        let (x, y) = point_to_xy(mouse_event.position);
                        p.x = Some(x);
                        p.y = Some(y);
                        p.modifiers = Some(mouse_event.modifiers.into());
                        p.pressed_button = mouse_event.pressed_button.map(mouse_button_to_u32);
                    });
                });
            }

            // ── Hover (mouseEnter + mouseLeave) ──────────────────
            // GPUI's on_hover fires with true on enter, false on leave.
            // We split into two distinct event types for the React side.
            "mouseEnter" | "mouseLeave" => {
                // Only wire once even if both mouseEnter and mouseLeave are registered.
                // Check if we already wired on_hover via the other event.
                let has_enter = element.events.contains("mouseEnter");
                let has_leave = element.events.contains("mouseLeave");
                // Wire on first encounter (mouseEnter sorts before mouseLeave).
                if event_type.as_str() == "mouseEnter" || !has_enter {
                    let callback_enter = if has_enter {
                        ctx.event_callback.clone()
                    } else {
                        None
                    };
                    let callback_leave = if has_leave {
                        ctx.event_callback.clone()
                    } else {
                        None
                    };
                    el = el.on_hover(move |&is_hovered, _window, _cx| {
                        if is_hovered {
                            emit_event_full(&callback_enter, id, "mouseEnter", |p| {
                                p.hovered = Some(true);
                            });
                        } else {
                            emit_event_full(&callback_leave, id, "mouseLeave", |p| {
                                p.hovered = Some(false);
                            });
                        }
                    });
                }
            }

            // ── Mouse down outside ───────────────────────────────
            // Fires when the user clicks OUTSIDE this element.
            // Critical for "click outside to close" pattern (dropdowns, modals).
            "mouseDownOutside" => {
                el = el.on_mouse_down_out(move |mouse_event, _window, _cx| {
                    emit_event_full(&callback, id, "mouseDownOutside", |p| {
                        let (x, y) = point_to_xy(mouse_event.position);
                        p.x = Some(x);
                        p.y = Some(y);
                        p.button = Some(mouse_button_to_u32(mouse_event.button));
                        p.modifiers = Some(mouse_event.modifiers.into());
                    });
                });
            }

            // ── Scroll wheel ─────────────────────────────────────
            "scroll" => {
                el = el.on_scroll_wheel(move |scroll_event, _window, _cx| {
                    emit_event_full(&callback, id, "scroll", |p| {
                        let (x, y) = point_to_xy(scroll_event.position);
                        p.x = Some(x);
                        p.y = Some(y);
                        p.modifiers = Some(scroll_event.modifiers.into());
                        p.precise = Some(scroll_event.delta.precise());

                        // Convert ScrollDelta to pixel values.
                        // For Lines delta, we use a default line height of 20px.
                        let line_height = gpui::px(20.0);
                        let pixel_delta = scroll_event.delta.pixel_delta(line_height);
                        p.delta_x = Some(f64::from(f32::from(pixel_delta.x)));
                        p.delta_y = Some(f64::from(f32::from(pixel_delta.y)));

                        p.touch_phase = Some(match scroll_event.touch_phase {
                            gpui::TouchPhase::Started => "started".to_string(),
                            gpui::TouchPhase::Moved => "moved".to_string(),
                            gpui::TouchPhase::Ended => "ended".to_string(),
                            gpui::TouchPhase::Cancelled => "cancelled".to_string(),
                        });
                    });
                });
            }

            // ── Key down ─────────────────────────────────────────
            // Requires .focusable() (set above). Element must be focused
            // (clicked or tabbed to) for these to fire.
            "keyDown" => {
                el = el.on_key_down(move |key_event, _window, _cx| {
                    emit_event_full(&callback, id, "keyDown", |p| {
                        p.key = Some(key_event.keystroke.key.clone());
                        p.key_char = key_event.keystroke.key_char.clone();
                        p.is_held = Some(key_event.is_held);
                        p.modifiers = Some(key_event.keystroke.modifiers.into());
                    });
                });
            }

            // ── Key up ───────────────────────────────────────────
            "keyUp" => {
                el = el.on_key_up(move |key_event, _window, _cx| {
                    emit_event_full(&callback, id, "keyUp", |p| {
                        p.key = Some(key_event.keystroke.key.clone());
                        p.key_char = key_event.keystroke.key_char.clone();
                        p.modifiers = Some(key_event.keystroke.modifiers.into());
                    });
                });
            }

            // ── Focus / Blur ─────────────────────────────────────
            // Event emission is handled by FocusHandle subscriptions
            // set up in GpuixView::sync_focus_handles(). The handle is
            // attached to this element via .track_focus() above.
            "focus" | "blur" => {}

            _ => {}
        }
    }

    if element.events.contains("mouseDown") && element.events.contains("mouseMove") {
        el = el.capture_pointer();
    }

    // Text content, selectable, same as a <text> leaf.
    if let Some(ref content) = element.content {
        el = el.child(text_content(element, content, ctx));
    }

    // Children. The parent's direct child rules reach this depth only, so
    // the element swaps in its own set here, and its subtree rules join the
    // descendant stack until the loop returns.
    let (saved_direct, pushed) = push_child_rules(resolved.as_deref(), ctx);
    let child_ids: Vec<u64> = element
        .children
        .iter()
        .copied()
        .filter(|child_id| ctx.tree.elements.contains_key(child_id))
        .collect();
    let positions = child_positions(ctx.tree, &child_ids);
    for (child_id, position) in child_ids.into_iter().zip(positions) {
        let child = build_element(child_id, position, ctx, window, cx);
        el = if overflow_x_only {
            el.child(gpui::div().flex_none().child(child))
        } else {
            el.child(child)
        };
    }
    pop_child_rules(saved_direct, pushed, ctx);

    // Last, so it paints over the content and takes the mouse first.
    if let Some(scrollbar) = scrollbar {
        el = el.child(scrollbar);
    }

    el.into_any_element()
}

/// `length` plus `extra`. A pixel length adds. Any other unit gives way,
/// because the sum would need the box's size to resolve.
fn add_pixels(length: Option<gpui::DefiniteLength>, extra: gpui::Pixels) -> gpui::DefiniteLength {
    match length {
        Some(gpui::DefiniteLength::Absolute(gpui::AbsoluteLength::Pixels(pixels))) => {
            (pixels + extra).into()
        }
        _ => extra.into(),
    }
}

/// Whether this element is the anonymous node the reconciler makes for a raw
/// string child. The web gives a text node no box of its own: `*` never
/// matches one, and `:nth-child` does not count one.
fn is_raw_text(element: &crate::retained_tree::RetainedElement) -> bool {
    element.element_type == "text" && element.content.is_some() && element.style.is_none()
}

/// The `:nth-child` position of each child. Raw text nodes get `None` and do
/// not count.
fn child_positions(tree: &RetainedTree, child_ids: &[u64]) -> Vec<Option<(usize, usize)>> {
    let raw: Vec<bool> = child_ids
        .iter()
        .map(|child_id| tree.elements.get(child_id).is_some_and(is_raw_text))
        .collect();
    let count = raw.iter().filter(|flag| !**flag).count();
    let mut index = 0;
    raw.into_iter()
        .map(|flag| {
            if flag {
                return None;
            }
            let position = Some((index, count));
            index += 1;
            position
        })
        .collect()
}

/// Merge the rules ancestors put on this element, under its own declarations.
///
/// `:where()` has specificity zero, so these run before the element's own
/// refinement, and the element's own set fields win. Descendant rules come
/// first, then the parent's direct rules, so the nearer declaration wins a
/// conflict between the two.
fn apply_child_rules<E: gpui::Styled>(
    mut el: E,
    position: Option<(usize, usize)>,
    ctx: &BuildCtx,
) -> E {
    for refinement in &ctx.descendant_rules {
        el = crate::style::resolve::apply_resolved(el, refinement);
    }
    let Some((index, count)) = position else {
        return el;
    };
    let last = index + 1 == count;
    for (except_last, refinement) in &ctx.direct_rules {
        if *except_last && last {
            continue;
        }
        el = crate::style::resolve::apply_resolved(el, refinement);
    }
    el
}

/// Install this element's child rules for the walk below it.
///
/// Returns the parent's direct rules to restore, and how many descendant
/// rules to cut back, both through `pop_child_rules`.
fn push_child_rules(
    resolved: Option<&crate::style::resolve::Resolved>,
    ctx: &mut BuildCtx,
) -> (Vec<(bool, std::sync::Arc<gpui::StyleRefinement>)>, usize) {
    use crate::style::resolve::ChildScope;
    let mut direct = Vec::new();
    let mut pushed = 0;
    let rules = resolved.map(|resolved| resolved.children.as_slice()).unwrap_or(&[]);
    for (which, refinement) in rules {
        match which {
            ChildScope::All => direct.push((false, refinement.clone())),
            ChildScope::ExceptLast => direct.push((true, refinement.clone())),
            ChildScope::Descendants => {
                ctx.descendant_rules.push(refinement.clone());
                pushed += 1;
            }
        }
    }
    (std::mem::replace(&mut ctx.direct_rules, direct), pushed)
}

fn pop_child_rules(
    saved_direct: Vec<(bool, std::sync::Arc<gpui::StyleRefinement>)>,
    pushed: usize,
    ctx: &mut BuildCtx,
) {
    ctx.direct_rules = saved_direct;
    let keep = ctx.descendant_rules.len() - pushed;
    ctx.descendant_rules.truncate(keep);
}

/// A selectable text run owned by `element`. Runs are left to gpui so the
/// text keeps inheriting colour, weight and family from ancestor styles.
///
/// The run's group is its parent host element, because React makes a separate
/// host node for every interpolated string. `<text>Hello {name}!</text>` is one
/// logical line painted as three runs that all share the parent's id.
/// A `userSelect: "none"` run still paints highlight washes, because a browser
/// still finds that text with Ctrl+F. Element chrome that must never be found,
/// such as a code gutter, uses `chrome_text` instead.
fn text_content(
    element: &crate::retained_tree::RetainedElement,
    content: &str,
    ctx: &BuildCtx,
) -> gpui::AnyElement {
    let text = crate::text::SelectableText::new(
        element.id,
        0,
        gpui::SharedString::from(content.to_string()),
        None,
        ctx.selection.clone(),
        crate::color::to_hsla(ctx.cascade.selection_wash()),
    );
    selectable_text(crate::text::SelectableText {
        group: crate::text::search::group_id(ctx.tree, element.id),
        selectable: ctx.cascade.selectable(),
        highlight: ctx
            .highlight
            .clone()
            .map(crate::text::HighlightSource::Resolved),
        cursor: text.cursor.filter(|_| !ctx.cascade.cursor_declared()),
        ..text
    })
}

/// Explicit `userSelect` on this node. `None` means inherit; the ancestor
/// that set the value already owns the start region.
fn selection_start_flag(style: Option<&StyleDesc>) -> Option<bool> {
    match style.and_then(|style| style.user_select.as_deref()) {
        Some("none") => Some(false),
        Some("text") | Some("auto") => Some(true),
        _ => None,
    }
}
