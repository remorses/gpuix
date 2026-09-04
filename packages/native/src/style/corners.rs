//! `corner-shape` and the `corner*` shorthands from CSS Borders 4, section 3.9,
//! plus the `border-*-radius` family they share their corners with.
//!
//! Each property parses on its own. A bad value drops that one property, as
//! in CSS, and the rest still apply. Logical names map with `horizontal-tb`
//! and `ltr`, which is the only writing mode GPUIX lays out.

use crate::style::{Numeric, StyleDesc};
use gpui::Corners;
use std::fmt::Debug;

/// The curvature of a plain `round` corner, which is also what a shorthand
/// resets the shape to when it names a radius only.
const ROUND: f32 = 1.0;

/// The curvature `K` of one `<corner-shape-value>`, or `None` when the text is
/// not one. The keywords map to `round = 1`, `squircle = 2`, `square = +inf`,
/// `bevel = 0`, `scoop = -1` and `notch = -inf`. `superellipse(K)` takes any
/// finite number or `infinity` with an optional sign.
pub(crate) fn shape(text: &str) -> Option<f32> {
    let lower = text.trim().to_ascii_lowercase();
    match lower.as_str() {
        "round" => return Some(ROUND),
        "squircle" => return Some(2.0),
        "square" => return Some(f32::INFINITY),
        "bevel" => return Some(0.0),
        "scoop" => return Some(-1.0),
        "notch" => return Some(f32::NEG_INFINITY),
        _ => {}
    }
    let inner = lower
        .strip_prefix("superellipse(")?
        .strip_suffix(')')?
        .trim();
    match inner {
        "infinity" | "+infinity" => Some(f32::INFINITY),
        "-infinity" => Some(f32::NEG_INFINITY),
        _ => inner.parse::<f32>().ok().filter(|k| k.is_finite()),
    }
}

/// Split on whitespace outside parentheses, so `superellipse( 2 )` stays one
/// token.
fn tokens(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = None;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            c if c.is_whitespace() && depth == 0 => {
                if let Some(s) = start.take() {
                    out.push(&text[s..i]);
                }
                continue;
            }
            _ => {}
        }
        if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        out.push(&text[s..]);
    }
    out
}

/// One to `max` shapes, or `None` when any token is not a shape.
fn shape_list(text: Option<&str>, max: usize) -> Option<Vec<f32>> {
    let shapes: Vec<f32> = tokens(text?)
        .into_iter()
        .map(shape)
        .collect::<Option<_>>()?;
    (1..=max).contains(&shapes.len()).then_some(shapes)
}

/// A `corner*` shorthand: radii and shapes in either order. A missing part
/// resets to its initial value, `0` or `round`, as every CSS shorthand does.
/// A `/` (elliptical radii) makes the whole value invalid.
fn shorthand(text: Option<&str>, max: usize) -> Option<(Vec<Numeric>, Vec<f32>)> {
    let mut radii = Vec::new();
    let mut shapes = Vec::new();
    for token in tokens(text?) {
        if token.contains('/') {
            return None;
        }
        match shape(token) {
            Some(k) => shapes.push(k),
            None => radii.push(Numeric::Text(token.to_owned())),
        }
    }
    if radii.len() > max || shapes.len() > max || radii.is_empty() && shapes.is_empty() {
        return None;
    }
    if radii.is_empty() {
        radii.push(Numeric::Number(0.0));
    }
    if shapes.is_empty() {
        shapes.push(ROUND);
    }
    Some((radii, shapes))
}

/// Expand a one-to-four value list the way `border-radius` does. Four corners
/// read top-left, top-right, bottom-right, bottom-left. Two corners read in the
/// order the side lists them.
fn spread<T: Clone>(values: &[T], count: usize) -> Vec<T> {
    let pick = |i: usize| values[i.min(values.len() - 1)].clone();
    match (count, values.len()) {
        (4, 2) => vec![pick(0), pick(1), pick(0), pick(1)],
        (4, 3) => vec![pick(0), pick(1), pick(2), pick(1)],
        _ => (0..count).map(pick).collect(),
    }
}

#[derive(Clone, Copy)]
enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

use Corner::*;

/// The four corners in the order a four-value list names them.
const ALL: [Corner; 4] = [TopLeft, TopRight, BottomRight, BottomLeft];

fn slot<T: Clone + Debug + Default + PartialEq>(corners: &mut Corners<T>, corner: Corner) -> &mut T {
    match corner {
        TopLeft => &mut corners.top_left,
        TopRight => &mut corners.top_right,
        BottomRight => &mut corners.bottom_right,
        BottomLeft => &mut corners.bottom_left,
    }
}

