//! `<code>` — a syntax-highlighted, selectable code block.
//!
//! Ported from Comet (https://github.com/zeronsh/comet), MIT.
//! Original: `render_code_block` in `crates/ui/src/markdown/render.rs`.
//!
//! ```tsx
//! <code
//!   code={source}
//!   language="typescript"       // or path="src/app.ts"
//!   showLineNumbers
//!   style={{ padding: 12, borderRadius: 10, backgroundColor: '#141414' }}
//! />
//! ```
//!
//! **It paints no surface of its own**: no fill, no border, no radius, no
//! padding and no language header. `style` is the surface, so an app owns the
//! card look and `<code>` owns the glyphs. `<markdown>` keeps its own
//! fenced-block card, because a document renderer owns its layout.
//!
//! Two things stay ours: lines never wrap, and the block is its own horizontal
//! scroller, so `whiteSpace` and `overflowX` in `style` do nothing.
//!
//! The block renders **one div per line** at an exact line height, so its
//! height is `lines × lineHeight` before any highlighting has run. Highlighting
//! is pure paint: every run on a line shares the same font and differs only in
//! colour, so a late highlight can never reflow the block.

use std::sync::Arc;

use gpui::{px, Font, Hsla, SharedString};

use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use crate::style::StyleDesc;
use crate::syntax::{cache::highlight_cached, HighlightedDocument};
use crate::text::runs::runs_for_spans;
use crate::theme::{Metrics, Theme};

// ── Factory ──────────────────────────────────────────────────────────

pub struct CodeFactory;

impl CustomElementFactory for CodeFactory {
    fn element_type(&self) -> &str {
        "code"
    }

    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(CodeElement::default())
    }
}

// ── Element ──────────────────────────────────────────────────────────

#[derive(Default)]
pub struct CodeElement {
    code: String,
    language: Option<String>,
    path: Option<String>,
    show_line_numbers: bool,
    theme: Theme,
    /// Cached highlight for the current `(code, language, path)`. The syntax
    /// cache already dedupes parsing, but this avoids hashing the source on
    /// every frame too.
    highlight: Option<Arc<HighlightedDocument>>,
    highlight_key: Option<(usize, u64)>,
}

impl CodeElement {
    /// Resolve the highlight for the current props, reusing the last result
    /// when nothing changed.
    fn resolve_highlight(&mut self) -> Option<Arc<HighlightedDocument>> {
        let key = (
            self.code.len(),
            fingerprint(&self.code, &self.language, &self.path),
        );
        if self.highlight_key == Some(key) {
            return self.highlight.clone();
        }
        self.highlight_key = Some(key);
        self.highlight =
            highlight_cached(&self.code, self.path.as_deref(), self.language.as_deref());
        self.highlight.clone()
    }
}

/// The typography actually used to paint, with `style` winning over the theme.
///
/// One resolver, because the same numbers feed three places: the div text
/// style, every `TextRun`, and the fixed row height. A `TextRun` that carries a
/// different font than the div measured with makes glyphs drift, and a row
/// height that ignores `style.lineHeight` clips the glyphs.
#[derive(Clone, PartialEq, Debug)]
struct Typography {
    family: String,
    weight: gpui::FontWeight,
    text_size: f32,
    line_height: f32,
    plain: Hsla,
}

impl Typography {
    fn font(&self) -> Font {
        let mut font = gpui::font(self.family.clone());
        font.weight = self.weight;
        font
    }
}

fn typography(style: Option<&StyleDesc>, theme: &Theme, m: &Metrics) -> Typography {
    let text_size = style
        .and_then(|style| style.font_size.as_ref())
        .and_then(|size| size.as_number())
        .map(|size| size as f32)
        .filter(|size| *size > 0.0)
        .unwrap_or(m.code_text_size);
    Typography {
        family: style
            .and_then(|style| style.font_family.clone())
            .unwrap_or_else(|| theme.font_mono.clone()),
        weight: style
            .and_then(|style| style.font_weight.as_ref())
            .map(crate::renderer::parse_font_weight)
            .unwrap_or(gpui::FontWeight::NORMAL),
        text_size,
        // `fontSize` alone must scale the row too. Rows are a fixed height, so
        // holding the theme line height while the glyphs grow makes lines
        // overlap. Keep the theme's ratio unless the caller states a height.
        // A zero `codeTextSize` metric would divide by zero and hand Taffy an
        // infinity, which paints nothing at all, so guard the ratio.
        line_height: style
            .and_then(|style| style.line_height.as_ref())
            .and_then(|height| height.as_number())
            .map(|height| height as f32)
            .filter(|height| *height > 0.0)
            .unwrap_or_else(|| {
                if m.code_text_size > 0.0 {
                    m.code_line_height * text_size / m.code_text_size
                } else {
                    m.code_line_height
                }
            }),
        plain: style
            .and_then(|style| style.color.as_deref())
            .and_then(crate::color::parse_color_rgba)
            .map(Hsla::from)
            .unwrap_or(theme.text),
    }
}

