use std::time::Duration;

use gpui::{Animation, AnimationExt, Font, Hsla, Rgba, SharedString, TextRun};

use super::{CustomElement, CustomElementFactory, CustomRenderContext};

const DEFAULT_DURATION_SECONDS: f64 = 2.0;

pub struct ShimmerFactory;

impl CustomElementFactory for ShimmerFactory {
    fn element_type(&self) -> &str {
        "shimmer"
    }

    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(ShimmerElement::default())
    }
}

#[derive(Debug, Clone)]
pub struct ShimmerElement {
    text: String,
    base_color: String,
    highlight_color: String,
    duration: f64,
}

impl Default for ShimmerElement {
    fn default() -> Self {
        Self {
            text: String::new(),
            base_color: "#8b8b8b".to_string(),
            highlight_color: "#ffffff".to_string(),
            duration: DEFAULT_DURATION_SECONDS,
        }
    }
}

impl CustomElement for ShimmerElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        let root = super::custom_surface(
            gpui::div().id(SharedString::from(format!("__gpuix_shimmer_{}", ctx.id))),
            &ctx,
        );
        if self.text.is_empty() {
            return root.into_any_element();
        }

        let text = SharedString::from(self.text.clone());
        let byte_lengths: Vec<usize> = self.text.chars().map(char::len_utf8).collect();
        let glyph_count = byte_lengths.len();
        let base = parse_color(
            &self.base_color,
            Rgba {
                r: 0.55,
                g: 0.55,
                b: 0.55,
                a: 1.0,
            },
        );
        let highlight = parse_color(
            &self.highlight_color,
            Rgba {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
        );
        let mut font = ctx
            .style
            .and_then(|style| style.font_family.as_deref())
            .map(gpui::font)
            .unwrap_or_default();
        if let Some(weight) = ctx.style.and_then(|style| style.font_weight.as_ref()) {
            font.weight = crate::renderer::parse_font_weight(weight);
        }
        let duration = if self.duration.is_finite() && self.duration > 0.0 {
            self.duration.clamp(0.1, 60.0)
        } else {
            DEFAULT_DURATION_SECONDS
        };
        let animation_id = SharedString::from(format!("__gpuix_shimmer_animation_{}", ctx.id));
        let animated = gpui::div().with_animation(
            animation_id,
            Animation::new(Duration::from_secs_f64(duration))
                .repeat_synced()
                .with_max_fps(30.0),
            move |el, phase| {
                let runs = shimmer_runs(&byte_lengths, &font, base, highlight, phase, glyph_count);
                el.child(crate::text::chrome_text(text.clone(), Some(runs)))
            },
        );

        root.child(animated).into_any_element()
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "text" => self.text = value.as_str().unwrap_or("").to_string(),
            "baseColor" => {
                self.base_color = value.as_str().unwrap_or("#8b8b8b").to_string();
            }
            "highlightColor" => {
                self.highlight_color = value.as_str().unwrap_or("#ffffff").to_string();
            }
            "duration" => {
                self.duration = value
                    .as_f64()
                    .filter(|value| value.is_finite() && *value > 0.0)
                    .unwrap_or(DEFAULT_DURATION_SECONDS);
            }
            _ => {}
        }
    }

    fn supported_props(&self) -> &'static [&'static str] {
        &["text", "baseColor", "highlightColor", "duration"]
    }

    fn supported_events(&self) -> &'static [&'static str] {
        &[]
    }

    fn destroy(&mut self) {}
}

fn parse_color(value: &str, fallback: Rgba) -> Rgba {
    crate::color::parse_color_rgba(value).unwrap_or(fallback)
}

fn shimmer_runs(
    byte_lengths: &[usize],
    font: &Font,
    base: Rgba,
    highlight: Rgba,
    phase: f32,
    glyph_count: usize,
) -> Vec<TextRun> {
    let center = phase * (glyph_count as f32 + 2.0) - 1.0;
    byte_lengths
        .iter()
        .enumerate()
        .map(|(index, &len)| {
            let amount = (1.0 - (index as f32 - center).abs() / 1.6).clamp(0.0, 1.0);
            TextRun {
                len,
                font: font.clone(),
                color: mix_color(base, highlight, amount),
                background_color: None,
                underline: None,
                strikethrough: None,
            }
        })
        .collect()
}

fn mix_color(base: Rgba, highlight: Rgba, amount: f32) -> Hsla {
    let amount = amount.clamp(0.0, 1.0);
    Rgba {
        r: base.r + (highlight.r - base.r) * amount,
        g: base.g + (highlight.g - base.g) * amount,
        b: base.b + (highlight.b - base.b) * amount,
        a: base.a + (highlight.a - base.a) * amount,
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_exact_utf8_runs_and_moves_the_highlight() {
        let lengths: Vec<usize> = "Wørk".chars().map(char::len_utf8).collect();
        let base = Rgba {
            r: 0.2,
            g: 0.2,
            b: 0.2,
            a: 1.0,
        };
        let highlight = Rgba {
            r: 0.9,
            g: 0.9,
            b: 0.9,
            a: 1.0,
        };
        let start = shimmer_runs(
            &lengths,
            &Font::default(),
            base,
            highlight,
            0.0,
            lengths.len(),
        );
        let middle = shimmer_runs(
            &lengths,
            &Font::default(),
            base,
            highlight,
            0.5,
            lengths.len(),
        );

        assert_eq!(start.iter().map(|run| run.len).sum::<usize>(), "Wørk".len());
        assert_eq!(
            middle.iter().map(|run| run.len).sum::<usize>(),
            "Wørk".len()
        );
        assert_ne!(start, middle);
    }
}