/// The radius and shape each corner ends up with, `None` where nothing set it.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ResolvedCorners {
    pub radii: Corners<Option<Numeric>>,
    pub shapes: Corners<Option<f32>>,
}

impl ResolvedCorners {
    fn set(&mut self, corners: &[Corner], radii: Option<&[Numeric]>, shapes: Option<&[f32]>) {
        if let Some(radii) = radii {
            for (corner, radius) in corners.iter().zip(spread(radii, corners.len())) {
                *slot(&mut self.radii, *corner) = Some(radius);
            }
        }
        if let Some(shapes) = shapes {
            for (corner, shape) in corners.iter().zip(spread(shapes, corners.len())) {
                *slot(&mut self.shapes, *corner) = Some(shape);
            }
        }
    }

    fn combined(&mut self, text: Option<&str>, corners: &[Corner]) {
        if let Some((radii, shapes)) = shorthand(text, corners.len()) {
            self.set(corners, Some(&radii), Some(&shapes));
        }
    }

    fn shapes_only(&mut self, text: Option<&str>, corners: &[Corner]) {
        if let Some(shapes) = shape_list(text, corners.len()) {
            self.set(corners, None, Some(&shapes));
        }
    }
}

/// Resolve every corner property of `style` into one radius and one shape per
/// corner.
///
/// Properties that name fewer corners win over ones that name more, and a
/// single-purpose property wins over a combined shorthand of the same reach.
/// So a longhand beats `cornerTopLeft`, which beats `cornerTop` and
/// `cornerTopShape`, which beat `borderRadius` and `cornerShape`, which beat
/// `corner`. CSS decides this by declaration order, which a style object does
/// not keep, so this is the nearest fixed rule.
pub(crate) fn resolve(style: &StyleDesc) -> ResolvedCorners {
    let mut out = ResolvedCorners::default();

    out.combined(style.corner.as_deref(), &ALL);
    if let Some(radius) = &style.border_radius {
        out.set(&ALL, Some(std::slice::from_ref(radius)), None);
    }
    out.shapes_only(style.corner_shape.as_deref(), &ALL);

    let sides: [(&Option<String>, &Option<String>, [Corner; 2]); 8] = [
        (&style.corner_top, &style.corner_top_shape, [TopLeft, TopRight]),
        (&style.corner_right, &style.corner_right_shape, [TopRight, BottomRight]),
        (&style.corner_bottom, &style.corner_bottom_shape, [BottomLeft, BottomRight]),
        (&style.corner_left, &style.corner_left_shape, [TopLeft, BottomLeft]),
        (&style.corner_block_start, &style.corner_block_start_shape, [TopLeft, TopRight]),
        (&style.corner_block_end, &style.corner_block_end_shape, [BottomLeft, BottomRight]),
        (&style.corner_inline_start, &style.corner_inline_start_shape, [TopLeft, BottomLeft]),
        (&style.corner_inline_end, &style.corner_inline_end_shape, [TopRight, BottomRight]),
    ];
    for (both, shape, corners) in sides {
        out.combined(both.as_deref(), &corners);
        out.shapes_only(shape.as_deref(), &corners);
    }

    let singles: [(&Option<String>, &Option<Numeric>, &Option<String>, Corner); 8] = [
        (&style.corner_top_left, &style.border_top_left_radius, &style.corner_top_left_shape, TopLeft),
        (&style.corner_top_right, &style.border_top_right_radius, &style.corner_top_right_shape, TopRight),
        (&style.corner_bottom_right, &style.border_bottom_right_radius, &style.corner_bottom_right_shape, BottomRight),
        (&style.corner_bottom_left, &style.border_bottom_left_radius, &style.corner_bottom_left_shape, BottomLeft),
        (&style.corner_start_start, &style.border_start_start_radius, &style.corner_start_start_shape, TopLeft),
        (&style.corner_start_end, &style.border_start_end_radius, &style.corner_start_end_shape, TopRight),
        (&style.corner_end_end, &style.border_end_end_radius, &style.corner_end_end_shape, BottomRight),
        (&style.corner_end_start, &style.border_end_start_radius, &style.corner_end_start_shape, BottomLeft),
    ];
    for (both, radius, shape, corner) in singles {
        out.combined(both.as_deref(), &[corner]);
        if let Some(radius) = radius {
            out.set(&[corner], Some(std::slice::from_ref(radius)), None);
        }
        out.shapes_only(shape.as_deref(), &[corner]);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled(build: impl FnOnce(&mut StyleDesc)) -> StyleDesc {
        let mut style = StyleDesc::default();
        build(&mut style);
        style
    }

    fn text(value: &str) -> Option<Numeric> {
        Some(Numeric::Text(value.to_owned()))
    }

    #[test]
    fn parses_every_keyword_and_the_function() {
        assert_eq!(shape("round"), Some(1.0));
        assert_eq!(shape("Squircle"), Some(2.0));
        assert_eq!(shape("square"), Some(f32::INFINITY));
        assert_eq!(shape("bevel"), Some(0.0));
        assert_eq!(shape("scoop"), Some(-1.0));
        assert_eq!(shape("notch"), Some(f32::NEG_INFINITY));
        assert_eq!(shape("superellipse(1.5)"), Some(1.5));
        assert_eq!(shape("superellipse( -3 )"), Some(-3.0));
        assert_eq!(shape("superellipse(infinity)"), Some(f32::INFINITY));
        assert_eq!(shape("superellipse(-infinity)"), Some(f32::NEG_INFINITY));
    }

    #[test]
    fn rejects_values_the_spec_does_not_allow() {
        for bad in ["circle", "superellipse()", "superellipse(nan)", "superellipse(1px)", "superellipse(1", "2"] {
            assert_eq!(shape(bad), None, "{bad}");
        }
    }

    #[test]
    fn corner_shape_fills_missing_corners_like_border_radius() {
        let style = styled(|s| s.corner_shape = Some("bevel scoop notch".into()));
        let out = resolve(&style);
        assert_eq!(out.shapes.top_left, Some(0.0));
        assert_eq!(out.shapes.top_right, Some(-1.0));
        assert_eq!(out.shapes.bottom_right, Some(f32::NEG_INFINITY));
        assert_eq!(out.shapes.bottom_left, Some(-1.0));
    }

    #[test]
    fn a_bad_token_drops_the_whole_property() {
        let style = styled(|s| s.corner_shape = Some("bevel oval".into()));
        assert_eq!(resolve(&style), ResolvedCorners::default());
        let style = styled(|s| s.corner_shape = Some("bevel bevel bevel bevel bevel".into()));
        assert_eq!(resolve(&style), ResolvedCorners::default());
    }

    #[test]
    fn corner_shorthand_takes_radii_and_shapes_in_either_order() {
        let style = styled(|s| s.corner = Some("squircle 8px 16px".into()));
        let out = resolve(&style);
        assert_eq!(out.radii.top_left, text("8px"));
        assert_eq!(out.radii.top_right, text("16px"));
        assert_eq!(out.radii.bottom_right, text("8px"));
        assert_eq!(out.shapes.bottom_left, Some(2.0));
    }

    #[test]
    fn a_shorthand_resets_the_part_it_leaves_out() {
        let style = styled(|s| s.corner_top_left = Some("bevel".into()));
        let out = resolve(&style);
        assert_eq!(out.radii.top_left, Some(Numeric::Number(0.0)));
        assert_eq!(out.shapes.top_left, Some(0.0));
        assert_eq!(out.radii.top_right, None);

        let style = styled(|s| s.corner = Some("4px".into()));
        assert_eq!(resolve(&style).shapes.top_left, Some(ROUND));
    }

    #[test]
    fn a_slash_makes_the_shorthand_invalid() {
        let style = styled(|s| s.corner = Some("8px / 4px bevel".into()));
        assert_eq!(resolve(&style), ResolvedCorners::default());
    }

    #[test]
    fn narrower_properties_win() {
        let style = styled(|s| {
            s.corner = Some("notch 1px".into());
            s.corner_shape = Some("bevel".into());
            s.corner_top_shape = Some("scoop squircle".into());
            s.corner_top_left_shape = Some("square".into());
            s.border_radius = Some(Numeric::Number(8.0));
            s.border_top_right_radius = Some(Numeric::Number(2.0));
        });
        let out = resolve(&style);
        assert_eq!(out.shapes.top_left, Some(f32::INFINITY));
        assert_eq!(out.shapes.top_right, Some(2.0));
        assert_eq!(out.shapes.bottom_right, Some(0.0));
        assert_eq!(out.radii.top_left, Some(Numeric::Number(8.0)));
        assert_eq!(out.radii.top_right, Some(Numeric::Number(2.0)));
    }

    #[test]
    fn logical_names_map_to_horizontal_ltr() {
        let style = styled(|s| {
            s.corner_inline_end_shape = Some("bevel scoop".into());
            s.corner_end_start_shape = Some("notch".into());
            s.border_start_start_radius = Some(Numeric::Number(3.0));
        });
        let out = resolve(&style);
        assert_eq!(out.shapes.top_right, Some(0.0));
        assert_eq!(out.shapes.bottom_right, Some(-1.0));
        assert_eq!(out.shapes.bottom_left, Some(f32::NEG_INFINITY));
        assert_eq!(out.radii.top_left, Some(Numeric::Number(3.0)));
    }
}
