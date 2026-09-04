//! Background fills for GPUIX.
//!
//! A `background` or `background-image` value is a colour or one
//! `linear-gradient()`. lightningcss reads the gradient syntax. This module
//! then fixes the colour stops up the way CSS Images 3 section 3.4.3 says, so
//! that every stop leaves here with a position from 0 to 1 that never
//! decreases. The renderer paints that list as it is.
//!
//! Stop positions are percentages only. A length needs the size of the box,
//! which only paint knows, so a length here is `Unsupported`. Radial and conic
//! gradients, repeating gradients and `url()` images are `Unsupported` too.

use lightningcss::traits::Parse;
use lightningcss::values::gradient::{Gradient, GradientItem, LineDirection};
use lightningcss::values::image::Image;
use lightningcss::values::percentage::DimensionPercentage;
use lightningcss::values::position::{HorizontalPositionKeyword, VerticalPositionKeyword};

use crate::color::{self, ColorContext, Rgba};
use crate::CssError;

/// An easing between two stops: the control points `[x1, y1, x2, y2]` of a
/// cubic bezier from (0, 0) to (1, 1). All zero means none, a straight mix.
///
/// CSS has no easing in gradients yet. This is the syntax the CSSWG proposal
/// (csswg-drafts issue 1332) uses: an `<easing-function>` in the place of a
/// colour hint, between two colour stops.
pub type Easing = [f32; 4];

/// Read one `<easing-function>` from CSS Easing 1. `linear` reads as none.
pub fn easing(text: &str) -> Option<Easing> {
    let lower = text.trim().to_ascii_lowercase();
    match lower.as_str() {
        "linear" => return Some([0.0; 4]),
        "ease" => return Some([0.25, 0.1, 0.25, 1.0]),
        "ease-in" => return Some([0.42, 0.0, 1.0, 1.0]),
        "ease-out" => return Some([0.0, 0.0, 0.58, 1.0]),
        "ease-in-out" => return Some([0.42, 0.0, 0.58, 1.0]),
        _ => {}
    }
    let inner = lower.strip_prefix("cubic-bezier(")?.strip_suffix(')')?;
    let numbers: Vec<f32> = inner
        .split(',')
        .map(|n| n.trim().parse::<f32>().ok().filter(|n| n.is_finite()))
        .collect::<Option<_>>()?;
    let [x1, y1, x2, y2] = numbers[..] else { return None };
    let unit = 0.0..=1.0;
    (unit.contains(&x1) && unit.contains(&x2)).then_some([x1, y1, x2, y2])
}

/// Split at the commas outside parentheses.
fn split_top_level(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            ',' if depth == 0 => {
                out.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&text[start..]);
    out
}

/// Pull the easings out of a `linear-gradient()` so lightningcss can read
/// the rest. Returns the value without them, how many arguments stay, and
/// each easing with the index of the argument that follows it.
fn split_easings(value: &str) -> Option<(String, usize, Vec<(usize, Easing)>)> {
    let open = value.find('(')?;
    let close = value.rfind(')')?;
    let head = &value[..open];
    if !head.trim().eq_ignore_ascii_case("linear-gradient") {
        return None;
    }
    let mut kept = Vec::new();
    let mut easings = Vec::new();
    for piece in split_top_level(&value[open + 1..close]) {
        match easing(piece) {
            Some(easing) => easings.push((kept.len(), easing)),
            None => kept.push(piece.trim()),
        }
    }
    Some((format!("{head}({})", kept.join(", ")), kept.len(), easings))
}

/// Where the line of a linear gradient points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Line {
    /// Degrees clockwise from `to top`.
    Angle(f32),
    ToTopLeft,
    ToTopRight,
    ToBottomRight,
    ToBottomLeft,
}

/// One colour stop after fix-up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stop {
    pub color: Rgba,
    /// Where on the gradient line, from 0 to 1.
    pub position: f32,
    /// Where between this stop and the next the mix is half way, as a
    /// fraction of that span. 0 means no hint.
    pub hint: f32,
    /// The easing to the next stop. All zero is none.
    pub easing: Easing,
}

/// A `linear-gradient()` ready to paint.
#[derive(Debug, Clone, PartialEq)]
pub struct LinearGradient {
    pub line: Line,
    /// At least two stops, positions from 0 to 1 and never decreasing.
    pub stops: Vec<Stop>,
}

