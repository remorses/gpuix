//! Colour values for GPUIX.
//!
//! One CSS colour string becomes four channels. This module knows nothing
//! about GPUI, so its tests run with no GPU and no Metal toolchain.
//!
//! lightningcss reads every colour syntax in CSS Color 4 and CSS Color 5,
//! including `color-mix()`, `oklch()` and relative colour syntax. Three colours
//! it cannot finish on its own, because each one reads something only the
//! engine knows. `currentColor` reads the computed `color` of the element.
//! `light-dark()` reads the appearance of the window. A system colour reads the
//! platform palette. Those three arrive here as their own variants, and
//! `ColorContext` supplies what they need.

use lightningcss::traits::Parse;
use lightningcss::values::color::{ColorSpace, CssColor, SRGB};

use crate::CssError;

/// An sRGB colour with straight alpha. Every channel runs from 0 to 1.
///
/// The engine defines its own colour type so that nothing here depends on the
/// renderer. `gpuix-native` converts this into `gpui::Rgba` at the edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const TRANSPARENT: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
}

/// What a colour needs from the element and the window to finish.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorContext {
    /// The computed `color` of this element, which `currentColor` reads.
    pub current_color: Rgba,
    /// Whether the window is in the dark appearance, which `light-dark()`
    /// reads.
    pub dark: bool,
}

impl Default for ColorContext {
    fn default() -> Self {
        Self { current_color: Rgba::BLACK, dark: false }
    }
}

/// A colour, and what it needed from the context to finish.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub color: Rgba,
    /// Whether the value read `currentColor` anywhere inside it.
    ///
    /// The resolved-style cache needs this. A colour that reads the inherited
    /// colour stops being valid when an ancestor changes it, and a colour that
    /// does not stays valid forever. Reporting it here beats making the caller
    /// search the text, because `currentColor` nests.
    pub read_current_color: bool,
}

/// Read one colour.
pub fn color(value: &str, context: &ColorContext) -> Result<Rgba, CssError> {
    read(value, context).map(|reading| reading.color)
}

/// Read one colour and report what it needed.
pub fn read(value: &str, context: &ColorContext) -> Result<Reading, CssError> {
    let parsed = CssColor::parse_string(value).map_err(|_| CssError::BadValue {
        property: "color".to_string(),
        value: value.to_string(),
    })?;
    Ok(Reading {
        color: resolve(&parsed, context)?,
        read_current_color: reads_current_color(&parsed),
    })
}

/// Whether a colour reads `currentColor` at any depth.
///
/// `light-dark()` holds two colours and `color-mix()` holds two more, and the
/// keyword is legal inside any of them, so this walks rather than matching one
/// level. Only the side of `light-dark()` the appearance selects counts,
/// because the other side never reaches paint.
pub fn reads_current_color(parsed: &CssColor) -> bool {
    match parsed {
        CssColor::CurrentColor => true,
        CssColor::LightDark(light, dark) => {
            reads_current_color(light) || reads_current_color(dark)
        }
        _ => false,
    }
}

