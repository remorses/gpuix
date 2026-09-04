//! `var()` substitution.
//!
//! A custom property holds text, not a typed value. CSS calls this a
//! "guaranteed-invalid value" until something reads it through `var()`, and the
//! text is only parsed once it lands in a property that knows what it means.
//! So substitution here is textual, and the existing value parsers see the
//! result as if the author had written it in place.
//!
//! # What happens when a variable is missing
//!
//! CSS calls a `var()` with no declaration and no fallback "invalid at
//! computed-value time", and the property takes its inherited or initial value.
//! `value` returns `None` for that, and every caller drops the declaration,
//! which lands on the same place: the element keeps whatever it would have had.

use std::borrow::Cow;
use std::cell::Cell;

use gpuix_css::color::{ColorContext, Rgba};
use gpuix_css::length::Length;

use crate::inheritance::Variables;
use crate::style::LinearGradientValue;

/// How deep one `var()` may reach through other variables.
///
/// `--a: var(--b)` with `--b: var(--a)` is a cycle. CSS says a cycle makes every
/// variable in it invalid, and a depth limit reaches the same answer without
/// tracking the chain.
const MAX_DEPTH: usize = 16;

/// The variables in scope while one style resolves.
///
/// `used` records whether any `var()` actually read one. A style that reads none
/// resolves to the same value under every scope, so its cached resolution
/// survives a cascade change. That is most elements.
pub(crate) struct Scope<'a> {
    variables: &'a Variables,
    /// The computed `color` here, which is what `currentColor` names.
    current_color: Rgba,
    /// Whether the window is in the dark appearance, which `light-dark()`
    /// reads.
    dark: bool,
    /// The root font size in pixels, which is what `rem` is a multiple of.
    rem_size: f32,
    used: Cell<bool>,
}

impl<'a> Scope<'a> {
    pub fn new(
        variables: &'a Variables,
        current_color: Rgba,
        dark: bool,
        rem_size: f32,
    ) -> Self {
        Self {
            variables,
            current_color,
            dark,
            rem_size,
            used: Cell::new(false),
        }
    }

    /// The length a declaration means, or `None` when it means none.
    ///
    /// A bare number needs no work, which is the shape almost every declaration
    /// arrives in. Text goes through `var()` and then through `gpuix-css`,
    /// which folds `calc()`, `min()`, `max()` and `clamp()` and converts every
    /// absolute unit and `rem`.
    pub fn length(&self, value: &Option<crate::style::Numeric>) -> Option<Length> {
        match value.as_ref()? {
            crate::style::Numeric::Number(number) => Some(Length::Number(*number as f32)),
            crate::style::Numeric::Text(text) => {
                let text = self.value(text)?;
                gpuix_css::length::length(&text, self.rem_size)
            }
        }
    }

    /// The size a sizing property means, or `None` when it means none.
    ///
    /// `width` and its family take `auto`, and a percentage on them resolves
    /// against the parent rather than dropping, so they read through here
    /// rather than through `number`.
    pub fn dimension(
        &self,
        value: &Option<crate::style::Numeric>,
    ) -> Option<crate::style::DimensionValue> {
        use crate::style::DimensionValue;

        let length = match value.as_ref()? {
            crate::style::Numeric::Number(number) => Length::Number(*number as f32),
            crate::style::Numeric::Text(text) => {
                let text = self.value(text)?;
                // `auto` is a keyword rather than a length, so the length
                // parser never sees it.
                if text.trim().eq_ignore_ascii_case("auto") {
                    return Some(DimensionValue::Auto);
                }
                gpuix_css::length::length(&text, self.rem_size)?
            }
        };
        Some(match length {
            Length::Number(number) | Length::Pixels(number) => {
                DimensionValue::Pixels(number as f64)
            }
            Length::Fraction(fraction) => DimensionValue::Percentage(fraction as f64),
        })
    }

    /// The pixels a declaration means.
    ///
    /// This is what most properties want. A bare number is pixels, which is how
    /// the `style` prop has always written a length. A percentage reads as
    /// nothing, because the properties that take one have their own type.
    pub fn number(&self, value: &Option<crate::style::Numeric>) -> Option<f64> {
        match self.length(value)? {
            Length::Number(number) | Length::Pixels(number) => Some(number as f64),
            Length::Fraction(_) => None,
        }
    }

