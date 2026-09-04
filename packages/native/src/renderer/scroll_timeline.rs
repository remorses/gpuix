//! CSS scroll-driven animation timelines.
//!
//! A scroll box publishes a timeline with `scroll-timeline-name` and
//! `scroll-timeline-axis`, or the `scroll-timeline` shorthand. An element
//! points its animation at one with `animation-timeline`: a `--name`, or
//! the anonymous `scroll(nearest | self | root, axis)`.
//!
//! GPUIX has no stylesheet, so it has no `@keyframes`. The `motion` prop
//! is the keyframes source instead: `initial` is 0%, `animate` is 100%,
//! and `transition.ease` bends the progress. `transition.duration` and
//! `delay` play no part, because the scroll offset is the clock.
//!
//! Divergences from CSS, on purpose:
//! - A missing or `none` value keeps the clock. CSS `animation-timeline:
//!   none` freezes the animation, but the `motion` prop always animates
//!   unless a timeline takes over.
//! - A `--name` that no box declares holds the animation at progress 0.
//! - When several boxes declare one name, the nearest ancestor of the
//!   animated element wins, and the oldest element wins otherwise.

use std::collections::HashMap;

use gpui::ScrollHandle;

use crate::retained_tree::RetainedTree;
use crate::style::StyleDesc;

/// Which axis of which scroll box drives an animation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Timeline {
    pub(crate) scroller: u64,
    pub(crate) vertical: bool,
}

/// The words `scroll-timeline-axis` takes. `block` is vertical and
/// `inline` is horizontal, because GPUIX lays text out horizontally,
/// like `scroll-margin-block`.
fn axis_word(word: &str) -> Option<bool> {
    match word {
        "block" | "y" => Some(true),
        "inline" | "x" => Some(false),
        _ => None,
    }
}

/// The name and axis a box publishes. The longhands win over the
/// shorthand, like every CSS shorthand. Only a `--name` names a timeline.
fn declared(style: &StyleDesc) -> Option<(&str, bool)> {
    let mut name = None;
    let mut axis = None;
    if let Some(shorthand) = style.scroll_timeline.as_deref() {
        let mut parts = shorthand.split_whitespace();
        name = parts.next();
        axis = parts.next().and_then(axis_word);
    }
    if let Some(word) = style.scroll_timeline_name.as_deref() {
        name = Some(word.trim());
    }
    if let Some(word) = style.scroll_timeline_axis.as_deref() {
        axis = axis_word(word.trim()).or(axis);
    }
    let name = name.filter(|name| name.starts_with("--"))?;
    Some((name, axis.unwrap_or(true)))
}

/// Whether the style makes a scroll box on either axis.
fn scrolls(style: Option<&StyleDesc>) -> bool {
    let Some(style) = style else {
        return false;
    };
    let x = style.overflow_x.as_deref().or(style.overflow.as_deref());
    let y = style.overflow_y.as_deref().or(style.overflow.as_deref());
    let (x, y) = super::scrollbar::used_overflow(x, y);
    super::scrollbar::scrolls(x) || super::scrollbar::scrolls(y)
}

/// Whether this `animation-timeline` value asks for a scroll timeline at
/// all. Anything else leaves the `motion` prop on the clock.
pub(crate) fn requested(value: &str) -> bool {
    let value = value.trim();
    value.starts_with("--") || value.starts_with("scroll(")
}

/// The timeline the element's `animation-timeline` points at, or `None`
/// when the value resolves to no box. The caller treats `None` after
/// `requested` as progress 0.
pub(crate) fn resolve(tree: &RetainedTree, id: u64) -> Option<Timeline> {
    let style = |id: u64| tree.elements.get(&id).and_then(|el| el.style.as_deref());
    let parent = |id: u64| tree.elements.get(&id).and_then(|el| el.parent);
    let value = style(id)?.animation_timeline.as_deref()?.trim();

    if let Some(args) = value
        .strip_prefix("scroll(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let mut scroller_word = "nearest";
        let mut vertical = true;
        for word in args.split(|ch: char| ch.is_whitespace() || ch == ',') {
            if word.is_empty() {
                continue;
            }
            match axis_word(word) {
                Some(axis) => vertical = axis,
                None => scroller_word = word,
            }
        }
        let scroller = match scroller_word {
            "self" => Some(id).filter(|&id| scrolls(style(id))),
            "root" => {
                let mut found = None;
                let mut current = parent(id);
                while let Some(ancestor) = current {
                    if scrolls(style(ancestor)) {
                        found = Some(ancestor);
                    }
                    current = parent(ancestor);
                }
                found
            }
            _ => {
                let mut current = parent(id);
                loop {
                    let Some(ancestor) = current else {
                        break None;
                    };
                    if scrolls(style(ancestor)) {
                        break Some(ancestor);
                    }
                    current = parent(ancestor);
                }
            }
        }?;
        return Some(Timeline { scroller, vertical });
    }

    if !value.starts_with("--") {
        return None;
    }
    let mut current = parent(id);
    while let Some(ancestor) = current {
        if let Some((name, vertical)) = style(ancestor).and_then(declared) {
            if name == value {
                return Some(Timeline {
                    scroller: ancestor,
                    vertical,
                });
            }
        }
        current = parent(ancestor);
    }
    tree.elements
        .iter()
        .filter_map(|(&other, element)| {
            let (name, vertical) = declared(element.style.as_deref()?)?;
            (name == value).then_some((other, vertical))
        })
        .min_by_key(|(other, _)| *other)
        .map(|(scroller, vertical)| Timeline {
            scroller,
            vertical,
        })
}

