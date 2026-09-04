//! Theme tokens for native text editors and document components.
//!
//! Ported from Comet (https://github.com/zeronsh/comet), MIT.
//! Original: `crates/ui/src/theme.rs`.
//!
//! Colours are declared in **oklch** exactly as the Tailwind v4 palette does, then
//! converted to `gpui::Hsla` here. Keeping the source notation means a token can be
//! checked against the Tailwind reference without reverse-engineering an HSL triple.
//!
//! Every token is overridable from JS: elements take a `theme` prop that
//! deserializes into [`ThemeOverride`] and is applied on top of [`Theme::dark`].
//! Unknown keys are ignored so a JS theme object can carry extra fields.

use crate::syntax::HighlightKind;
use gpui::{hsla, Hsla};
use serde::Deserialize;

// ── Colour helpers ───────────────────────────────────────────────────

/// Opaque grey from an 8-bit channel value (`grey(0x0e)` is `#0e0e0e`).
pub fn grey(value: u8) -> Hsla {
    hsla(0.0, 0.0, value as f32 / 255.0, 1.0)
}

/// Achromatic colour at an oklch lightness — the Tailwind neutral ramp.
pub fn neutral(lightness: f32) -> Hsla {
    let [v, _, _] = oklch_to_srgb(lightness, 0.0, 0.0);
    hsla(0.0, 0.0, v, 1.0)
}

/// oklch (CSS notation: L 0..1, C, H in degrees) to `gpui::Hsla`.
pub fn oklch(l: f32, c: f32, h_deg: f32) -> Hsla {
    let [r, g, b] = oklch_to_srgb(l, c, h_deg);
    let (h, s, l) = rgb_to_hsl(r, g, b);
    hsla(h, s, l, 1.0)
}

/// oklch to sRGB, each channel 0..1 and gamut-clipped.
/// Reference: Björn Ottosson's OKLab, the same matrices CSS Color 4 uses.
fn oklch_to_srgb(l: f32, c: f32, h_deg: f32) -> [f32; 3] {
    let h = h_deg.to_radians();
    let a = c * h.cos();
    let b = c * h.sin();

    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);

    let r = 4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_93 * s3;
    let g = -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_4 * s3;
    let b = -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3;

    [gamma_encode(r), gamma_encode(g), gamma_encode(b)]
}

fn gamma_encode(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.003_130_8 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

/// sRGB (0..1) to HSL with all components 0..1, gpui's `Hsla` convention.
fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let delta = max - min;
    if delta < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let h = if (max - r).abs() < f32::EPSILON {
        ((g - b) / delta).rem_euclid(6.0)
    } else if (max - g).abs() < f32::EPSILON {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (h / 6.0, s, l)
}

/// Comet softens syntax saturation so code stays colourful without shouting.
fn graph_tone(mut color: Hsla) -> Hsla {
    color.s *= 0.72;
    color
}

// ── Syntax palette ───────────────────────────────────────────────────

/// Paint-only colours for one Syntect [`HighlightKind`] each.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxPalette {
    pub comment: Hsla,
    pub keyword: Hsla,
    pub string: Hsla,
    pub string_special: Hsla,
    pub escape: Hsla,
    pub number: Hsla,
    pub boolean: Hsla,
    pub type_name: Hsla,
    pub type_builtin: Hsla,
    pub constructor: Hsla,
    pub function: Hsla,
    pub function_builtin: Hsla,
    pub macro_name: Hsla,
    pub property: Hsla,
    pub constant: Hsla,
    pub variable: Hsla,
    pub variable_special: Hsla,
    pub parameter: Hsla,
    pub operator: Hsla,
    pub punctuation: Hsla,
    pub tag: Hsla,
    pub attribute: Hsla,
    pub label: Hsla,
    pub invalid: Hsla,
}

