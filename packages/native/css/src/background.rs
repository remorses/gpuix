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
    let Ok(image) = Image::parse_string(value) else {
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
                let (stops, read_current_color) = fix_up(&linear.items, context, value)?;
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
                });
            }
            GradientItem::Hint(position) => pending.push(Pending {
                color: None,
                position: Some(fraction(position, value)?),
            }),
        }
    }
    if pending.len() < 2 {
        return Err(CssError::BadValue {
            property: "background".to_string(),
            value: value.to_string(),
        });
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
            Some(color) => stops.push(Stop { color, position, hint: 0.0 }),
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