/// Turn a colour lightningcss already read into channels.
///
/// The engine calls this once it holds a `Property`, so the string never gets
/// parsed twice.
pub fn resolve(parsed: &CssColor, context: &ColorContext) -> Result<Rgba, CssError> {
    match parsed {
        CssColor::CurrentColor => Ok(context.current_color),
        CssColor::LightDark(light, dark) => {
            resolve(if context.dark { dark } else { light }, context)
        }
        CssColor::System(system) => Err(CssError::Unsupported {
            feature: "system colour".to_string(),
            value: format!("{system:?}"),
        }),
        other => {
            // sRGB rather than RGBA, because RGBA holds 8-bit channels and
            // would quantise alpha. An alpha of 0.5 comes back as 128/255.
            // GPUI blends in f32, so that rounding buys nothing.
            let srgb = SRGB::try_from(other).map_err(|_| CssError::BadValue {
                property: "color".to_string(),
                value: String::new(),
            })?;
            // `none` arrives as NaN, which the specification treats as zero
            // outside interpolation. Anything wider than sRGB has to land
            // inside it, because that is what GPUI paints.
            let srgb = srgb.resolve_missing();
            Ok(Rgba {
                r: srgb.r.clamp(0.0, 1.0),
                g: srgb.g.clamp(0.0, 1.0),
                b: srgb.b.clamp(0.0, 1.0),
                a: srgb.alpha.clamp(0.0, 1.0),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(value: &str) -> Rgba {
        color(value, &ColorContext::default())
            .unwrap_or_else(|error| panic!("did not read `{value}`: {error}"))
    }

    fn assert_same(input: &str, expected: &str) {
        let actual = ok(input);
        let expected = ok(expected);
        for (actual, expected) in [actual.r, actual.g, actual.b, actual.a]
            .into_iter()
            .zip([expected.r, expected.g, expected.b, expected.a])
        {
            assert!((actual - expected).abs() <= 1.0 / 255.0, "{input}");
        }
    }

    #[test]
    fn reads_every_absolute_syntax() {
        assert_same("#ff0000", "rgb(255, 0, 0)");
        assert_same("#f00", "rgb(255, 0, 0)");
        assert_same("#ff0000ff", "rgb(255, 0, 0)");
        assert_same("red", "rgb(255, 0, 0)");
        assert_same("hsl(0, 100%, 50%)", "rgb(255, 0, 0)");
        assert_same("hwb(0 0% 0%)", "rgb(255, 0, 0)");
        assert_same("rgb(255 0 0 / 50%)", "rgba(255, 0, 0, 0.5)");
    }

    #[test]
    fn reads_the_wide_gamut_syntaxes() {
        // csscolorparser could read these too. The reason to move is what
        // follows in the next three tests, not this one.
        for value in [
            "lab(50% 40 30)",
            "lch(50% 40 30)",
            "oklab(0.5 0.1 0.1)",
            "oklch(0.637 0.237 25.331)",
            "color(display-p3 1 0 0)",
        ] {
            assert!(color(value, &ColorContext::default()).is_ok(), "{value}");
        }
    }

    #[test]
    fn mixes_two_colours() {
        // Tailwind opacity modifiers such as `bg-red-500/50` emit color-mix,
        // which csscolorparser cannot read at all.
        assert_same(
            "color-mix(in srgb, #ff0000 100%, #0000ff 0%)",
            "#ff0000",
        );
    }

    #[test]
    fn reads_current_color_from_the_element() {
        let context = ColorContext {
            current_color: Rgba { r: 1.0, g: 0.0, b: 0.0, a: 1.0 },
            dark: false,
        };
        assert_eq!(color("currentColor", &context).unwrap(), context.current_color);
    }

    #[test]
    fn picks_the_side_of_light_dark_that_matches_the_window() {
        let light = ColorContext { dark: false, ..ColorContext::default() };
        let dark = ColorContext { dark: true, ..ColorContext::default() };
        let value = "light-dark(#ff0000, #0000ff)";
        assert_eq!(color(value, &light).unwrap(), ok("#ff0000"));
        assert_eq!(color(value, &dark).unwrap(), ok("#0000ff"));
    }

    #[test]
    fn resolves_current_color_inside_a_nested_function() {
        // `light-dark()` may hold `currentColor` on either side, so resolving
        // has to recurse rather than match one level.
        let context = ColorContext {
            current_color: Rgba { r: 0.0, g: 1.0, b: 0.0, a: 1.0 },
            dark: true,
        };
        let got = color("light-dark(#ff0000, currentColor)", &context).unwrap();
        assert_eq!(got, context.current_color);
    }

    #[test]
    fn reports_a_system_colour_rather_than_guessing() {
        // The platform palette is not wired up. Failing loudly beats painting
        // a colour the operating system did not choose.
        assert!(matches!(
            color("ButtonFace", &ColorContext::default()),
            Err(CssError::Unsupported { .. })
        ));
    }

    #[test]
    fn reports_whether_a_colour_read_the_element() {
        let context = ColorContext::default();
        assert!(read("currentColor", &context).unwrap().read_current_color);
        assert!(
            read("light-dark(#ff0000, currentColor)", &context)
                .unwrap()
                .read_current_color,
            "nested inside light-dark"
        );
        assert!(!read("#ff0000", &context).unwrap().read_current_color);
        assert!(!read("oklch(0.5 0.1 30)", &context).unwrap().read_current_color);
    }

    #[test]
    fn reports_a_value_that_is_not_a_colour() {
        assert!(matches!(
            color("definitely-not-a-colour", &ColorContext::default()),
            Err(CssError::BadValue { .. })
        ));
    }
}