impl SyntaxPalette {
    /// Colour for one capture kind.
    pub fn color(&self, kind: HighlightKind) -> Hsla {
        match kind {
            HighlightKind::Comment => self.comment,
            HighlightKind::Keyword => self.keyword,
            HighlightKind::String => self.string,
            HighlightKind::StringSpecial => self.string_special,
            HighlightKind::Escape => self.escape,
            HighlightKind::Number => self.number,
            HighlightKind::Boolean => self.boolean,
            HighlightKind::Type => self.type_name,
            HighlightKind::TypeBuiltin => self.type_builtin,
            HighlightKind::Constructor => self.constructor,
            HighlightKind::Function => self.function,
            HighlightKind::FunctionBuiltin => self.function_builtin,
            HighlightKind::Macro => self.macro_name,
            HighlightKind::Property => self.property,
            HighlightKind::Constant => self.constant,
            HighlightKind::Variable => self.variable,
            HighlightKind::VariableSpecial => self.variable_special,
            HighlightKind::Parameter => self.parameter,
            HighlightKind::Operator => self.operator,
            HighlightKind::Punctuation | HighlightKind::Embedded => self.punctuation,
            HighlightKind::Tag => self.tag,
            HighlightKind::Attribute => self.attribute,
            HighlightKind::Label => self.label,
            HighlightKind::Invalid => self.invalid,
        }
    }

    fn dark(text: Hsla, comment: Hsla, danger: Hsla) -> Self {
        let indigo = graph_tone(oklch(0.673, 0.182, 276.935));
        let pink = graph_tone(oklch(0.718, 0.202, 349.761));
        let emerald = graph_tone(oklch(0.765, 0.177, 163.223));
        let amber = graph_tone(oklch(0.828, 0.189, 84.429));
        let red = graph_tone(danger);
        Self {
            comment,
            keyword: indigo,
            string: emerald,
            string_special: pink,
            escape: pink,
            number: amber,
            boolean: amber,
            type_name: amber,
            type_builtin: emerald,
            constructor: amber,
            function: indigo,
            function_builtin: pink,
            macro_name: pink,
            property: amber,
            constant: emerald,
            variable: text,
            variable_special: pink,
            parameter: text,
            operator: text,
            punctuation: text,
            tag: pink,
            attribute: amber,
            label: amber,
            invalid: red,
        }
    }
}

// ── Metrics ──────────────────────────────────────────────────────────

/// Every number that decides layout in `<code>`, `<diff>` and `<markdown>`.
///
/// These used to be Rust `const`s, which meant that tuning a row height needed
/// a native rebuild. They travel in the `theme` prop instead, so the whole
/// design surface changes with a React re-render and no rebuild at all. The
/// defaults are Comet's numbers.
///
/// Row heights in particular are load-bearing: `<diff>` virtualizes with a
/// height model it computes without measuring, so every row kind must know its
/// height up front.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    // Code blocks. Shared by `<code>` and the markdown fenced block.
    pub code_text_size: f32,
    pub code_line_height: f32,
    /// Gutter width per line-number digit.
    pub code_gutter_digit_width: f32,
    pub code_gutter_padding_right: f32,
    pub code_gutter_min_width: f32,

    // Diffs.
    pub diff_text_size: f32,
    pub diff_line_height: f32,
    pub diff_file_header_height: f32,
    pub diff_hunk_header_height: f32,
    pub diff_notice_height: f32,
    pub diff_body_bottom_pad: f32,
    /// Minimum width of one line-number gutter column.
    pub diff_gutter_width: f32,
    /// The `+` / `−` / `·` marker column.
    pub diff_marker_width: f32,
    pub diff_accent_bar_width: f32,
    pub diff_row_padding_x: f32,

    // Markdown.
    pub md_text_size: f32,
    pub md_line_height: f32,
    pub md_block_gap: f32,
    /// Font size for h1, h2, h3, and h4-h6.
    pub md_heading_sizes: [f32; 4],
    /// Line height for h1, h2, h3, and h4-h6.
    pub md_heading_line_heights: [f32; 4],
    pub md_table_cell_padding: f32,
    /// Width below which a column stops shrinking; wider content wraps.
    pub md_table_min_column_width: f32,
    /// Floor for a column's max-content share, so a short column beside a prose
    /// column keeps a readable width.
    pub md_table_min_column_content: f32,
    pub md_inline_code_radius: f32,
    // The fenced-block card. `<code>` paints no card of its own, so these are
    // markdown-only: a document renderer owns its layout, a primitive does not.
    pub md_code_padding_x: f32,
    pub md_code_padding_y: f32,
    pub md_code_radius: f32,
    pub md_code_header_padding_y: f32,
    pub md_code_header_text_size: f32,
}