/// What a background value paints.
#[derive(Debug, Clone, PartialEq)]
pub enum Fill {
    Color(Rgba),
    LinearGradient(LinearGradient),
}

impl Fill {
    /// The colour the fill paints first, for callers that want one colour.
    pub fn first_color(&self) -> Rgba {
        match self {
            Fill::Color(color) => *color,
            Fill::LinearGradient(gradient) => gradient.stops[0].color,
        }
    }
}

/// A fill, and what it needed from the context to finish.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub fill: Fill,
    /// Whether any colour in the value read `currentColor`.
    pub read_current_color: bool,
}

/// Read one background value. `none` reads as `Ok(None)`.
pub fn read(value: &str, context: &ColorContext) -> Result<Option<Reading>, CssError> {
    let (parsed, kept, easings) =
        split_easings(value).unwrap_or_else(|| (value.to_string(), 0, Vec::new()));
    let Ok(image) = Image::parse_string(&parsed) else {
        let reading = color::read(value, context)?;
        return Ok(Some(Reading {
            fill: Fill::Color(reading.color),
            read_current_color: reading.read_current_color,
        }));
    };
    match image {
        Image::None => Ok(None),
        Image::Gradient(gradient) => match *gradient {
            Gradient::Linear(linear) => {
                let line = line_of(&linear.direction);
                // The direction, when written, is the one argument that is
                // not an item. An easing sits after the item before it.
                let offset = kept - linear.items.len();
                let easings = easings
                    .iter()
                    .map(|(index, easing)| (index.checked_sub(offset + 1), *easing))
                    .collect::<Vec<_>>();
                let (stops, read_current_color) =
                    fix_up(&linear.items, &easings, context, value)?;
                Ok(Some(Reading {
                    fill: Fill::LinearGradient(LinearGradient { line, stops }),
                    read_current_color,
                }))
            }
            other => Err(unsupported(
                match other {
                    Gradient::RepeatingLinear(_) => "repeating-linear-gradient()",
                    Gradient::Radial(_) => "radial-gradient()",
                    Gradient::RepeatingRadial(_) => "repeating-radial-gradient()",
                    Gradient::Conic(_) => "conic-gradient()",
                    Gradient::RepeatingConic(_) => "repeating-conic-gradient()",
                    _ => "a vendor gradient",
                },
                value,
            )),
        },
        Image::Url(_) => Err(unsupported("url() images", value)),
        Image::ImageSet(_) => Err(unsupported("image-set()", value)),
    }
}

fn unsupported(feature: &str, value: &str) -> CssError {
    CssError::Unsupported {
        feature: feature.to_string(),
        value: value.to_string(),
    }
}

fn line_of(direction: &LineDirection) -> Line {
    use HorizontalPositionKeyword::{Left, Right};
    use VerticalPositionKeyword::{Bottom, Top};
    match direction {
        LineDirection::Angle(angle) => Line::Angle(angle.to_degrees()),
        LineDirection::Horizontal(Left) => Line::Angle(270.0),
        LineDirection::Horizontal(Right) => Line::Angle(90.0),
        LineDirection::Vertical(Top) => Line::Angle(0.0),
        LineDirection::Vertical(Bottom) => Line::Angle(180.0),
        LineDirection::Corner { horizontal: Left, vertical: Top } => Line::ToTopLeft,
        LineDirection::Corner { horizontal: Right, vertical: Top } => Line::ToTopRight,
        LineDirection::Corner { horizontal: Right, vertical: Bottom } => Line::ToBottomRight,
        LineDirection::Corner { horizontal: Left, vertical: Bottom } => Line::ToBottomLeft,
    }
}

type Item = GradientItem<lightningcss::values::length::LengthPercentage>;

/// A stop or hint while fix-up runs. A hint has no colour.
struct Pending {
    color: Option<Rgba>,
    position: Option<f32>,
    easing: Easing,
}

