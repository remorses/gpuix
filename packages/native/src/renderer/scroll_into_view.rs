//! CSS `scrollIntoView` with `scroll-margin` and `scroll-padding`.
//!
//! Each ancestor scroll box scrolls in turn, nearest first, the way a
//! browser walks the chain. The `scroll-margin` of the element grows
//! the rectangle a box brings into view. The `scroll-padding` of the
//! box shrinks the viewport the rectangle lands in. Both take a number
//! of pixels or `"Npx"` text, alone or as the CSS one-to-four
//! shorthand. The bounds come from the last painted frame.

use gpui::{point, px, Point, ScrollHandle};

use crate::retained_tree::RetainedTree;
use crate::style::{Numeric, StyleDesc};

/// Where the element lands in the viewport on one axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Align {
    Start,
    Center,
    End,
    Nearest,
}

impl Align {
    /// The `scrollIntoView` words `start`, `center`, `end` and
    /// `nearest`. An unknown word falls back to `default`.
    pub(crate) fn parse(word: Option<&str>, default: Align) -> Align {
        match word.map(str::trim) {
            Some("start") => Align::Start,
            Some("center") => Align::Center,
            Some("end") => Align::End,
            Some("nearest") => Align::Nearest,
            _ => default,
        }
    }
}

/// An absolute rectangle, as two corners.
#[derive(Clone, Copy)]
struct Rect {
    start: Point<f32>,
    end: Point<f32>,
}

impl Rect {
    fn from_bounds(bounds: crate::automation::ElementBounds) -> Self {
        let start = point(bounds.x as f32, bounds.y as f32);
        Self {
            start,
            end: point(
                start.x + bounds.width as f32,
                start.y + bounds.height as f32,
            ),
        }
    }
}

/// One length: a number of pixels or `"Npx"`. Anything else is zero.
fn length(value: &str) -> f32 {
    let value = value.trim();
    let value = value.strip_suffix("px").unwrap_or(value);
    value.trim().parse().unwrap_or(0.0)
}

/// The CSS one-to-four shorthand, as top, right, bottom and left.
fn shorthand(value: Option<&Numeric>) -> [f32; 4] {
    let words: Vec<f32> = match value {
        None => return [0.0; 4],
        Some(Numeric::Number(number)) => return [*number as f32; 4],
        Some(Numeric::Text(text)) => text.split_whitespace().map(length).collect(),
    };
    match words[..] {
        [all] => [all; 4],
        [vertical, horizontal] => [vertical, horizontal, vertical, horizontal],
        [top, horizontal, bottom] => [top, horizontal, bottom, horizontal],
        [top, right, bottom, left] => [top, right, bottom, left],
        _ => [0.0; 4],
    }
}

/// One declared length, or `None` when the property is not set.
fn declared(value: Option<&Numeric>) -> Option<f32> {
    match value {
        None => None,
        Some(Numeric::Number(number)) => Some(*number as f32),
        Some(Numeric::Text(text)) => Some(length(text)),
    }
}

/// The one-or-two shorthand of a logical pair, as start and end.
fn pair(value: Option<&Numeric>) -> [Option<f32>; 2] {
    let words: Vec<f32> = match value {
        None => return [None; 2],
        Some(Numeric::Number(number)) => return [Some(*number as f32); 2],
        Some(Numeric::Text(text)) => text.split_whitespace().map(length).collect(),
    };
    match words[..] {
        [both] => [Some(both); 2],
        [start, end] => [Some(start), Some(end)],
        _ => [None; 2],
    }
}

/// One side: the physical longhand first, then the logical longhand, then
/// the logical shorthand, then the physical shorthand. GPUIX lays text out
/// horizontally, left to right, so block is vertical and inline is
/// horizontal.
fn side(physical: Option<&Numeric>, logical: Option<&Numeric>, of_pair: Option<f32>, short: f32) -> f32 {
    declared(physical)
        .or_else(|| declared(logical))
        .or(of_pair)
        .unwrap_or(short)
}

/// The `scroll-margin` of the element, as top, right, bottom and left.
pub(crate) fn scroll_margin(style: Option<&StyleDesc>) -> [f32; 4] {
    let Some(style) = style else { return [0.0; 4] };
    let base = shorthand(style.scroll_margin.as_ref());
    let block = pair(style.scroll_margin_block.as_ref());
    let inline = pair(style.scroll_margin_inline.as_ref());
    [
        side(style.scroll_margin_top.as_ref(), style.scroll_margin_block_start.as_ref(), block[0], base[0]),
        side(style.scroll_margin_right.as_ref(), style.scroll_margin_inline_end.as_ref(), inline[1], base[1]),
        side(style.scroll_margin_bottom.as_ref(), style.scroll_margin_block_end.as_ref(), block[1], base[2]),
        side(style.scroll_margin_left.as_ref(), style.scroll_margin_inline_start.as_ref(), inline[0], base[3]),
    ]
}