/// How far the box has scrolled on the axis, from 0 to 1. A box whose
/// content fits, or that has not painted yet, reads 0.
pub(crate) fn progress(handles: &HashMap<u64, ScrollHandle>, timeline: Timeline) -> f64 {
    let Some(handle) = handles.get(&timeline.scroller) else {
        return 0.0;
    };
    let max = handle.max_offset();
    let offset = handle.offset();
    let (max, offset) = if timeline.vertical {
        (max.y, offset.y)
    } else {
        (max.x, offset.x)
    };
    if f32::from(max) <= 0.0 {
        return 0.0;
    }
    (f64::from(f32::from(-offset)) / f64::from(f32::from(max))).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retained_tree::RetainedElement;

    fn style(pairs: &[(&str, &str)]) -> StyleDesc {
        let mut style = StyleDesc::default();
        for (key, value) in pairs {
            match *key {
                "scrollTimeline" => style.scroll_timeline = Some(value.to_string()),
                "scrollTimelineName" => style.scroll_timeline_name = Some(value.to_string()),
                "scrollTimelineAxis" => style.scroll_timeline_axis = Some(value.to_string()),
                "animationTimeline" => style.animation_timeline = Some(value.to_string()),
                "overflowY" => style.overflow_y = Some(value.to_string()),
                "overflowX" => style.overflow_x = Some(value.to_string()),
                _ => unreachable!(),
            }
        }
        style
    }

    fn tree(elements: &[(u64, Option<u64>, StyleDesc)]) -> RetainedTree {
        let mut tree = RetainedTree::new();
        for (id, parent, style) in elements {
            let mut element = RetainedElement::new(*id, "div".to_string(), 0);
            element.style = Some(std::sync::Arc::new(style.clone()));
            element.parent = *parent;
            if let Some(parent) = parent {
                if let Some(parent) = tree.elements.get_mut(parent) {
                    parent.children.push(*id);
                }
            }
            tree.elements.insert(*id, element);
        }
        tree
    }

    #[test]
    fn a_named_timeline_prefers_the_nearest_ancestor() {
        let tree = tree(&[
            (1, None, style(&[("scrollTimelineName", "--p"), ("overflowY", "scroll")])),
            (2, Some(1), style(&[("scrollTimeline", "--p inline"), ("overflowX", "scroll")])),
            (3, Some(2), style(&[("animationTimeline", "--p")])),
            (4, None, style(&[("animationTimeline", "--missing")])),
        ]);
        assert_eq!(
            resolve(&tree, 3),
            Some(Timeline { scroller: 2, vertical: false })
        );
        assert_eq!(resolve(&tree, 4), None);
    }

    #[test]
    fn the_longhands_win_over_the_shorthand() {
        let longhands = style(&[
            ("scrollTimeline", "--a inline"),
            ("scrollTimelineName", "--b"),
            ("scrollTimelineAxis", "block"),
        ]);
        assert_eq!(declared(&longhands), Some(("--b", true)));
        let unnamed = style(&[("scrollTimelineName", "none")]);
        assert!(declared(&unnamed).is_none());
    }

    #[test]
    fn anonymous_scroll_finds_the_nearest_and_the_root_box() {
        let tree = tree(&[
            (1, None, style(&[("overflowY", "scroll")])),
            (2, Some(1), style(&[("overflowX", "scroll")])),
            (3, Some(2), style(&[("animationTimeline", "scroll(nearest x)")])),
            (4, Some(2), style(&[("animationTimeline", "scroll(root)")])),
            (5, Some(2), style(&[("animationTimeline", "scroll(self)")])),
        ]);
        assert_eq!(
            resolve(&tree, 3),
            Some(Timeline { scroller: 2, vertical: false })
        );
        assert_eq!(
            resolve(&tree, 4),
            Some(Timeline { scroller: 1, vertical: true })
        );
        assert_eq!(resolve(&tree, 5), None);
    }
}
