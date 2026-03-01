/// Image custom element — renders raster/SVG images from local file paths via GPUI img().
///
/// This provides a native `<img>` for GPUIX React apps while keeping the same
/// custom-element prop pipeline (`setCustomProp`/`custom_props`).
use super::{CustomElement, CustomElementFactory, CustomRenderContext};

pub struct ImgFactory;

impl CustomElementFactory for ImgFactory {
    fn element_type(&self) -> &str {
        "img"
    }

    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(ImgElement::default())
    }
}

#[derive(Debug, Clone)]
enum ImgObjectFit {
    Fill,
    Contain,
    Cover,
    ScaleDown,
    None,
}

impl Default for ImgObjectFit {
    fn default() -> Self {
        Self::Contain
    }
}

impl ImgObjectFit {
    fn from_str(value: &str) -> Self {
        match value {
            "fill" => Self::Fill,
            "cover" => Self::Cover,
            "scaleDown" => Self::ScaleDown,
            "none" => Self::None,
            _ => Self::Contain,
        }
    }

    fn as_gpui(&self) -> gpui::ObjectFit {
        match self {
            Self::Fill => gpui::ObjectFit::Fill,
            Self::Contain => gpui::ObjectFit::Contain,
            Self::Cover => gpui::ObjectFit::Cover,
            Self::ScaleDown => gpui::ObjectFit::ScaleDown,
            Self::None => gpui::ObjectFit::None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ImgElement {
    src: String,
    object_fit: ImgObjectFit,
}

impl CustomElement for ImgElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        if self.src.trim().is_empty() {
            let mut fallback = gpui::div()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui::rgba(0x1f2230ff))
                .border(gpui::px(1.0))
                .border_color(gpui::rgba(0x5d6481ff))
                .text_color(gpui::rgba(0xa4accdff))
                .child("img: no src");

            if let Some(style) = ctx.style {
                fallback = crate::renderer::apply_styles(fallback, style);
            }

            return fallback.into_any_element();
        }

        let src_path = std::path::PathBuf::from(self.src.clone());
        let mut el = gpui::img(src_path)
            .object_fit(self.object_fit.as_gpui())
            .with_fallback(|| {
                gpui::div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(gpui::rgba(0x1f2230ff))
                    .border(gpui::px(1.0))
                    .border_color(gpui::rgba(0x5d6481ff))
                    .text_color(gpui::rgba(0xa4accdff))
                    .child("img: load failed")
                    .into_any_element()
            });

        if let Some(style) = ctx.style {
            el = crate::renderer::apply_styles(el, style);
        }

        el.into_any_element()
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "src" => self.src = value.as_str().unwrap_or("").to_string(),
            "objectFit" => {
                self.object_fit = value
                    .as_str()
                    .map(ImgObjectFit::from_str)
                    .unwrap_or_default()
            }
            _ => {}
        }
    }

    fn supported_props(&self) -> &[&str] {
        &["src", "objectFit"]
    }

    fn get_prop(&self, key: &str) -> Option<serde_json::Value> {
        match key {
            "src" => Some(serde_json::Value::String(self.src.clone())),
            "objectFit" => Some(serde_json::Value::String(
                match self.object_fit {
                    ImgObjectFit::Fill => "fill",
                    ImgObjectFit::Contain => "contain",
                    ImgObjectFit::Cover => "cover",
                    ImgObjectFit::ScaleDown => "scaleDown",
                    ImgObjectFit::None => "none",
                }
                .to_string(),
            )),
            _ => None,
        }
    }

    fn supported_events(&self) -> &[&str] {
        &[]
    }

    fn destroy(&mut self) {}
}