impl Metrics {
    /// `(font size, line height)` for a heading level.
    pub fn heading(&self, level: u8) -> (f32, f32) {
        let ix = (level.clamp(1, 6) as usize - 1).min(3);
        (self.md_heading_sizes[ix], self.md_heading_line_heights[ix])
    }

    fn apply(&mut self, o: &MetricsOverride) {
        // Metrics come straight from JS, so a typo can hand us NaN, a negative
        // size, or a value that overflows `f32` to infinity. Any of those turn
        // into invalid Taffy geometry and gpui renders nothing at all, with no
        // error to explain it. Reject them and keep the default.
        let set = |slot: &mut f32, value: Option<f64>| {
            let Some(value) = value else { return };
            let value = value as f32;
            if value.is_finite() && value >= 0.0 {
                *slot = value;
            } else {
                log::warn!("ignoring invalid theme metric {value}");
            }
        };
        set(&mut self.code_text_size, o.code_text_size);
        set(&mut self.code_line_height, o.code_line_height);
        set(&mut self.md_code_padding_x, o.md_code_padding_x);
        set(&mut self.md_code_padding_y, o.md_code_padding_y);
        set(&mut self.md_code_radius, o.md_code_radius);
        set(
            &mut self.md_code_header_padding_y,
            o.md_code_header_padding_y,
        );
        set(
            &mut self.md_code_header_text_size,
            o.md_code_header_text_size,
        );
        set(&mut self.code_gutter_digit_width, o.code_gutter_digit_width);
        set(
            &mut self.code_gutter_padding_right,
            o.code_gutter_padding_right,
        );
        set(&mut self.code_gutter_min_width, o.code_gutter_min_width);

        set(&mut self.diff_text_size, o.diff_text_size);
        set(&mut self.diff_line_height, o.diff_line_height);
        set(&mut self.diff_file_header_height, o.diff_file_header_height);
        set(&mut self.diff_hunk_header_height, o.diff_hunk_header_height);
        set(&mut self.diff_notice_height, o.diff_notice_height);
        set(&mut self.diff_body_bottom_pad, o.diff_body_bottom_pad);
        set(&mut self.diff_gutter_width, o.diff_gutter_width);
        set(&mut self.diff_marker_width, o.diff_marker_width);
        set(&mut self.diff_accent_bar_width, o.diff_accent_bar_width);
        set(&mut self.diff_row_padding_x, o.diff_row_padding_x);

        set(&mut self.md_text_size, o.md_text_size);
        set(&mut self.md_line_height, o.md_line_height);
        set(&mut self.md_block_gap, o.md_block_gap);
        set(&mut self.md_table_cell_padding, o.md_table_cell_padding);
        set(
            &mut self.md_table_min_column_width,
            o.md_table_min_column_width,
        );
        set(
            &mut self.md_table_min_column_content,
            o.md_table_min_column_content,
        );
        set(&mut self.md_inline_code_radius, o.md_inline_code_radius);
        if let Some(sizes) = &o.md_heading_sizes {
            for (slot, value) in self.md_heading_sizes.iter_mut().zip(sizes) {
                *slot = *value as f32;
            }
        }
        if let Some(heights) = &o.md_heading_line_heights {
            for (slot, value) in self.md_heading_line_heights.iter_mut().zip(heights) {
                *slot = *value as f32;
            }
        }
    }

