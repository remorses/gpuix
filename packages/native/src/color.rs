//! The GPUI edge for colour.
//!
//! `gpuix-css` reads a colour string into `gpuix_css::color::Rgba`, which is
//! four channels and nothing else. This file is the only place that turns those
//! channels into a GPUI paint type. Keeping the conversion here is what lets
//! the colour tests in `gpuix-css` run with no GPU.

use gpuix_css::color::{ColorContext, Rgba};

/// Turn engine channels into GPUI's sRGB paint type.
pub(crate) fn to_gpui(color: Rgba) -> gpui::Rgba {
    gpui::Rgba { r: color.r, g: color.g, b: color.b, a: color.a }
}

/// Turn a GPUI colour into engine channels.
///
/// The theme is still written in GPUI types, so a theme colour crosses back
/// this way to reach the cascade.
pub(crate) fn from_gpui(color: impl Into<gpui::Rgba>) -> Rgba {
    let color = color.into();
    Rgba { r: color.r, g: color.g, b: color.b, a: color.a }
}

/// Turn engine channels into GPUI's HSL paint type.
///
/// GPUI takes `Hsla` in most style setters, so this is the shape the style
/// sink needs most often.
pub(crate) fn to_hsla(color: Rgba) -> gpui::Hsla {
    to_gpui(color).into()
}

/// Turn a fill the engine read into what GPUI paints.
///
/// A gradient carries its stops already fixed up, so this only copies them
/// across.
pub(crate) fn to_background(fill: &gpuix_css::background::Fill) -> gpui::Background {
    use gpuix_css::background::{Fill, Line};
    match fill {
        Fill::Color(color) => to_hsla(*color).into(),
        Fill::LinearGradient(gradient) => {
            let line = match gradient.line {
                Line::Angle(degrees) => gpui::GradientLine::Angle(degrees),
                Line::ToTopLeft => gpui::GradientLine::ToTopLeft,
                Line::ToTopRight => gpui::GradientLine::ToTopRight,
                Line::ToBottomRight => gpui::GradientLine::ToBottomRight,
                Line::ToBottomLeft => gpui::GradientLine::ToBottomLeft,
            };
            let stops: Vec<gpui::LinearColorStop> = gradient
                .stops
                .iter()
                .map(|stop| gpui::LinearColorStop {
                    color: to_hsla(stop.color),
                    percentage: stop.position,
                    hint: hint_for(stop),
                })
                .collect();
            gpui::linear_gradient_stops(line, &stops)
        }
    }
}

/// The hint GPUI paints for one stop.
///
/// GPUI has no easing in a gradient, but its hint moves the half-way point of
/// the mix along the CSS exponential curve. An easing becomes the hint where
/// its own curve crosses one half. The two curves then agree at both ends and
/// at the half-way point, which is as close as one number gets.
fn hint_for(stop: &gpuix_css::background::Stop) -> f32 {
    if stop.hint != 0.0 || stop.easing == [0.0; 4] {
        return stop.hint;
    }
    easing_half_point(stop.easing)
}

/// The x at which a cubic bezier easing's output crosses one half.
///
/// The curve runs from (0, 0) to (1, 1) with the control points
/// `[x1, y1, x2, y2]`. The output can overshoot, so this walks to the first
/// crossing and then bisects.
fn easing_half_point(easing: [f32; 4]) -> f32 {
    let [x1, y1, x2, y2] = easing;
    let at = |a: f32, b: f32, t: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
    };
    let y = |t: f32| at(y1, y2, t);
    let mut low = 0.0f32;
    let mut high = 1.0f32;
    for step in 1..=64 {
        let t = step as f32 / 64.0;
        if y(t) >= 0.5 {
            high = t;
            low = t - 1.0 / 64.0;
            break;
        }
    }
    for _ in 0..24 {
        let mid = (low + high) / 2.0;
        if y(mid) < 0.5 {
            low = mid;
        } else {
            high = mid;
        }
    }
    let x = at(x1, x2, (low + high) / 2.0);
    // The shader reads a hint of 0 or 1 as none, so keep the value inside.
    x.clamp(0.001, 0.999)
}