fn fingerprint(code: &str, language: &Option<String>, path: &Option<String>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    code.hash(&mut hasher);
    language.hash(&mut hasher);
    path.hash(&mut hasher);
    hasher.finish()
}

impl CustomElement for CodeElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        let theme = self.theme.clone();
        let m = &theme.metrics;
        let highlight = self.resolve_highlight();
        let type_style = typography(ctx.style, &theme, m);
        let font = type_style.font();
        let lines: Vec<&str> = self.code.split('\n').collect();
        let gutter_width = gutter_width(lines.len(), m);

        // overflow-x only works as a flex row viewport. A flex_col scroller
        // stretches nowrap rows to the viewport width, so a horizontal wheel
        // does nothing. Same pattern as host overflowX.
        let mut content = gpui::div()
            .flex_none()
            .flex()
            .flex_col()
            .font_family(type_style.family.clone())
            .font_weight(type_style.weight)
            .text_size(px(type_style.text_size))
            .line_height(px(type_style.line_height))
            .whitespace_nowrap();

        for (line_ix, line) in lines.iter().enumerate() {
            let spans: Vec<(std::ops::Range<usize>, gpui::Hsla)> = highlight
                .as_ref()
                .and_then(|doc| doc.lines.get(line_ix))
                .map(|spans| {
                    spans
                        .iter()
                        .map(|span| (span.range.clone(), theme.syntax.color(span.kind)))
                        .collect()
                })
                .unwrap_or_default();
            let runs = runs_for_spans(line, &spans, &font, type_style.plain);

            let mut row = gpui::div()
                .h(px(type_style.line_height))
                .flex_none()
                .flex()
                .flex_row();

            if self.show_line_numbers {
                row = row.child(
                    gpui::div()
                        .w(px(gutter_width))
                        .flex_none()
                        .flex()
                        .justify_end()
                        .pr(px(m.code_gutter_padding_right))
                        .text_color(theme.text_faint)
                        // The gutter is chrome, not content: a drag across the
                        // block must copy code, never a column of numbers.
                        .child(ctx.chrome_text((line_ix + 1).to_string(), None)),
                );
            }

            // `sub` is the line index so each line owns a stable selection key
            // across frames. Using the element id alone would make every line
            // share one key and the wash would paint on all of them at once.
            content = content.child(row.child(ctx.text(line_ix, line.to_string(), Some(runs))));
        }

        // The scroller stays a child of the styled surface instead of being the
        // surface. `custom_surface` records the last painted bounds on the
        // styled node, and gpui applies the scroll offset to a scroller's own
        // children — merging the two would drift `getElementBounds` (and every
        // automation click) after a horizontal pan.
        let body = gpui::div()
            .id(SharedString::from(format!("__gpuix_code_body_{}", ctx.id)))
            .flex()
            .min_w_0()
            .overflow_x_scroll()
            .restrict_scroll_to_axis()
            .child(content);

        let block = super::custom_surface(
            gpui::div().id(SharedString::from(format!("__gpuix_code_{}", ctx.id))),
            &ctx,
        );
        block.child(body).into_any_element()
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "code" => self.code = value.as_str().unwrap_or("").replace("\r\n", "\n"),
            "language" => self.language = value.as_str().map(str::to_string),
            "path" => self.path = value.as_str().map(str::to_string),
            "showLineNumbers" => self.show_line_numbers = value.as_bool().unwrap_or(false),
            "theme" => self.theme = Theme::from_prop(Some(&value)),
            _ => {}
        }
    }

    fn supported_props(&self) -> &'static [&'static str] {
        &["code", "language", "path", "showLineNumbers", "theme"]
    }

    fn supported_events(&self) -> &'static [&'static str] {
        &["click", "mouseEnter", "mouseLeave"]
    }

    fn destroy(&mut self) {}
}