    /// Hash the metrics so an element can tell when its layout inputs changed.
    /// `f32` is not `Hash`, so the bit patterns stand in.
    /// Hash only the metrics that decide `<diff>` ROW HEIGHTS.
    ///
    /// The virtualized list caches measured heights per index, so it must be
    /// reset when a row height moves. It must NOT be reset for a markdown or
    /// code tweak: resetting drops the scroll anchor, and changing
    /// `mdTextSize` would jump every diff on screen back to the top.
    pub fn hash_diff_layout_into(&self, hasher: &mut impl std::hash::Hasher) {
        let mut feed = |value: f32| hasher.write_u32(value.to_bits());
        for value in [
            self.diff_line_height,
            self.diff_file_header_height,
            self.diff_hunk_header_height,
            self.diff_notice_height,
            self.diff_body_bottom_pad,
        ] {
            feed(value);
        }
    }

    pub fn hash_into(&self, hasher: &mut impl std::hash::Hasher) {
        let mut feed = |value: f32| hasher.write_u32(value.to_bits());
        for value in [
            self.code_text_size,
            self.code_line_height,
            self.md_code_padding_x,
            self.md_code_padding_y,
            self.md_code_radius,
            self.md_code_header_padding_y,
            self.md_code_header_text_size,
            self.code_gutter_digit_width,
            self.code_gutter_padding_right,
            self.code_gutter_min_width,
            self.diff_text_size,
            self.diff_line_height,
            self.diff_file_header_height,
            self.diff_hunk_header_height,
            self.diff_notice_height,
            self.diff_body_bottom_pad,
            self.diff_gutter_width,
            self.diff_marker_width,
            self.diff_accent_bar_width,
            self.diff_row_padding_x,
            self.md_text_size,
            self.md_line_height,
            self.md_block_gap,
            self.md_table_cell_padding,
            self.md_table_min_column_width,
            self.md_table_min_column_content,
            self.md_inline_code_radius,
        ] {
            feed(value);
        }
        for value in self.md_heading_sizes {
            feed(value);
        }
        for value in self.md_heading_line_heights {
            feed(value);
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            code_text_size: 12.5,
            code_line_height: 18.0,
            code_gutter_digit_width: 7.0,
            code_gutter_padding_right: 12.0,
            code_gutter_min_width: 28.0,

            diff_text_size: 12.0,
            diff_line_height: 21.0,
            diff_file_header_height: 36.0,
            diff_hunk_header_height: 28.0,
            diff_notice_height: 24.0,
            diff_body_bottom_pad: 8.0,
            diff_gutter_width: 36.0,
            diff_marker_width: 28.0,
            diff_accent_bar_width: 3.0,
            diff_row_padding_x: 16.0,

            md_text_size: 14.0,
            md_line_height: 22.0,
            md_block_gap: 12.0,
            // Tight monochrome scale: headings step down quickly toward body
            // size, so a document of h2s does not read as a wall of titles.
            md_heading_sizes: [19.0, 16.0, 15.0, 14.0],
            md_heading_line_heights: [27.0, 24.0, 22.0, 22.0],
            md_table_cell_padding: 12.0,
            md_table_min_column_width: 96.0,
            md_table_min_column_content: 48.0,
            md_inline_code_radius: 4.5,
            md_code_padding_x: 12.0,
            md_code_padding_y: 10.0,
            md_code_radius: 10.0,
            md_code_header_padding_y: 5.0,
            md_code_header_text_size: 11.0,
        }
    }
}

// ── Theme ────────────────────────────────────────────────────────────