/// Read a colour that depends on the element or the window.
///
/// `currentColor` and `light-dark()` both need context, so this is the entry
/// point the cascade uses once it knows the computed `color` and the window
/// appearance.
pub(crate) fn parse_color_in(value: &str, context: &ColorContext) -> Option<gpui::Rgba> {
    gpuix_css::color::color(value, context).ok().map(to_gpui)
}

/// Read a colour that stands on its own.
///
/// Callers that hold no cascade get the default context, where `currentColor`
/// is black and the appearance is light.
pub(crate) fn parse_color_rgba(value: &str) -> Option<gpui::Rgba> {
    parse_color_in(value, &ColorContext::default())
}

/// Compatibility helper kept at the gpuix-native crate root.
pub fn parse_color(value: &str) -> Option<(f32, f32, f32, f32)> {
    parse_color_rgba(value).map(|color| (color.r, color.g, color.b, color.a))
}

/// Compatibility helper kept at the gpuix-native crate root.
pub fn parse_color_hex(value: &str) -> Option<u32> {
    parse_color_rgba(value).map(u32::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_same(input: &str, expected: &str) {
        let actual = parse_color(input).unwrap_or_else(|| panic!("did not parse {input}"));
        let expected = parse_color(expected).unwrap();
        for (actual, expected) in [actual.0, actual.1, actual.2, actual.3]
            .into_iter()
            .zip([expected.0, expected.1, expected.2, expected.3])
        {
            assert!((actual - expected).abs() <= 1.0 / 255.0, "{input}");
        }
    }

    #[test]
    fn parses_every_absolute_function_family() {
        // `hsv()`, `hsva()` and `hwba()` used to appear in this list. No CSS
        // specification defines any of them. They came from csscolorparser,
        // which this crate no longer uses.
        let cases = [
            ("#f00f", "#ff0000ff"),
            ("rebeccapurple", "#663399"),
            ("transparent", "#00000000"),
            ("rgb(255 0 0)", "#ff0000"),
            ("rgba(255, 0, 0, 1)", "#ff0000"),
            ("hsl(0 100% 50%)", "#ff0000"),
            ("hsla(0, 100%, 50%, 1)", "#ff0000"),
            ("hwb(0 0% 0%)", "#ff0000"),
            ("lab(100% 0 0)", "#ffffff"),
            ("lch(100% 0 0)", "#ffffff"),
            ("oklab(0.62796 0.22486 0.12585)", "#ff0000"),
            ("oklch(0.62796 0.25768 29.23388)", "#ff0000"),
            ("rgb(none none none / none)", "#00000000"),
        ];

        for (input, expected) in cases {
            assert_same(input, expected);
        }
    }

    #[test]
    fn parses_alpha_in_every_function_family() {
        // lightningcss keeps `rgb()`, `hsl()` and `hwb()` in an 8-bit RGBA,
        // so an alpha of 50% comes back as 128/255. The wider colour spaces
        // below keep the exact float. One step of 8-bit alpha is the tolerance
        // every other test in this file already uses.
        let cases = [
            "rgb(0 0 0 / 50%)",
            "rgba(0, 0, 0, 0.5)",
            "hsl(0 0% 0% / 50%)",
            "hsla(0, 0%, 0%, 0.5)",
            "hwb(0 0% 100% / 50%)",
            "lab(0% 0 0 / 50%)",
            "lch(0% 0 0 / 50%)",
            "oklab(0 0 0 / 50%)",
            "oklch(0 0 0 / 50%)",
        ];

        for input in cases {
            let (_, _, _, alpha) =
                parse_color(input).unwrap_or_else(|| panic!("did not parse {input}"));
            assert!((alpha - 0.5).abs() <= 1.0 / 255.0, "{input}");
        }
    }

    #[test]
    fn parses_every_relative_function_family() {
        let cases = [
            ("rgb(from #bad455 b r g / alpha)", "#55bad4"),
            ("hsl(from #bad455 h s l / alpha)", "#bad455"),
            ("hwb(from #bad455 h w b / alpha)", "#bad455"),
            ("lab(from #bad455 l a b / alpha)", "#bad455"),
            ("lch(from #bad455 l c h / alpha)", "#bad455"),
            ("oklab(from #bad455 calc(l * 0.7) a b)", "#708500"),
            (
                "oklch(from #bad455 calc(l - 0.15) calc(c * 0.7) h)",
                "#8fa150",
            ),
        ];

        for (input, expected) in cases {
            assert_same(input, expected);
        }

        for input in [
            "lab(from #bad455 l a b / calc(alpha / 2))",
            "lch(from #bad455 l c h / calc(alpha * 0.5))",
        ] {
            let (_, _, _, alpha) = parse_color(input).unwrap();
            assert!((alpha - 0.5).abs() < f32::EPSILON, "{input}");
        }
    }

    #[test]
    fn reads_the_syntaxes_csscolorparser_could_not() {
        // Both are CSS Color specifications, and both used to come back as
        // `None`. Tailwind emits color-mix for every opacity modifier such as
        // `bg-red-500/50`.
        assert!(parse_color("color(display-p3 1 0 0)").is_some());
        assert_same("color-mix(in srgb, #ff0000 100%, #0000ff 0%)", "#ff0000");
    }

    #[test]
    fn rejects_values_outside_the_parser_contract() {
        for input in [
            "",
            "reddish",
            "#gg0000",
            "hsl(nope)",
            // Bare hex with no `#` is not a CSS colour.
            "ff0000ff",
            // `hsv()` is not a CSS colour function.
            "hsv(0 100% 100%)",
        ] {
            assert_eq!(parse_color_hex(input), None, "{input}");
        }
    }

    #[test]
    fn clips_wide_gamut_output_before_gpui() {
        let color = parse_color_rgba("oklch(90% 0.4 40)").expect("valid OKLCH");
        for channel in [color.r, color.g, color.b, color.a] {
            assert!((0.0..=1.0).contains(&channel));
        }
    }

    #[test]
    fn reads_current_color_from_the_context() {
        let context = ColorContext {
            current_color: Rgba { r: 1.0, g: 0.0, b: 0.0, a: 1.0 },
            dark: false,
        };
        assert_eq!(
            parse_color_in("currentColor", &context),
            parse_color_rgba("#ff0000")
        );
    }

    #[test]
    fn compatibility_helpers_share_the_same_result() {
        let rgba = parse_color_rgba("oklch(0.62796 0.25768 29.23388 / 50%)").unwrap();
        assert_eq!(
            parse_color("oklch(0.62796 0.25768 29.23388 / 50%)"),
            Some((rgba.r, rgba.g, rgba.b, rgba.a))
        );
        assert_eq!(
            parse_color_hex("oklch(0.62796 0.25768 29.23388 / 50%)"),
            Some(u32::from(rgba))
        );
    }

    #[test]
    fn an_easing_becomes_the_hint_at_its_half_point() {
        // ease-in-out is symmetric, so its half point is the middle, which is
        // the same paint as no hint at all.
        let middle = easing_half_point([0.42, 0.0, 0.58, 1.0]);
        assert!((middle - 0.5).abs() < 0.01, "got {middle}");

        // ease-in holds the first colour longer, so the half point sits late.
        let late = easing_half_point([0.42, 0.0, 1.0, 1.0]);
        assert!(late > 0.6, "got {late}");

        // cubic-bezier(0, 1, 0, 1) jumps toward the second colour at once.
        let early = easing_half_point([0.0, 1.0, 0.0, 1.0]);
        assert!(early < 0.05, "got {early}");
    }

    #[test]
    fn an_explicit_hint_wins_over_the_easing() {
        let stop = gpuix_css::background::Stop {
            color: Rgba { r: 1.0, g: 0.0, b: 0.0, a: 1.0 },
            position: 0.0,
            hint: 0.25,
            easing: [0.42, 0.0, 1.0, 1.0],
        };
        assert_eq!(hint_for(&stop), 0.25);
    }
}