/// Turn the parsed items into stops with positions, the way CSS Images 3
/// section 3.4.3 says.
///
/// 1. A first stop with no position gets 0, a last one gets 1.
/// 2. A position smaller than one before it becomes that earlier position.
/// 3. A run of stops with no position spreads evenly between its neighbours.
///
/// A hint then folds into the stop before it as a fraction of the span to
/// the stop after it.
fn fix_up(
    items: &[Item],
    easings: &[(Option<usize>, Easing)],
    context: &ColorContext,
    value: &str,
) -> Result<(Vec<Stop>, bool), CssError> {
    let mut read_current_color = false;
    let mut pending = Vec::with_capacity(items.len());
    for item in items {
        match item {
            GradientItem::ColorStop(stop) => {
                read_current_color |= color::reads_current_color(&stop.color);
                pending.push(Pending {
                    color: Some(color::resolve(&stop.color, context)?),
                    position: stop
                        .position
                        .as_ref()
                        .map(|p| fraction(p, value))
                        .transpose()?,
                    easing: [0.0; 4],
                });
            }
            GradientItem::Hint(position) => pending.push(Pending {
                color: None,
                position: Some(fraction(position, value)?),
                easing: [0.0; 4],
            }),
        }
    }
    let bad_value = || CssError::BadValue {
        property: "background".to_string(),
        value: value.to_string(),
    };
    if pending.len() < 2 {
        return Err(bad_value());
    }
    // An easing goes between two colour stops, one per pair, and not next
    // to a hint, which already says where the half-way point is.
    for (index, easing) in easings {
        let Some(index) = *index else { return Err(bad_value()) };
        let both_colours = pending.get(index).is_some_and(|p| p.color.is_some())
            && pending.get(index + 1).is_some_and(|p| p.color.is_some());
        if !both_colours || pending[index].easing != [0.0; 4] {
            return Err(bad_value());
        }
        pending[index].easing = *easing;
    }

    let last = pending.len() - 1;
    pending[0].position.get_or_insert(0.0);
    pending[last].position.get_or_insert(1.0);
    let mut floor = 0.0f32;
    for item in &mut pending {
        if let Some(position) = item.position.as_mut() {
            *position = position.max(floor);
            floor = *position;
        }
    }
    let mut index = 0;
    while index < pending.len() {
        if pending[index].position.is_some() {
            index += 1;
            continue;
        }
        let start = index;
        while pending[index].position.is_none() {
            index += 1;
        }
        let from = pending[start - 1].position.unwrap();
        let to = pending[index].position.unwrap();
        let steps = (index - start + 1) as f32;
        for (offset, item) in pending[start..index].iter_mut().enumerate() {
            item.position = Some(from + (to - from) * (offset as f32 + 1.0) / steps);
        }
    }

    let mut stops: Vec<Stop> = Vec::with_capacity(pending.len());
    for (i, item) in pending.iter().enumerate() {
        let position = item.position.unwrap();
        match item.color {
            Some(color) => stops.push(Stop {
                color,
                position,
                hint: 0.0,
                easing: item.easing,
            }),
            None => {
                let Some(previous) = stops.last_mut() else { continue };
                let next = pending[i + 1..]
                    .iter()
                    .find(|p| p.color.is_some())
                    .and_then(|p| p.position)
                    .unwrap_or(position);
                let span = next - previous.position;
                if span > 0.0 {
                    previous.hint = (position - previous.position) / span;
                }
            }
        }
    }
    Ok((stops, read_current_color))
}