/// The token set the native text components paint with.
///
/// This is a trimmed Comet theme: only tokens read by the native editors and
/// document elements remain. Surfaces, buttons and chrome tokens are the host
/// app's business and stay in JS as ordinary `style` props.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Whether this is a dark theme, which is what CSS `light-dark()` reads.
    pub dark: bool,
    /// Content plane behind code and diff bodies.
    pub bg: Hsla,
    /// Hairline border.
    pub border: Hsla,
    /// Primary text.
    pub text: Hsla,
    /// Secondary labels, code-block language tag, copy button.
    pub text_muted: Hsla,
    /// Placeholders and diff gutter numbers.
    pub text_faint: Hsla,
    /// Diff file paths — one notch below `text_muted`, sampled in Comet as #989898.
    pub text_dim: Hsla,
    /// Accent (indigo) — list markers, blockquote rail, selection wash.
    pub accent: Hsla,
    /// Text-editor caret.
    pub caret: Hsla,
    /// Inline-code text, violet-300.
    pub code_text: Hsla,
    /// Rounded wash behind inline code, violet-400 at 12%.
    pub code_wash: Hsla,
    /// Diff added lines, emerald-400.
    pub diff_add: Hsla,
    /// Diff deleted lines, red-400.
    pub diff_del: Hsla,
    /// Diff hunk-header wash.
    pub diff_hunk_bg: Hsla,
    /// Paint-only syntax colours.
    pub syntax: SyntaxPalette,
    /// UI font family.
    pub font_sans: String,
    /// Monospace family for code, diffs and terminals.
    pub font_mono: String,
    /// Every layout number. Overridable from JS, so a design tweak needs no
    /// native rebuild.
    pub metrics: Metrics,
}

