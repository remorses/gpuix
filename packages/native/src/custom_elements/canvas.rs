/// `<canvas>` paints a GPUCanvas snapshot into the GPUI scene via `paint_image`.
///
/// The WebGPU device is a separate wgpu instance from the window, so Linux
/// `paint_surface` cannot sample these textures yet.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use gpui::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub struct CanvasFactory;

impl CustomElementFactory for CanvasFactory {
    fn element_type(&self) -> &str {
        "canvas"
    }

    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(CanvasElement::default())
    }
}

#[derive(Default)]
pub struct CanvasElement {
    source: Option<u64>,
    last_image: Rc<RefCell<Option<Arc<gpui::RenderImage>>>>,
}

impl CustomElement for CanvasElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        let host_id = gpui::SharedString::from(format!("__gpuix_canvas_{}", ctx.id));
        let source = self.source;
        let last_image = self.last_image.clone();
        let el = super::custom_surface(
            gpui::div()
                .id(host_id)
                .overflow_hidden()
                .child(
                    gpui::canvas(
                        |_, _, _| (),
                        {
                            let last_image = last_image.clone();
                            move |bounds, _, window, _| {
                                let next = source.and_then(crate::webgpu::canvas_snapshot);
                                let previous = last_image.borrow_mut().take();
                                if let Some(previous) = previous {
                                    let changed = next
                                        .as_ref()
                                        .is_none_or(|image| !Arc::ptr_eq(image, &previous));
                                    if changed {
                                        if let Err(error) = window.drop_image(previous) {
                                            log::warn!("drop_image failed: {error:#}");
                                        }
                                    }
                                }
                                if let Some(image) = next {
                                    if let Err(error) = window.paint_image(
                                        bounds,
                                        bounds,
                                        gpui::Corners::default(),
                                        image.clone(),
                                        0,
                                        false,
                                    ) {
                                        log::warn!("paint_image failed: {error:#}");
                                    } else {
                                        *last_image.borrow_mut() = Some(image);
                                    }
                                    window.request_animation_frame();
                                }
                            }
                        },
                    )
                    .size_full(),
                ),
            &ctx,
        );
        el.into_any_element()
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        if key == "source" {
            self.source = match &value {
                serde_json::Value::Number(number) => number.as_u64().or_else(|| {
                    number.as_f64().and_then(|value| {
                        if value.is_finite() && value >= 0.0 && value.fract() == 0.0 {
                            Some(value as u64)
                        } else {
                            None
                        }
                    })
                }),
                serde_json::Value::Object(object) => object.get("id").and_then(|value| {
                    value
                        .as_u64()
                        .or_else(|| value.as_f64().map(|value| value as u64))
                }),
                _ => None,
            };
        }
    }

    fn supported_props(&self) -> &'static [&'static str] {
        &["source"]
    }

    fn supported_events(&self) -> &'static [&'static str] {
        &["click", "mouseEnter", "mouseLeave"]
    }

    fn destroy(&mut self) {}
}
