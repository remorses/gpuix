use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use serde::Deserialize;
use web_time::Instant;

pub struct CanvasFactory;

impl CustomElementFactory for CanvasFactory {
    fn element_type(&self) -> &str {
        "canvas"
    }

    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(CanvasElement {
            shapes: Vec::new(),
            start_time: Instant::now(),
        })
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PathCommand {
    #[serde(rename = "type")]
    pub cmd_type: String,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub x1: Option<f32>,
    pub y1: Option<f32>,
    pub x2: Option<f32>,
    pub y2: Option<f32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CanvasShape {
    pub shape_type: Option<String>,
    pub path: Option<Vec<PathCommand>>,
    pub fill: Option<String>,
    pub stroke: Option<String>,
    pub stroke_width: Option<f32>,
    pub squash: Option<f32>,
    pub breathe_loop: Option<f32>,
    pub glance_x: Option<f32>,
    pub glance_y: Option<f32>,
    pub blink: Option<f32>,
    pub blink_loop: Option<f32>,
    pub wiggle: Option<f32>,
    pub wiggle_loop: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct CanvasElement {
    shapes: Vec<CanvasShape>,
    start_time: Instant,
}

impl CustomElement for CanvasElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut gpui::Window,
        _cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        let shapes = self.shapes.clone();
        let mut has_animation = false;
        for shape in &shapes {
            if shape.breathe_loop.is_some() || shape.blink_loop.is_some() || shape.wiggle_loop.is_some() {
                has_animation = true;
            }
        }

        if has_animation {
            window.request_animation_frame();
        }

        let element_id = gpui::SharedString::from(format!("__gpuix_canvas_{}", ctx.id));
        let start_time = self.start_time;

        let el = gpui::canvas(move |bounds, window| {
            let now = window.cx().now();
            let elapsed = now.duration_since(start_time).as_secs_f64();
            
            for shape in &shapes {
                let Some(cmds) = &shape.path else { continue };

                let mut fill = None;
                if let Some(f) = &shape.fill {
                    if let Some(c) = crate::color::parse_color_rgba(f) {
                        fill = Some(c);
                    }
                }

                let mut stroke = None;
                if let Some(s) = &shape.stroke {
                    if let Some(c) = crate::color::parse_color_rgba(s) {
                        stroke = Some(c);
                    }
                }
                
                let stroke_width = shape.stroke_width.unwrap_or(1.0);

                let mut bounds_min_x = f32::MAX;
                let mut bounds_max_x = f32::MIN;
                let mut bounds_min_y = f32::MAX;
                let mut bounds_max_y = f32::MIN;

                for cmd in cmds {
                    if let Some(x) = cmd.x {
                        bounds_min_x = bounds_min_x.min(x);
                        bounds_max_x = bounds_max_x.max(x);
                    }
                    if let Some(y) = cmd.y {
                        bounds_min_y = bounds_min_y.min(y);
                        bounds_max_y = bounds_max_y.max(y);
                    }
                }

                let cx = (bounds_min_x + bounds_max_x) / 2.0;
                let cy = (bounds_min_y + bounds_max_y) / 2.0;

                let mut scale_x = 1.0;
                let mut scale_y = 1.0;
                let mut trans_x = bounds.origin.x.0;
                let mut trans_y = bounds.origin.y.0;

                if let Some(squash) = shape.squash {
                    let mut s = squash;
                    if let Some(speed) = shape.breathe_loop {
                        s *= (elapsed * speed as f64).sin() as f32;
                    }
                    scale_x += s;
                    scale_y -= s;
                }

                if let Some(blink) = shape.blink {
                    let mut b = blink;
                    if let Some(speed) = shape.blink_loop {
                        b *= ((elapsed * speed as f64).sin() as f32).max(0.0);
                    }
                    scale_y *= (1.0 - b).max(0.1);
                }

                if let Some(glance_x) = shape.glance_x {
                    trans_x += glance_x;
                }
                if let Some(glance_y) = shape.glance_y {
                    trans_y += glance_y;
                }
                
                let wiggle_amp = shape.wiggle.unwrap_or(0.0);
                let wiggle_speed = shape.wiggle_loop.unwrap_or(0.0) as f64;

                let tx = |x: f32, y: f32| -> gpui::Point<gpui::Pixels> {
                    let mut nx = (x - cx) * scale_x + cx;
                    let mut ny = (y - cy) * scale_y + cy;
                    if wiggle_amp > 0.0 && wiggle_speed > 0.0 {
                        nx += ((elapsed * wiggle_speed + y as f64 * 0.1).sin() as f32) * wiggle_amp;
                        ny += ((elapsed * wiggle_speed + x as f64 * 0.1).cos() as f32) * wiggle_amp;
                    }
                    gpui::point(gpui::px(nx + trans_x), gpui::px(ny + trans_y))
                };

                let mut draw_path = |is_stroke: bool| {
                    let mut builder = if is_stroke {
                        gpui::PathBuilder::stroke(gpui::px(stroke_width))
                    } else {
                        gpui::PathBuilder::fill()
                    };

                    for cmd in cmds {
                        match cmd.cmd_type.as_str() {
                            "move" => builder.move_to(tx(cmd.x.unwrap_or(0.0), cmd.y.unwrap_or(0.0))),
                            "line" => builder.line_to(tx(cmd.x.unwrap_or(0.0), cmd.y.unwrap_or(0.0))),
                            "curve" => {
                                let ctrl1 = tx(cmd.x1.unwrap_or(0.0), cmd.y1.unwrap_or(0.0));
                                let ctrl2 = tx(cmd.x2.unwrap_or(0.0), cmd.y2.unwrap_or(0.0));
                                let to = tx(cmd.x.unwrap_or(0.0), cmd.y.unwrap_or(0.0));
                                builder.cubic_bezier_to(to, ctrl1, ctrl2);
                            }
                            "close" => builder.close(),
                            _ => {}
                        }
                    }

                    builder.build()
                };

                if let Some(f) = fill {
                    if let Ok(path) = draw_path(false) {
                        window.paint_path(path, f);
                    }
                }
                if let Some(s) = stroke {
                    if let Ok(path) = draw_path(true) {
                        window.paint_path(path, s);
                    }
                }
            }
        }).size_full();

        let mut div = gpui::div().id(element_id).size_full().child(el);
        if let Some(style) = ctx.style {
            div = crate::renderer::apply_interactive_styles(div, style);
        }
        let div = super::wire_standard_events(div, &ctx);
        crate::automation::track_own_bounds(div, ctx.id).into_any_element()
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        if key == "shapes" {
            if let Ok(shapes) = serde_json::from_value::<Vec<CanvasShape>>(value) {
                self.shapes = shapes;
            }
        }
    }

    fn supported_props(&self) -> &'static [&'static str] {
        &["shapes"]
    }

    fn supported_events(&self) -> &'static [&'static str] {
        &["click", "mouseEnter", "mouseLeave"]
    }

    fn destroy(&mut self) {}
}