impl Theme {
    /// Comet's dark theme, token for token.
    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: grey(6),
            border: hsla(0.0, 0.0, 1.0, 0.08),
            text: neutral(0.922),
            text_muted: neutral(0.708),
            text_faint: neutral(0.556),
            text_dim: grey(0x98),
            accent: oklch(0.673, 0.182, 276.935),
            caret: oklch(0.673, 0.182, 276.935),
            code_text: oklch(0.811, 0.111, 293.571),
            code_wash: with_alpha(oklch(0.702, 0.183, 293.541), 0.12),
            diff_add: oklch(0.765, 0.177, 163.223),
            diff_del: oklch(0.704, 0.191, 22.216),
            diff_hunk_bg: hsla(0.6, 0.35, 0.6, 0.05),
            syntax: SyntaxPalette::dark(neutral(0.922), neutral(0.60), oklch(0.704, 0.191, 22.216)),
            font_sans: system_sans().to_string(),
            font_mono: system_mono().to_string(),
            metrics: Metrics::default(),
        }
    }

    /// Comet's light theme.
    pub fn light() -> Self {
        Self {
            dark: false,
            bg: grey(0xff),
            border: hsla(0.0, 0.0, 0.0, 0.10),
            text: neutral(0.25),
            text_muted: neutral(0.439),
            text_faint: neutral(0.535),
            text_dim: neutral(0.50),
            accent: oklch(0.511, 0.262, 276.966),
            caret: oklch(0.511, 0.262, 276.966),
            code_text: oklch(0.491, 0.27, 292.581),
            code_wash: with_alpha(oklch(0.541, 0.281, 293.009), 0.10),
            diff_add: oklch(0.596, 0.145, 163.225),
            diff_del: oklch(0.577, 0.245, 27.325),
            diff_hunk_bg: hsla(0.6, 0.35, 0.35, 0.07),
            syntax: SyntaxPalette::dark(neutral(0.25), neutral(0.48), oklch(0.505, 0.213, 27.518)),
            font_sans: system_sans().to_string(),
            font_mono: system_mono().to_string(),
            metrics: Metrics::default(),
        }
    }

    /// Apply a JS-supplied override on top of this theme.
    pub fn with_override(mut self, o: &ThemeOverride) -> Self {
        if o.appearance.as_deref() == Some("light") {
            self = Theme::light();
        }
        set(&mut self.bg, &o.bg);
        set(&mut self.border, &o.border);
        set(&mut self.text, &o.text);
        set(&mut self.text_muted, &o.text_muted);
        set(&mut self.text_faint, &o.text_faint);
        set(&mut self.text_dim, &o.text_dim);
        set(&mut self.accent, &o.accent);
        set(&mut self.caret, &o.caret);
        set(&mut self.code_text, &o.code_text);
        set(&mut self.code_wash, &o.code_wash);
        set(&mut self.diff_add, &o.diff_add);
        set(&mut self.diff_del, &o.diff_del);
        set(&mut self.diff_hunk_bg, &o.diff_hunk_bg);
        if let Some(font) = &o.font_mono {
            self.font_mono = font.clone();
        }
        if let Some(font) = &o.font_sans {
            self.font_sans = font.clone();
        }
        if let Some(metrics) = &o.metrics {
            self.metrics.apply(metrics);
        }
        if let Some(s) = &o.syntax {
            let p = &mut self.syntax;
            set(&mut p.comment, &s.comment);
            set(&mut p.keyword, &s.keyword);
            set(&mut p.string, &s.string);
            set(&mut p.string_special, &s.string_special);
            set(&mut p.escape, &s.escape);
            set(&mut p.number, &s.number);
            set(&mut p.boolean, &s.boolean);
            set(&mut p.type_name, &s.type_name);
            set(&mut p.type_builtin, &s.type_builtin);
            set(&mut p.constructor, &s.constructor);
            set(&mut p.function, &s.function);
            set(&mut p.function_builtin, &s.function_builtin);
            set(&mut p.macro_name, &s.macro_name);
            set(&mut p.property, &s.property);
            set(&mut p.constant, &s.constant);
            set(&mut p.variable, &s.variable);
            set(&mut p.variable_special, &s.variable_special);
            set(&mut p.parameter, &s.parameter);
            set(&mut p.operator, &s.operator);
            set(&mut p.punctuation, &s.punctuation);
            set(&mut p.tag, &s.tag);
            set(&mut p.attribute, &s.attribute);
            set(&mut p.label, &s.label);
            set(&mut p.invalid, &s.invalid);
        }
        self
    }

    /// Build from an optional JSON prop value. `null` or a parse failure yields dark.
    pub fn from_prop(value: Option<&serde_json::Value>) -> Self {
        let Some(value) = value.filter(|v| !v.is_null()) else {
            return Theme::dark();
        };
        match serde_json::from_value::<ThemeOverride>(value.clone()) {
            Ok(o) => Theme::dark().with_override(&o),
            Err(e) => {
                log::warn!("invalid theme prop, using dark: {e}");
                Theme::dark()
            }
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

fn set(slot: &mut Hsla, value: &Option<String>) {
    if let Some(color) = value.as_deref().and_then(crate::color::parse_color_rgba) {
        *slot = color.into();
    }
}

fn with_alpha(mut color: Hsla, alpha: f32) -> Hsla {
    color.a = alpha;
    color
}

fn system_sans() -> &'static str {
    if cfg!(target_family = "wasm") {
        "IBM Plex Sans"
    } else if cfg!(target_os = "macos") {
        "Helvetica"
    } else if cfg!(target_os = "windows") {
        "Segoe UI"
    } else {
        "DejaVu Sans"
    }
}

fn system_mono() -> &'static str {
    if cfg!(target_family = "wasm") {
        "Lilex"
    } else if cfg!(target_os = "macos") {
        "Menlo"
    } else if cfg!(target_os = "windows") {
        "Consolas"
    } else {
        "DejaVu Sans Mono"
    }
}

// ── JS-facing override ───────────────────────────────────────────────