/// The `scroll-padding` of a scroll box, in the same order.
pub(crate) fn scroll_padding(style: Option<&StyleDesc>) -> [f32; 4] {
    let Some(style) = style else { return [0.0; 4] };
    let base = shorthand(style.scroll_padding.as_ref());
    let block = pair(style.scroll_padding_block.as_ref());
    let inline = pair(style.scroll_padding_inline.as_ref());
    [
        side(style.scroll_padding_top.as_ref(), style.scroll_padding_block_start.as_ref(), block[0], base[0]),
        side(style.scroll_padding_right.as_ref(), style.scroll_padding_inline_end.as_ref(), inline[1], base[1]),
        side(style.scroll_padding_bottom.as_ref(), style.scroll_padding_block_end.as_ref(), block[1], base[2]),
        side(style.scroll_padding_left.as_ref(), style.scroll_padding_inline_start.as_ref(), inline[0], base[3]),
    ]
}

/// How far the content must move on one axis, in pixels.
pub(crate) fn axis_delta(align: Align, start: f32, end: f32, port_start: f32, port_end: f32) -> f32 {
    match align {
        Align::Start => start - port_start,
        Align::End => end - port_end,
        Align::Center => (start + end) / 2.0 - (port_start + port_end) / 2.0,
        Align::Nearest => {
            if start >= port_start && end <= port_end {
                0.0
            } else if (start - port_start).abs() <= (end - port_end).abs() {
                start - port_start
            } else {
                end - port_end
            }
        }
    }
}

/// Which ancestors scroll: every one, or only the nearest scroll box.
/// The web `container` option, from CSSOM View. `all` is the default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Container {
    All,
    Nearest,
}

impl Container {
    pub(crate) fn parse(word: Option<&str>) -> Container {
        match word.map(str::trim) {
            Some("nearest") => Container::Nearest,
            _ => Container::All,
        }
    }
}

/// Scroll every ancestor scroll box of `target` so the element shows.
/// Returns true when an offset changed.
pub(crate) fn scroll_into_view(
    tree: &RetainedTree,
    target: u64,
    block: Align,
    inline: Align,
    behavior: super::scroll_motion::Behavior,
    container: Container,
    handle_for: impl Fn(u64) -> Option<ScrollHandle>,
) -> bool {
    let Some(bounds) = crate::automation::get_bounds(target) else {
        return false;
    };
    let mut rect = Rect::from_bounds(bounds);
    let style = |id: u64| tree.elements.get(&id).and_then(|el| el.style.as_deref());
    let margin = scroll_margin(style(target));
    rect.start.x -= margin[3];
    rect.start.y -= margin[0];
    rect.end.x += margin[1];
    rect.end.y += margin[2];

    let mut moved = false;
    let mut current = tree.elements.get(&target).and_then(|el| el.parent);
    while let Some(id) = current {
        if let (Some(handle), Some(bounds)) = (handle_for(id), crate::automation::get_bounds(id)) {
            let mut port = Rect::from_bounds(bounds);
            let padding = scroll_padding(style(id));
            port.start.x += padding[3];
            port.start.y += padding[0];
            port.end.x -= padding[1];
            port.end.y -= padding[2];

            let delta = point(
                axis_delta(inline, rect.start.x, rect.end.x, port.start.x, port.end.x),
                axis_delta(block, rect.start.y, rect.end.y, port.start.y, port.end.y),
            );
            let old = handle.offset();
            let max = handle.max_offset();
            let new = point(
                (old.x - px(delta.x)).max(-max.x).min(px(0.0)),
                (old.y - px(delta.y)).max(-max.y).min(px(0.0)),
            );
            if new != old {
                if behavior.smooth(style(id)) {
                    super::scroll_motion::animate(id, &handle, new);
                } else {
                    handle.set_offset(new);
                }
                moved = true;
            }
            // The content of the box moves with the offset, and the
            // rectangle of the element moves with the content.
            rect.start.x += f32::from(new.x - old.x);
            rect.end.x += f32::from(new.x - old.x);
            rect.start.y += f32::from(new.y - old.y);
            rect.end.y += f32::from(new.y - old.y);
            if container == Container::Nearest {
                break;
            }
        }
        current = tree.elements.get(&id).and_then(|el| el.parent);
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shorthand_expands_the_css_way() {
        let sides = |text: &str| shorthand(Some(&Numeric::Text(text.to_string())));
        assert_eq!(sides("8px"), [8.0; 4]);
        assert_eq!(sides("8px 12px"), [8.0, 12.0, 8.0, 12.0]);
        assert_eq!(sides("1px 2px 3px"), [1.0, 2.0, 3.0, 2.0]);
        assert_eq!(sides("1px 2px 3px 4px"), [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(shorthand(Some(&Numeric::Number(6.0))), [6.0; 4]);
    }

    #[test]
    fn nearest_moves_the_short_way_or_not_at_all() {
        let near = |start, end| axis_delta(Align::Nearest, start, end, 100.0, 200.0);
        assert_eq!(near(120.0, 180.0), 0.0);
        assert_eq!(near(40.0, 80.0), -60.0);
        assert_eq!(near(240.0, 280.0), 80.0);
        assert_eq!(axis_delta(Align::Start, 40.0, 80.0, 100.0, 200.0), -60.0);
        assert_eq!(axis_delta(Align::End, 240.0, 280.0, 100.0, 200.0), 80.0);
        assert_eq!(axis_delta(Align::Center, 90.0, 110.0, 100.0, 200.0), -50.0);
    }
}
