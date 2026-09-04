//! Reading a CSS length, including `calc()`.
//!
//! lightningcss folds an arithmetic expression while it parses, so
//! `calc(8px + 2px)` arrives here as `10px` and `min(4px, 8px)` as `4px`. This
//! module maps what comes back onto the three shapes GPUIX can use, and
//! converts `rem` with the root font size the caller passes in.
//!
//! `rem` is converted to pixels before the text reaches the parser. lightningcss
//! holds `rem` as a relative unit and will not add it to a `px`, so
//! `calc(1rem + 4px)` would come back unfolded. Rewriting it to `calc(16px +
//! 4px)` first leaves one absolute unit and lets the parser fold the whole
//! expression.
//!
//! An expression that mixes a percentage with an absolute length, such as
//! `calc(100% - 8px)`, cannot fold without doing layout first. GPUI has no
//! length type that carries an unfolded expression, so those return `None` and
//! the declaration drops. That is the one case this cannot finish, and adding
//! it needs a calc variant on GPUI's own `Length`.

use lightningcss::traits::Parse;
use lightningcss::values::length::LengthValue;
use lightningcss::values::percentage::DimensionPercentage;

/// A length folded as far as it goes without layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    /// An absolute length, in pixels.
    Pixels(f32),
    /// A fraction of a value the layout decides, from a percentage.
    Fraction(f32),
    /// A number with no unit, such as the `1.5` in `line-height: 1.5`.
    Number(f32),
}

/// Read a CSS length. `rem` is the root font size in pixels.
///
/// Handles `calc()`, `min()`, `max()`, `clamp()` and every absolute unit, plus
/// `rem`. Returns `None` for a value that needs layout to finish and for a unit
/// this does not know.
pub fn length(text: &str, rem: f32) -> Option<Length> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    // A bare number first. lightningcss reads `1.5` as `1.5px`, which is the
    // quirks-mode rule for a length, and CSS gives a unitless number its own
    // meaning in `line-height` and in `opacity`. Reading it here keeps that
    // meaning and skips the parser for the shape most declarations arrive in.
    if let Ok(number) = text.parse::<f32>() {
        return Some(Length::Number(number));
    }
    // `8px` is the next most common shape, and stripping the suffix beats
    // running the parser over it.
    if let Some(pixels) = text
        .strip_suffix("px")
        .and_then(|number| number.trim_end().parse::<f32>().ok())
    {
        return Some(Length::Pixels(pixels));
    }

    let converted = expand_rem(text, rem);
    match DimensionPercentage::<LengthValue>::parse_string(&converted).ok()? {
        DimensionPercentage::Dimension(value) => absolute(&value, rem),
        DimensionPercentage::Percentage(percentage) => Some(Length::Fraction(percentage.0)),
        // The expression did not fold, so it holds a percentage next to an
        // absolute length and only layout can finish it.
        DimensionPercentage::Calc(_) => None,
    }
}

/// One folded dimension in pixels, where the unit allows it.
fn absolute(value: &LengthValue, rem: f32) -> Option<Length> {
    if let Some(pixels) = value.to_px() {
        return Some(Length::Pixels(pixels));
    }
    match value {
        // `expand_rem` handles this before the parser sees it. A `rem` still
        // reaching here came from a place that scan does not cover, so convert
        // it rather than drop it.
        LengthValue::Rem(value) => Some(Length::Pixels(value * rem)),
        // `em`, `ex` and `ch` need the element's own font size, and the
        // viewport units need the window. Neither is here yet.
        _ => None,
    }
}

/// `text` with every `rem` length written as pixels.
///
/// Borrows when there is no `rem` in it, which is most values. A match has to
/// be a whole token: the `rem` in `2rems` is not a unit, and neither is the one
/// in `--border-rem`.
fn expand_rem(text: &str, rem: f32) -> std::borrow::Cow<'_, str> {
    if !text.contains("rem") {
        return std::borrow::Cow::Borrowed(text);
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    let mut copied = 0;
    while let Some(offset) = text[at..].find("rem") {
        let start = at + offset;
        let end = start + "rem".len();
        at = end;
        // Anything word-like after it means this is not the unit `rem`.
        if bytes.get(end).is_some_and(|c| {
            c.is_ascii_alphanumeric() || *c == b'-' || *c == b'_' || *c == b'%' || *c == b'.'
        }) {
            continue;
        }
        let Some(number_at) = number_start(text, start) else {
            continue;
        };
        let Ok(number) = text[number_at..start].parse::<f32>() else {
            continue;
        };
        out.push_str(&text[copied..number_at]);
        out.push_str(&format!("{}px", number * rem));
        copied = end;
    }
    if copied == 0 {
        return std::borrow::Cow::Borrowed(text);
    }
    out.push_str(&text[copied..]);
    std::borrow::Cow::Owned(out)
}

/// Where the number that a unit at `unit_at` belongs to starts.
///
/// `None` when there is no number there, or when what runs up to the unit is
/// part of a longer word such as `--my-rem`.
fn number_start(text: &str, unit_at: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut start = unit_at;
    while start > 0 {
        let before = bytes[start - 1];
        if before.is_ascii_digit() || before == b'.' {
            start -= 1;
            continue;
        }
        if (before == b'-' || before == b'+') && start - 1 == first_of_token(bytes, start - 1) {
            start -= 1;
        }
        break;
    }
    if start == unit_at {
        return None;
    }
    // A digit run that follows a letter is part of a word, not a number.
    if start > 0 && (bytes[start - 1].is_ascii_alphabetic() || bytes[start - 1] == b'_') {
        return None;
    }
    Some(start)
}