/// A `theme` prop from JS. Every field is a CSS colour string and optional.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeOverride {
    /// `"dark"` (default) or `"light"` — picks the base before overrides apply.
    pub appearance: Option<String>,
    pub bg: Option<String>,
    pub border: Option<String>,
    pub text: Option<String>,
    pub text_muted: Option<String>,
    pub text_faint: Option<String>,
    pub text_dim: Option<String>,
    pub accent: Option<String>,
    pub caret: Option<String>,
    pub code_text: Option<String>,
    pub code_wash: Option<String>,
    pub diff_add: Option<String>,
    pub diff_del: Option<String>,
    pub diff_hunk_bg: Option<String>,
    pub font_sans: Option<String>,
    pub font_mono: Option<String>,
    pub syntax: Option<SyntaxOverride>,
    pub metrics: Option<MetricsOverride>,
}

/// A `theme.metrics` object from JS. Every field is a pixel number.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MetricsOverride {
    pub code_text_size: Option<f64>,
    pub code_line_height: Option<f64>,
    pub code_gutter_digit_width: Option<f64>,
    pub code_gutter_padding_right: Option<f64>,
    pub code_gutter_min_width: Option<f64>,

    pub diff_text_size: Option<f64>,
    pub diff_line_height: Option<f64>,
    pub diff_file_header_height: Option<f64>,
    pub diff_hunk_header_height: Option<f64>,
    pub diff_notice_height: Option<f64>,
    pub diff_body_bottom_pad: Option<f64>,
    pub diff_gutter_width: Option<f64>,
    pub diff_marker_width: Option<f64>,
    pub diff_accent_bar_width: Option<f64>,
    pub diff_row_padding_x: Option<f64>,

    pub md_text_size: Option<f64>,
    pub md_line_height: Option<f64>,
    pub md_block_gap: Option<f64>,
    /// `[h1, h2, h3, h4to6]`. A shorter array leaves the rest alone.
    pub md_heading_sizes: Option<Vec<f64>>,
    pub md_heading_line_heights: Option<Vec<f64>>,
    pub md_table_cell_padding: Option<f64>,
    pub md_table_min_column_width: Option<f64>,
    pub md_table_min_column_content: Option<f64>,
    pub md_inline_code_radius: Option<f64>,
    pub md_code_padding_x: Option<f64>,
    pub md_code_padding_y: Option<f64>,
    pub md_code_radius: Option<f64>,
    pub md_code_header_padding_y: Option<f64>,
    pub md_code_header_text_size: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyntaxOverride {
    pub comment: Option<String>,
    pub keyword: Option<String>,
    pub string: Option<String>,
    pub string_special: Option<String>,
    pub escape: Option<String>,
    pub number: Option<String>,
    pub boolean: Option<String>,
    pub type_name: Option<String>,
    pub type_builtin: Option<String>,
    pub constructor: Option<String>,
    pub function: Option<String>,
    pub function_builtin: Option<String>,
    pub macro_name: Option<String>,
    pub property: Option<String>,
    pub constant: Option<String>,
    pub variable: Option<String>,
    pub variable_special: Option<String>,
    pub parameter: Option<String>,
    pub operator: Option<String>,
    pub punctuation: Option<String>,
    pub tag: Option<String>,
    pub attribute: Option<String>,
    pub label: Option<String>,
    pub invalid: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn srgb_u8(c: [f32; 3]) -> [u8; 3] {
        [
            (c[0] * 255.0).round() as u8,
            (c[1] * 255.0).round() as u8,
            (c[2] * 255.0).round() as u8,
        ]
    }

    /// Reference values from CSS Color 4 matrices — the same assertion Comet makes.
    #[test]
    fn oklch_matches_tailwind_reference() {
        assert_eq!(
            srgb_u8(oklch_to_srgb(0.673, 0.182, 276.935)),
            [124, 134, 255]
        );
        assert_eq!(
            srgb_u8(oklch_to_srgb(0.704, 0.191, 22.216)),
            [255, 100, 103]
        );
        assert_eq!(srgb_u8(oklch_to_srgb(0.828, 0.189, 84.429)), [255, 185, 0]);
    }

    #[test]
    fn dark_theme_tokens_are_distinct() {
        let t = Theme::dark();
        assert_ne!(t.text, t.text_muted);
        assert_ne!(t.diff_add, t.diff_del);
        assert_ne!(t.syntax.keyword, t.syntax.string);
        assert_eq!(t.syntax.variable, t.text);
    }

    #[test]
    fn override_replaces_only_named_tokens() {
        let base = Theme::dark();
        let o: ThemeOverride = serde_json::from_str(
            r##"{ "text": "#ff0000", "caret": "#0000ff", "syntax": { "keyword": "#00ff00" }, "unknownKey": 1 }"##,
        )
        .unwrap();
        let t = base.clone().with_override(&o);
        assert_eq!(t.text, gpui::rgba(0xff0000ff).into());
        assert_eq!(t.caret, gpui::rgba(0x0000ffff).into());
        assert_eq!(t.syntax.keyword, gpui::rgba(0x00ff00ff).into());
        assert_eq!(t.diff_add, base.diff_add);
        assert_eq!(t.syntax.string, base.syntax.string);
    }

    #[test]
    fn override_accepts_full_color_functions() {
        let base = Theme::dark();
        let o: ThemeOverride = serde_json::from_str(
            r#"{
          "text": "oklch(0.62796 0.25768 29.23388)",
          "caret": "hsl(240 100% 50%)",
          "syntax": { "keyword": "rebeccapurple" }
        }"#,
        )
        .unwrap();
        let t = base.with_override(&o);
        let text: gpui::Rgba = t.text.into();
        assert!((text.r - 1.0).abs() < 0.001);
        assert!(text.g.abs() < 0.001);
        assert!(text.b.abs() < 0.001);
        assert!((text.a - 1.0).abs() < f32::EPSILON);
        assert_eq!(t.caret, gpui::rgba(0x0000ffff).into());
        assert_eq!(t.syntax.keyword, gpui::rgba(0x663399ff).into());
    }

    #[test]
    fn metrics_override_touches_only_named_numbers() {
        let base = Theme::dark();
        let o: ThemeOverride = serde_json::from_str(
            r#"{ "metrics": { "diffLineHeight": 30, "mdHeadingSizes": [40, 32] } }"#,
        )
        .unwrap();
        let t = base.clone().with_override(&o);
        assert_eq!(t.metrics.diff_line_height, 30.0);
        assert_eq!(t.metrics.md_heading_sizes[0], 40.0);
        assert_eq!(t.metrics.md_heading_sizes[1], 32.0);
        // A short array leaves the remaining tiers alone.
        assert_eq!(
            t.metrics.md_heading_sizes[2],
            base.metrics.md_heading_sizes[2]
        );
        assert_eq!(t.metrics.code_line_height, base.metrics.code_line_height);
    }

    #[test]
    fn diff_layout_hash_tracks_row_heights_and_nothing_else() {
        let hash = |m: &Metrics| {
            use std::hash::Hasher;
            let mut h = std::collections::hash_map::DefaultHasher::new();
            m.hash_diff_layout_into(&mut h);
            h.finish()
        };
        let base = Metrics::default();
        assert_eq!(hash(&base), hash(&Metrics::default()));

        // A row height moves, so the measured-height cache has to drop.
        let mut taller = base;
        taller.diff_line_height += 1.0;
        assert_ne!(hash(&base), hash(&taller));

        let mut header = base;
        header.diff_hunk_header_height += 1.0;
        assert_ne!(hash(&base), hash(&header));

        // A markdown tweak leaves diff rows where they are. Hashing it would
        // drop the scroll anchor and jump every diff on screen back to the top.
        let mut heading = base;
        heading.md_heading_sizes[2] += 1.0;
        assert_eq!(hash(&base), hash(&heading));
    }

    #[test]
    fn light_appearance_switches_the_base() {
        let o: ThemeOverride = serde_json::from_str(r#"{ "appearance": "light" }"#).unwrap();
        let t = Theme::dark().with_override(&o);
        assert_eq!(t.bg, grey(0xff));
    }
}