    /// The colour a declaration names, or `None` when it names none.
    ///
    /// `currentColor` resolves wherever it sits, including nested inside
    /// `light-dark()`. `gpuix-css` reports whether the value read it, which is
    /// what marks the resolution as depending on an ancestor.
    pub fn color(&self, text: &str) -> Option<Rgba> {
        let text = self.value(text)?;
        let context = ColorContext {
            current_color: self.current_color,
            dark: self.dark,
        };
        let reading = gpuix_css::color::read(&text, &context).ok()?;
        if reading.read_current_color {
            self.used.set(true);
        }
        Some(reading.color)
    }

    /// The fill a `background` or `background-image` declaration names.
    ///
    /// `None` when the value is `none`, or when it is not something this
    /// build paints. A colour or a `linear-gradient()` are what it paints.
    pub fn fill(&self, text: &str) -> Option<gpuix_css::background::Fill> {
        let text = self.value(text)?;
        let context = ColorContext {
            current_color: self.current_color,
            dark: self.dark,
        };
        let reading = gpuix_css::background::read(&text, &context).ok()??;
        if reading.read_current_color {
            self.used.set(true);
        }
        Some(reading.fill)
    }

    /// The blur and colour matrix a `filter` or `backdrop-filter` names.
    ///
    /// `None` for `none`, and for a list this build cannot paint, such as
    /// one with `drop-shadow()`.
    pub fn filter(&self, text: &str) -> Option<gpuix_css::effects::Filter> {
        let text = self.value(text)?;
        gpuix_css::effects::filter(&text).ok()?
    }

    /// The blend mode a `mix-blend-mode` or `background-blend-mode` names.
    pub fn blend_mode(&self, text: &str) -> Option<gpui::BlendMode> {
        use gpuix_css::effects::BlendMode as Css;
        let text = self.value(text)?;
        Some(match gpuix_css::effects::blend_mode(&text).ok()? {
            Css::Normal => gpui::BlendMode::Normal,
            Css::Multiply => gpui::BlendMode::Multiply,
            Css::Screen => gpui::BlendMode::Screen,
            Css::Overlay => gpui::BlendMode::Overlay,
            Css::Darken => gpui::BlendMode::Darken,
            Css::Lighten => gpui::BlendMode::Lighten,
            Css::ColorDodge => gpui::BlendMode::ColorDodge,
            Css::ColorBurn => gpui::BlendMode::ColorBurn,
            Css::HardLight => gpui::BlendMode::HardLight,
            Css::SoftLight => gpui::BlendMode::SoftLight,
            Css::Difference => gpui::BlendMode::Difference,
            Css::Exclusion => gpui::BlendMode::Exclusion,
            Css::Hue => gpui::BlendMode::Hue,
            Css::Saturation => gpui::BlendMode::Saturation,
            Css::Color => gpui::BlendMode::Color,
            Css::Luminosity => gpui::BlendMode::Luminosity,
            Css::PlusLighter => gpui::BlendMode::PlusLighter,
        })
    }

