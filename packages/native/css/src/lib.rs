//! CSS values for GPUIX.
//!
//! This crate turns a CSS declaration, one property name and one value string,
//! into a parsed value. It knows nothing about GPUI, so its tests are pure
//! value tests that run on any machine with no GPU and no Metal toolchain.
//!
//! A value that holds `var()` cannot finish here, because the variables it
//! reads live on the element and its ancestors. Such a value comes back as
//! `Parsed::Pending`, and the cascade finishes it later with `substitute`.

pub mod background;
pub mod color;
pub mod effects;
pub mod length;

use std::collections::HashMap;

use lightningcss::properties::custom::{TokenList, TokenOrValue};
use lightningcss::properties::{Property, PropertyId};
use lightningcss::stylesheet::ParserOptions;
use lightningcss::traits::IntoOwned;

/// Custom property values in scope, keyed without the leading dashes.
pub type Vars = HashMap<String, String>;

/// A value that still holds one or more `var()` references.
#[derive(Debug, Clone, PartialEq)]
pub struct Unparsed {
    property: String,
    value: String,
}

/// The result of reading one CSS declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum Parsed {
    /// The value is complete.
    Ready(Property<'static>),
    /// The value reads a custom property, so the cascade has to finish it.
    Pending(Unparsed),
}

/// Why a declaration could not be read.
#[derive(Debug, Clone, PartialEq)]
pub enum CssError {
    /// The property name is not one this build knows.
    UnknownProperty { property: String },
    /// The property is known but the value does not fit it.
    BadValue { property: String, value: String },
    /// A `var()` reference has no value and no fallback.
    MissingVariable { property: String, name: String },
    /// The value is valid CSS that this build cannot finish.
    Unsupported { feature: String, value: String },
}

impl std::fmt::Display for CssError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CssError::UnknownProperty { property } => {
                write!(f, "unknown CSS property `{property}`")
            }
            CssError::BadValue { property, value } => {
                write!(f, "`{value}` is not a valid value for `{property}`")
            }
            CssError::MissingVariable { property, name } => {
                write!(f, "`{property}` reads `--{name}`, which has no value")
            }
            CssError::Unsupported { feature, value } => {
                write!(f, "`{value}` needs {feature}, which this build does not read")
            }
        }
    }
}

impl std::error::Error for CssError {}

/// Read one CSS declaration.
pub fn parse(property: &str, value: &str) -> Result<Parsed, CssError> {
    let id = PropertyId::from(property);
    if matches!(id, PropertyId::Custom(_)) && !property.starts_with("--") {
        return Err(CssError::UnknownProperty {
            property: property.to_string(),
        });
    }

    let parsed = Property::parse_string(id, value, ParserOptions::default()).map_err(|_| {
        CssError::BadValue {
            property: property.to_string(),
            value: value.to_string(),
        }
    })?;

    // lightningcss keeps anything it cannot fold as `Unparsed`, so this covers
    // two different cases. A value holding `var()` is pending, because the
    // variables live on the element and its ancestors. A value with no `var()`
    // is one lightningcss could not read at all, which means it is wrong.
    if let Property::Unparsed(held) = &parsed {
        if !reads_a_variable(&held.value) {
            return Err(CssError::BadValue {
                property: property.to_string(),
                value: value.to_string(),
            });
        }
        return Ok(Parsed::Pending(Unparsed {
            property: property.to_string(),
            value: value.to_string(),
        }));
    }

    Ok(Parsed::Ready(parsed.into_owned()))
}

/// Finish a value that reads custom properties.
pub fn substitute(unparsed: &Unparsed, vars: &Vars) -> Result<Parsed, CssError> {
    let substituted = expand_vars(&unparsed.value, vars, &unparsed.property)?;
    parse(&unparsed.property, &substituted)
}

/// Whether a held value reads a custom property anywhere inside it.
///
/// `var()` nests, as in `calc(var(--spacing) * 4)`, so this walks the whole
/// token tree instead of only its top level.
fn reads_a_variable(tokens: &TokenList) -> bool {
    tokens.0.iter().any(|token| match token {
        TokenOrValue::Var(_) => true,
        TokenOrValue::Function(function) => reads_a_variable(&function.arguments),
        _ => false,
    })
}

/// Replace every `var(--name, fallback)` with the value in scope.
fn expand_vars(value: &str, vars: &Vars, property: &str) -> Result<String, CssError> {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;

    while let Some(start) = rest.find("var(") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 4..];
        let end = match_paren(after).ok_or_else(|| CssError::BadValue {
            property: property.to_string(),
            value: value.to_string(),
        })?;
        let inner = &after[..end];

        let (name, fallback) = match inner.find(',') {
            Some(comma) => (inner[..comma].trim(), Some(inner[comma + 1..].trim())),
            None => (inner.trim(), None),
        };
        let key = name.trim_start_matches("--");

        match vars.get(key) {
            Some(found) => out.push_str(found),
            None => match fallback {
                // A fallback may itself read a variable.
                Some(fallback) => out.push_str(&expand_vars(fallback, vars, property)?),
                None => {
                    return Err(CssError::MissingVariable {
                        property: property.to_string(),
                        name: key.to_string(),
                    })
                }
            },
        }

        rest = &after[end + 1..];
    }

    out.push_str(rest);
    Ok(out)
}