fn fraction<D>(position: &DimensionPercentage<D>, value: &str) -> Result<f32, CssError> {
    match position {
        DimensionPercentage::Percentage(percentage) => Ok(percentage.0),
        _ => Err(unsupported("gradient stop lengths", value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(value: &str) -> LinearGradient {
        match read(value, &ColorContext::default()) {
            Ok(Some(Reading { fill: Fill::LinearGradient(gradient), .. })) => gradient,
            other => panic!("`{value}` did not read as a gradient: {other:?}"),
        }
    }

    fn positions(value: &str) -> Vec<f32> {
        gradient(value)
            .stops
            .iter()
            .map(|s| (s.position * 1000.0).round() / 1000.0)
            .collect()
    }

    #[test]
    fn reads_a_plain_colour_as_a_fill() {
        let reading = read("red", &ColorContext::default()).unwrap().unwrap();
        assert_eq!(reading.fill, Fill::Color(Rgba { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }));
    }

    #[test]
    fn none_is_no_fill() {
        assert_eq!(read("none", &ColorContext::default()).unwrap(), None);
    }

    #[test]
    fn spreads_stops_with_no_position() {
        assert_eq!(positions("linear-gradient(red, lime, blue)"), vec![0.0, 0.5, 1.0]);
        assert_eq!(
            positions("linear-gradient(red 10%, lime, blue, black 70%)"),
            vec![0.1, 0.3, 0.5, 0.7]
        );
    }

    #[test]
    fn a_position_never_decreases() {
        assert_eq!(positions("linear-gradient(red 60%, lime 20%, blue)"), vec![0.6, 0.6, 1.0]);
    }

    #[test]
    fn folds_a_hint_into_the_stop_before_it() {
        let gradient = gradient("linear-gradient(red, 20%, blue)");
        assert_eq!(gradient.stops.len(), 2);
        assert!((gradient.stops[0].hint - 0.2).abs() < 1e-6);
        assert_eq!(gradient.stops[1].hint, 0.0);
    }

    #[test]
    fn reads_an_easing_between_two_stops() {
        let read = gradient("linear-gradient(to right, red, ease-in-out, blue)");
        assert_eq!(read.stops.len(), 2);
        assert_eq!(read.stops[0].easing, [0.42, 0.0, 0.58, 1.0]);
        assert_eq!(read.stops[1].easing, [0.0; 4]);

        let read =
            gradient("linear-gradient(red, cubic-bezier(0.5, 0, 1, 1.5), blue 80%, green)");
        assert_eq!(read.stops[0].easing, [0.5, 0.0, 1.0, 1.5]);
        assert_eq!(read.stops[1].position, 0.8);
        assert_eq!(read.stops[1].easing, [0.0; 4]);

        // `linear` is the straight mix, which is what no easing does.
        let read = gradient("linear-gradient(red, linear, blue)");
        assert_eq!(read.stops[0].easing, [0.0; 4]);
    }

    #[test]
    fn an_easing_needs_a_stop_on_each_side() {
        let context = ColorContext::default();
        for bad in [
            "linear-gradient(ease-in, red, blue)",
            "linear-gradient(red, blue, ease-in)",
            "linear-gradient(red, ease-in, ease-out, blue)",
            "linear-gradient(red, ease-in, 30%, blue)",
            "linear-gradient(red, 30%, ease-in, blue)",
            "linear-gradient(red, cubic-bezier(2, 0, 1, 1), blue)",
        ] {
            assert!(read(bad, &context).is_err(), "{bad}");
        }
    }

    #[test]
    fn reads_every_direction() {
        assert_eq!(gradient("linear-gradient(red, blue)").line, Line::Angle(180.0));
        assert_eq!(gradient("linear-gradient(to right, red, blue)").line, Line::Angle(90.0));
        assert_eq!(gradient("linear-gradient(0.25turn, red, blue)").line, Line::Angle(90.0));
        assert_eq!(gradient("linear-gradient(to top left, red, blue)").line, Line::ToTopLeft);
        assert_eq!(
            gradient("linear-gradient(to bottom right, red, blue)").line,
            Line::ToBottomRight
        );
    }

    #[test]
    fn current_color_inside_a_stop_is_reported() {
        let context = ColorContext { current_color: Rgba::TRANSPARENT, dark: false };
        let reading = read("linear-gradient(currentColor, blue)", &context).unwrap().unwrap();
        assert!(reading.read_current_color);
        assert_eq!(reading.fill.first_color(), Rgba::TRANSPARENT);
    }

    #[test]
    fn rejects_what_it_cannot_paint() {
        let context = ColorContext::default();
        assert!(matches!(
            read("radial-gradient(red, blue)", &context),
            Err(CssError::Unsupported { .. })
        ));
        assert!(matches!(
            read("linear-gradient(red 10px, blue)", &context),
            Err(CssError::Unsupported { .. })
        ));
        assert!(matches!(read("url(x.png)", &context), Err(CssError::Unsupported { .. })));
        assert!(matches!(read("linear-gradient(red)", &context), Err(CssError::BadValue { .. })));
        assert!(matches!(read("nonsense", &context), Err(CssError::BadValue { .. })));
    }
}