/// Whether `at` can start a signed number, meaning nothing word-like precedes it.
fn first_of_token(bytes: &[u8], at: usize) -> usize {
    if at == 0 {
        return at;
    }
    match bytes[at - 1] {
        c if c.is_ascii_alphanumeric() || c == b'_' || c == b'%' || c == b')' => usize::MAX,
        _ => at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REM: f32 = 16.0;

    fn read(text: &str) -> Option<Length> {
        length(text, REM)
    }

    #[test]
    fn a_bare_number_keeps_its_own_meaning() {
        // Not 1.5 pixels. `line-height: 1.5` means one and a half times the
        // font size, and lightningcss would read this as a quirks-mode length.
        assert_eq!(read("1.5"), Some(Length::Number(1.5)));
        assert_eq!(read("0"), Some(Length::Number(0.0)));
        assert_eq!(read("-2"), Some(Length::Number(-2.0)));
    }

    #[test]
    fn pixels_read_as_pixels() {
        assert_eq!(read("8px"), Some(Length::Pixels(8.0)));
        assert_eq!(read(" 8px "), Some(Length::Pixels(8.0)));
        assert_eq!(read("-1.5px"), Some(Length::Pixels(-1.5)));
    }

    #[test]
    fn a_percentage_reads_as_a_fraction() {
        assert_eq!(read("150%"), Some(Length::Fraction(1.5)));
        assert_eq!(read("50%"), Some(Length::Fraction(0.5)));
    }

    #[test]
    fn rem_converts_with_the_root_font_size() {
        assert_eq!(read("2rem"), Some(Length::Pixels(32.0)));
        assert_eq!(length("2rem", 10.0), Some(Length::Pixels(20.0)));
    }

    #[test]
    fn the_other_absolute_units_convert_too() {
        assert_eq!(read("1in"), Some(Length::Pixels(96.0)));
        assert_eq!(read("12pt"), Some(Length::Pixels(16.0)));
    }

    #[test]
    fn calc_folds_arithmetic() {
        assert_eq!(read("calc(8px + 2px)"), Some(Length::Pixels(10.0)));
        assert_eq!(read("calc(8px - 2px)"), Some(Length::Pixels(6.0)));
        assert_eq!(read("calc(8px / 2)"), Some(Length::Pixels(4.0)));
        assert_eq!(read("calc(4px * 3)"), Some(Length::Pixels(12.0)));
    }

    #[test]
    fn calc_folds_the_tailwind_spacing_shape() {
        // Tailwind's whole spacing scale is `calc(var(--spacing) * n)`, and
        // `--spacing` is `0.25rem`. `var()` is already substituted by the time
        // this runs.
        assert_eq!(read("calc(0.25rem * 6)"), Some(Length::Pixels(24.0)));
        assert_eq!(read("calc(0.25rem * 1)"), Some(Length::Pixels(4.0)));
    }

    #[test]
    fn calc_mixes_units() {
        assert_eq!(read("calc(1rem + 4px)"), Some(Length::Pixels(20.0)));
    }

    #[test]
    fn min_max_and_clamp_fold_too() {
        assert_eq!(read("min(4px, 8px)"), Some(Length::Pixels(4.0)));
        assert_eq!(read("max(4px, 8px)"), Some(Length::Pixels(8.0)));
        assert_eq!(read("clamp(2px, 8px, 4px)"), Some(Length::Pixels(4.0)));
    }

    #[test]
    fn a_percentage_next_to_a_length_cannot_fold() {
        // Only layout knows what the percentage is a percentage of, and GPUI
        // has no length that carries the unfinished expression.
        assert_eq!(read("calc(100% - 8px)"), None);
    }

    #[test]
    fn a_unit_this_does_not_know_reads_as_nothing() {
        assert_eq!(read("2em"), None);
        assert_eq!(read("10vw"), None);
    }

    #[test]
    fn nonsense_reads_as_nothing() {
        assert_eq!(read(""), None);
        assert_eq!(read("   "), None);
        assert_eq!(read("not-a-length"), None);
        assert_eq!(read("calc(8px +)"), None);
    }

    #[test]
    fn rem_next_to_another_unit_still_folds() {
        assert_eq!(read("calc(1rem + 4px)"), Some(Length::Pixels(20.0)));
        assert_eq!(read("calc(2rem - 1rem)"), Some(Length::Pixels(16.0)));
        assert_eq!(read("max(1rem, 20px)"), Some(Length::Pixels(20.0)));
    }

    #[test]
    fn a_word_that_ends_in_rem_is_not_a_unit() {
        assert_eq!(expand_rem("2rems", REM), "2rems");
        assert_eq!(expand_rem("theorem", REM), "theorem");
        assert_eq!(expand_rem("var(--my-rem)", REM), "var(--my-rem)");
        assert_eq!(expand_rem("8px", REM), "8px");
    }

    #[test]
    fn rem_conversion_keeps_the_rest_of_the_text() {
        assert_eq!(expand_rem("calc(1rem + 4px)", REM), "calc(16px + 4px)");
        assert_eq!(expand_rem("calc(-1rem)", REM), "calc(-16px)");
        assert_eq!(expand_rem("clamp(1rem, 2rem, 3rem)", REM), "clamp(16px, 32px, 48px)");
    }
}