/// Line-number gutter width, sized analytically from the digit count so the
/// code column never shifts as the block scrolls.
fn gutter_width(line_count: usize, m: &Metrics) -> f32 {
    let digits = line_count.max(1).to_string().len() as f32;
    (digits * m.code_gutter_digit_width + m.code_gutter_padding_right).max(m.code_gutter_min_width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutter_grows_with_digit_count() {
        let m = Metrics::default();
        assert_eq!(gutter_width(9, &m), m.code_gutter_min_width);
        assert!(gutter_width(1000, &m) > gutter_width(10, &m));
    }

    #[test]
    fn gutter_follows_the_metrics_override() {
        let mut m = Metrics::default();
        m.code_gutter_min_width = 64.0;
        assert_eq!(gutter_width(9, &m), 64.0);
    }

    #[test]
    fn typography_falls_back_to_the_theme() {
        let theme = Theme::dark();
        let resolved = typography(None, &theme, &theme.metrics);
        assert_eq!(resolved.family, theme.font_mono);
        assert_eq!(resolved.text_size, theme.metrics.code_text_size);
        assert_eq!(resolved.line_height, theme.metrics.code_line_height);
        assert_eq!(resolved.plain, theme.text);
    }

    #[test]
    fn typography_prefers_the_style_prop() {
        let theme = Theme::dark();
        let style = StyleDesc {
            font_family: Some("Fira Code".to_string()),
            font_size: Some(crate::style::Numeric::Number(20.0)),
            line_height: Some(crate::style::Numeric::Number(30.0)),
            color: Some("#ff0000".to_string()),
            ..Default::default()
        };
        let resolved = typography(Some(&style), &theme, &theme.metrics);
        assert_eq!(resolved.family, "Fira Code");
        assert_eq!(resolved.text_size, 20.0);
        assert_eq!(resolved.line_height, 30.0);
        assert_eq!(
            resolved.plain,
            Hsla::from(crate::color::parse_color_rgba("#ff0000").unwrap())
        );
    }

    #[test]
    fn typography_ignores_a_zero_size_or_line_height() {
        let theme = Theme::dark();
        let style = StyleDesc {
            font_size: Some(crate::style::Numeric::Number(0.0)),
            line_height: Some(crate::style::Numeric::Number(0.0)),
            ..Default::default()
        };
        let resolved = typography(Some(&style), &theme, &theme.metrics);
        assert_eq!(resolved.text_size, theme.metrics.code_text_size);
        assert_eq!(resolved.line_height, theme.metrics.code_line_height);
    }

    #[test]
    fn a_bare_font_size_scales_the_row_height() {
        let theme = Theme::dark();
        let style = StyleDesc {
            font_size: Some(crate::style::Numeric::Number((theme.metrics.code_text_size * 2.0) as f64)),
            ..Default::default()
        };
        let resolved = typography(Some(&style), &theme, &theme.metrics);
        // Doubling the glyphs without doubling the row would overlap the lines.
        assert_eq!(resolved.line_height, theme.metrics.code_line_height * 2.0);
    }

    #[test]
    fn a_zero_text_size_metric_cannot_produce_an_infinite_row() {
        let mut metrics = Metrics::default();
        metrics.code_text_size = 0.0;
        let style = StyleDesc {
            font_size: Some(crate::style::Numeric::Number(20.0)),
            ..Default::default()
        };
        let resolved = typography(Some(&style), &Theme::dark(), &metrics);
        assert_eq!(resolved.line_height, metrics.code_line_height);
    }

    #[test]
    fn highlight_is_reused_until_props_change() {
        let mut element = CodeElement {
            code: "let a = 1;".to_string(),
            path: Some("a.rs".to_string()),
            ..Default::default()
        };
        let first = element.resolve_highlight().unwrap();
        let second = element.resolve_highlight().unwrap();
        assert!(Arc::ptr_eq(&first, &second));

        element.code = "let bb = 2;".to_string();
        let third = element.resolve_highlight().unwrap();
        assert!(!Arc::ptr_eq(&first, &third));
    }
}