    /// The object form of `background`, painted in the colour space it asks
    /// for. A stop colour goes through `color`, so `var()` and `currentColor`
    /// work there too.
    pub fn gradient(&self, gradient: &LinearGradientValue) -> Option<gpui::Background> {
        let angle = gradient.angle as f32;
        if !angle.is_finite() || gradient.stops.is_empty() {
            return None;
        }
        let color_space = match gradient.color_space.as_deref() {
            None | Some("srgb") => gpui::ColorSpace::Srgb,
            Some("oklab") => gpui::ColorSpace::Oklab,
            Some(_) => return None,
        };
        let stops = gradient
            .stops
            .iter()
            .map(|stop| {
                let position = stop.position as f32;
                if !(0.0..=1.0).contains(&position) {
                    return None;
                }
                Some(gpui::LinearColorStop {
                    color: crate::color::to_hsla(self.color(&stop.color)?),
                    percentage: position,
                    hint: 0.0,
                    easing: [0.0; 4],
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(
            gpui::linear_gradient_stops(gpui::GradientLine::Angle(angle), &stops)
                .color_space(color_space),
        )
    }

    /// Whether resolving read a variable.
    pub fn used_a_variable(&self) -> bool {
        self.used.get()
    }

    /// `text` with every `var()` in it replaced.
    ///
    /// Borrows when there is nothing to replace, which is the common case. The
    /// `var(` test is a substring scan over bytes, so a style with no variables
    /// pays close to nothing for going through here.
    pub fn value<'t>(&self, text: &'t str) -> Option<Cow<'t, str>> {
        if !text.contains("var(") {
            return Some(Cow::Borrowed(text));
        }
        self.used.set(true);
        self.expand(text, 0).map(Cow::Owned)
    }

    fn expand(&self, text: &str, depth: usize) -> Option<String> {
        if depth > MAX_DEPTH {
            return None;
        }
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = find_var(rest) {
            out.push_str(&rest[..start]);
            let open = start + "var(".len();
            let close = closing_paren(rest, open)?;
            out.push_str(&self.one(&rest[open..close], depth)?);
            rest = &rest[close + 1..];
        }
        out.push_str(rest);
        Some(out)
    }

    /// The replacement for the inside of one `var(...)`.
    fn one(&self, inner: &str, depth: usize) -> Option<String> {
        let (name, fallback) = match top_level_comma(inner) {
            Some(comma) => (&inner[..comma], Some(&inner[comma + 1..])),
            None => (inner, None),
        };
        let name = name.trim();
        if !name.starts_with("--") {
            return None;
        }
        if let Some(declared) = self.variables.get(name) {
            return self.expand(declared, depth + 1);
        }
        // `var(--x,)` declares an empty fallback, which is legal and stands for
        // no value at all. Tailwind writes it, so the empty case has to survive
        // the trim below rather than count as a missing fallback.
        let fallback = fallback?;
        self.expand(fallback.trim(), depth + 1)
    }
}

/// Where the next `var(` starts, if there is one.
///
/// A match has to begin a token. Without that check the `var(` inside a name
/// such as `--myvar(` would count.
fn find_var(text: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(offset) = text[from..].find("var(") {
        let at = from + offset;
        let before = text[..at].chars().next_back();
        match before {
            Some(c) if c.is_alphanumeric() || c == '-' || c == '_' => from = at + 1,
            _ => return Some(at),
        }
    }
    None
}

/// The index of the `)` that closes the paren opened before `from`.
fn closing_paren(text: &str, from: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    for (at, c) in text[from..].char_indices() {
        match quote {
            Some(open) => {
                if c == open {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '(' => depth += 1,
                ')' if depth == 0 => return Some(from + at),
                ')' => depth -= 1,
                _ => {}
            },
        }
    }
    None
}

/// The index of the comma that splits the name from the fallback.
///
/// Only a comma outside every nested paren counts. A fallback such as
/// `rgb(0, 0, 0)` holds two commas of its own.
fn top_level_comma(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    for (at, c) in text.char_indices() {
        match quote {
            Some(open) => {
                if c == open {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => return Some(at),
                _ => {}
            },
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope_of(pairs: &[(&str, &str)]) -> Variables {
        let declared: Vec<(String, String)> = pairs
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect();
        Variables::default().layer(&declared)
    }

    fn value(text: &str, pairs: &[(&str, &str)]) -> Option<String> {
        let variables = scope_of(pairs);
        Scope::new(&variables, Rgba::BLACK, false, 16.0)
            .value(text)
            .map(|v| v.into_owned())
    }

    #[test]
    fn text_with_no_var_comes_back_untouched() {
        let variables = scope_of(&[]);
        let scope = Scope::new(&variables, Rgba::BLACK, false, 16.0);
        assert!(matches!(scope.value("#ff0000"), Some(Cow::Borrowed(_))));
        assert!(!scope.used_a_variable());
    }

    #[test]
    fn a_declared_variable_replaces_the_reference() {
        assert_eq!(
            value("var(--brand)", &[("--brand", "#ff0000")]),
            Some("#ff0000".to_string())
        );
    }

    #[test]
    fn reading_a_variable_marks_the_scope_as_used() {
        let variables = scope_of(&[("--brand", "#ff0000")]);
        let scope = Scope::new(&variables, Rgba::BLACK, false, 16.0);
        scope.value("var(--brand)");
        assert!(scope.used_a_variable());
    }

    #[test]
    fn a_reference_inside_other_text_keeps_that_text() {
        assert_eq!(
            value("rgb(var(--channels))", &[("--channels", "1 2 3")]),
            Some("rgb(1 2 3)".to_string())
        );
    }

    #[test]
    fn two_references_both_replace() {
        assert_eq!(
            value("var(--a) var(--b)", &[("--a", "1px"), ("--b", "solid")]),
            Some("1px solid".to_string())
        );
    }

    #[test]
    fn a_missing_variable_falls_back() {
        assert_eq!(
            value("var(--nope, #00ff00)", &[]),
            Some("#00ff00".to_string())
        );
    }

    #[test]
    fn a_declared_variable_beats_its_fallback() {
        assert_eq!(
            value("var(--brand, #00ff00)", &[("--brand", "#ff0000")]),
            Some("#ff0000".to_string())
        );
    }

    #[test]
    fn a_fallback_keeps_its_own_commas() {
        assert_eq!(
            value("var(--nope, rgb(1, 2, 3))", &[]),
            Some("rgb(1, 2, 3)".to_string())
        );
    }

    #[test]
    fn an_empty_fallback_stands_for_no_value() {
        // Tailwind writes `var(--tw-ring-inset,)` to mean "nothing unless the
        // inset variable is set". An empty fallback is legal CSS and must not
        // read as a missing one.
        assert_eq!(value("var(--nope,)", &[]), Some(String::new()));
        assert_eq!(
            value("var(--nope,) #ff0000", &[]),
            Some(" #ff0000".to_string())
        );
    }

    #[test]
    fn a_missing_variable_with_no_fallback_drops_the_declaration() {
        assert_eq!(value("var(--nope)", &[]), None);
    }

    #[test]
    fn a_variable_may_point_at_another_variable() {
        assert_eq!(
            value(
                "var(--outer)",
                &[("--outer", "var(--inner)"), ("--inner", "#ff0000")]
            ),
            Some("#ff0000".to_string())
        );
    }

    #[test]
    fn a_fallback_may_hold_a_reference() {
        assert_eq!(
            value("var(--nope, var(--other))", &[("--other", "#ff0000")]),
            Some("#ff0000".to_string())
        );
    }

    #[test]
    fn a_cycle_drops_the_declaration_instead_of_hanging() {
        assert_eq!(
            value("var(--a)", &[("--a", "var(--b)"), ("--b", "var(--a)")]),
            None
        );
    }

    #[test]
    fn a_name_without_the_two_dashes_is_not_a_variable() {
        assert_eq!(value("var(brand)", &[("brand", "#ff0000")]), None);
    }

    #[test]
    fn an_unclosed_paren_drops_the_declaration() {
        assert_eq!(value("var(--a", &[("--a", "#ff0000")]), None);
    }

    #[test]
    fn var_inside_a_name_is_not_a_reference() {
        assert_eq!(
            value("var(--myvar(x), #00ff00)", &[]),
            Some("#00ff00".to_string())
        );
    }

    #[test]
    fn current_color_names_the_computed_colour() {
        let variables = scope_of(&[]);
        let scope = Scope::new(
            &variables,
            Rgba {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            false,
            16.0,
        );
        assert_eq!(
            scope.color("currentColor"),
            Some(Rgba {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0
            })
        );
        assert_eq!(
            scope.color("CURRENTCOLOR"),
            Some(Rgba {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0
            })
        );
        assert!(scope.used_a_variable());
    }

    #[test]
    fn a_variable_may_hold_the_current_colour_keyword() {
        let variables = scope_of(&[("--edge", "currentColor")]);
        let scope = Scope::new(
            &variables,
            Rgba {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            false,
            16.0,
        );
        assert_eq!(
            scope.color("var(--edge)"),
            Some(Rgba {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0
            })
        );
    }

    #[test]
    fn an_ordinary_colour_does_not_read_the_scope() {
        let variables = scope_of(&[]);
        let scope = Scope::new(
            &variables,
            Rgba {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            false,
            16.0,
        );
        assert_eq!(
            scope.color("#00ff00"),
            gpuix_css::color::color("#00ff00", &Default::default()).ok()
        );
        assert!(!scope.used_a_variable());
    }

    #[test]
    fn a_bare_number_needs_no_resolving() {
        let variables = scope_of(&[]);
        let scope = Scope::new(&variables, Rgba::BLACK, false, 16.0);
        assert_eq!(scope.number(&Some(crate::style::Numeric::Number(8.0))), Some(8.0));
        assert!(!scope.used_a_variable());
    }

    fn number(text: &str, pairs: &[(&str, &str)]) -> Option<f64> {
        let variables = scope_of(pairs);
        Scope::new(&variables, Rgba::BLACK, false, 16.0)
            .number(&Some(crate::style::Numeric::Text(text.to_string())))
    }

    #[test]
    fn text_reads_as_a_number_or_a_length() {
        assert_eq!(number("8", &[]), Some(8.0));
        assert_eq!(number("8px", &[]), Some(8.0));
        assert_eq!(number("-1.5px", &[]), Some(-1.5));
        assert_eq!(number("2rem", &[]), Some(32.0));
        assert_eq!(number("calc(8px + 2px)", &[]), Some(10.0));
    }

    #[test]
    fn a_percentage_is_not_a_number_of_pixels() {
        // The properties that take a percentage have their own type. Reading
        // `50%` as 50 pixels here would be worse than reading nothing.
        assert_eq!(number("50%", &[]), None);
    }

    #[test]
    fn a_variable_reaches_a_number() {
        assert_eq!(number("var(--pad)", &[("--pad", "8px")]), Some(8.0));
        assert_eq!(number("var(--pad)", &[("--pad", "8")]), Some(8.0));
        assert_eq!(number("var(--nope, 4px)", &[]), Some(4.0));
        assert_eq!(number("var(--nope)", &[]), None);
    }

    #[test]
    fn calc_reads_a_variable_before_it_folds() {
        // This is Tailwind's whole spacing scale. `p-6` compiles to
        // `padding: calc(var(--spacing) * 6)` with `--spacing: 0.25rem`.
        assert_eq!(
            number("calc(var(--spacing) * 6)", &[("--spacing", "0.25rem")]),
            Some(24.0)
        );
        assert_eq!(
            number("calc(var(--spacing) * -1)", &[("--spacing", "0.25rem")]),
            Some(-4.0)
        );
    }

    #[test]
    fn a_bare_number_and_a_percentage_read_as_themselves() {
        let variables = scope_of(&[]);
        let scope = Scope::new(&variables, Rgba::BLACK, false, 16.0);
        let text = |t: &str| Some(crate::style::Numeric::Text(t.to_string()));
        assert_eq!(scope.length(&text("1.5")), Some(Length::Number(1.5)));
        assert_eq!(scope.length(&text("150%")), Some(Length::Fraction(1.5)));
        assert_eq!(scope.length(&text("8px")), Some(Length::Pixels(8.0)));
    }

    fn dimension(
        value: Option<crate::style::Numeric>,
        pairs: &[(&str, &str)],
    ) -> Option<crate::style::DimensionValue> {
        let variables = scope_of(pairs);
        Scope::new(&variables, Rgba::BLACK, false, 16.0).dimension(&value)
    }

    #[test]
    fn a_size_reads_every_length_the_other_properties_read() {
        use crate::style::{DimensionValue, Numeric};
        let text = |t: &str| Some(Numeric::Text(t.to_string()));

        assert_eq!(dimension(Some(Numeric::Number(200.0)), &[]), Some(DimensionValue::Pixels(200.0)));
        assert_eq!(dimension(text("200px"), &[]), Some(DimensionValue::Pixels(200.0)));
        assert_eq!(dimension(text("6rem"), &[]), Some(DimensionValue::Pixels(96.0)));
        assert_eq!(dimension(text("calc(100px + 2rem)"), &[]), Some(DimensionValue::Pixels(132.0)));
        assert_eq!(
            dimension(text("calc(var(--spacing) * 30)"), &[("--spacing", "4px")]),
            Some(DimensionValue::Pixels(120.0))
        );
    }

    #[test]
    fn a_size_also_takes_a_share_and_auto() {
        use crate::style::{DimensionValue, Numeric};
        let text = |t: &str| Some(Numeric::Text(t.to_string()));

        assert_eq!(dimension(text("50%"), &[]), Some(DimensionValue::Percentage(0.5)));
        assert_eq!(dimension(text("auto"), &[]), Some(DimensionValue::Auto));
        assert_eq!(dimension(text("AUTO"), &[]), Some(DimensionValue::Auto));
        assert_eq!(dimension(text("var(--w)"), &[("--w", "auto")]), Some(DimensionValue::Auto));
    }

    #[test]
    fn a_size_it_cannot_read_drops_the_declaration() {
        use crate::style::Numeric;
        let text = |t: &str| Some(Numeric::Text(t.to_string()));

        // None of these throws. The declaration drops and the element keeps
        // what it had, which is what CSS does with a value it cannot parse.
        assert_eq!(dimension(text("banana"), &[]), None);
        assert_eq!(dimension(text("3em"), &[]), None);
        assert_eq!(dimension(text("12vw"), &[]), None);
        assert_eq!(dimension(text("var(--missing)"), &[]), None);
    }

    #[test]
    fn an_absent_declaration_stays_absent() {
        let variables = scope_of(&[]);
        assert_eq!(Scope::new(&variables, Rgba::BLACK, false, 16.0).number(&None), None);
    }
}