/// Byte offset of the `)` that closes the group `input` starts inside.
fn match_paren(input: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, byte) in input.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' if depth == 0 => return Some(index),
            b')' => depth -= 1,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vars {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn ready(property: &str, value: &str) -> Property<'static> {
        match parse(property, value) {
            Ok(Parsed::Ready(property)) => property,
            other => panic!("expected a complete value, got {other:?}"),
        }
    }

    fn pending(property: &str, value: &str) -> Unparsed {
        match parse(property, value) {
            Ok(Parsed::Pending(unparsed)) => unparsed,
            other => panic!("expected a pending value, got {other:?}"),
        }
    }

    #[test]
    fn reads_a_plain_length() {
        assert!(matches!(parse("padding", "4px"), Ok(Parsed::Ready(_))));
    }

    #[test]
    fn reads_a_calc_with_no_variables() {
        assert!(matches!(
            parse("width", "calc(100% - 2rem)"),
            Ok(Parsed::Ready(_))
        ));
    }

    #[test]
    fn holds_a_value_that_reads_a_variable() {
        let held = pending("padding", "calc(var(--spacing) * 4)");
        let done = substitute(&held, &vars(&[("spacing", "0.25rem")])).unwrap();
        assert_eq!(done, parse("padding", "calc(0.25rem * 4)").unwrap());
    }

    #[test]
    fn reads_the_tailwind_default_palette() {
        // Tailwind v4 emits its palette in oklch, not in hex.
        let held = pending("color", "var(--color-red-500)");
        let done = substitute(&held, &vars(&[("color-red-500", "oklch(0.637 0.237 25.331)")]));
        assert!(matches!(done, Ok(Parsed::Ready(_))));
    }

    #[test]
    fn uses_the_fallback_when_the_variable_has_no_value() {
        // `text-sm` emits `line-height: var(--tw-leading, ...)`, and the
        // element only sets `--tw-leading` when a `leading-*` class is present.
        let held = pending("line-height", "var(--tw-leading, 1.25)");
        let done = substitute(&held, &vars(&[])).unwrap();
        assert_eq!(done, parse("line-height", "1.25").unwrap());
    }

    #[test]
    fn prefers_the_variable_over_the_fallback() {
        let held = pending("line-height", "var(--tw-leading, 1.25)");
        let done = substitute(&held, &vars(&[("tw-leading", "2")])).unwrap();
        assert_eq!(done, parse("line-height", "2").unwrap());
    }

    #[test]
    fn reads_a_fallback_that_reads_another_variable() {
        let held = pending("color", "var(--a, var(--b, #ff0000))");
        let done = substitute(&held, &vars(&[("b", "#00ff00")])).unwrap();
        assert_eq!(done, parse("color", "#00ff00").unwrap());
    }

    #[test]
    fn reports_a_variable_with_no_value_and_no_fallback() {
        let held = pending("padding", "var(--nothing)");
        assert_eq!(
            substitute(&held, &vars(&[])),
            Err(CssError::MissingVariable {
                property: "padding".to_string(),
                name: "nothing".to_string(),
            })
        );
    }

    #[test]
    fn reports_a_property_it_does_not_know() {
        assert_eq!(
            parse("not-a-property", "1px"),
            Err(CssError::UnknownProperty {
                property: "not-a-property".to_string(),
            })
        );
    }

    #[test]
    fn reports_a_value_that_does_not_fit_the_property() {
        assert_eq!(
            parse("width", "definitely-not-a-width"),
            Err(CssError::BadValue {
                property: "width".to_string(),
                value: "definitely-not-a-width".to_string(),
            })
        );
    }

    #[test]
    fn keeps_a_custom_property() {
        assert!(parse("--spacing", "0.25rem").is_ok());
    }

    #[test]
    fn reads_a_colour_that_mixes_two_colours() {
        // Tailwind opacity modifiers such as `bg-red-500/50` emit color-mix.
        assert!(matches!(
            parse(
                "background-color",
                "color-mix(in oklab, oklch(0.637 0.237 25.331) 50%, transparent)"
            ),
            Ok(Parsed::Ready(_))
        ));
    }

    #[test]
    fn the_resolved_value_does_not_borrow_the_input() {
        // `Parsed::Ready` owns its value, so the caller can cache it.
        let held = {
            let value = String::from("8px");
            ready("padding", &value)
        };
        assert_eq!(Parsed::Ready(held), parse("padding", "8px").unwrap());
    }
}
