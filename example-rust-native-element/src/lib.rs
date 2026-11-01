//! A Node addon that adds `<rust-greeting>` to GPUIX.
//!
//! It links `gpuix-native` as a Rust library, so the built `.node` has every
//! `@gpuix/native` export plus this element. Load it in place of the prebuilt
//! binary with `NAPI_RS_NATIVE_LIBRARY_PATH`.

use gpuix_native::gpui;
use gpuix_native::{
    custom_surface, emit_event_full, CustomElement, CustomElementFactory, CustomRenderContext,
    GpuixView,
};

struct Greeting {
    name: String,
}

impl CustomElement for Greeting {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        let id = ctx.id;
        let callback = ctx.event_callback.clone();
        let name = self.name.clone();
        let root = gpui::div().id(gpui::ElementId::Name(format!("rust_greeting_{id}").into()));
        custom_surface(root, &ctx)
            .on_mouse_up(gpui::MouseButton::Left, move |_, _, _| {
                emit_event_full(&callback, id, "greet", |payload| {
                    payload.detail = Some(serde_json::json!({ "name": name }));
                });
            })
            .child(ctx.text(0, format!("Hello, {}", self.name), None))
            .into_any_element()
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        if key == "name" {
            self.name = value.as_str().unwrap_or_default().to_string();
        }
    }

    fn supported_props(&self) -> &'static [&'static str] {
        &["name"]
    }

    fn supported_events(&self) -> &'static [&'static str] {
        &["greet"]
    }

    fn destroy(&mut self) {}
}

struct GreetingFactory;

impl CustomElementFactory for GreetingFactory {
    fn element_type(&self) -> &str {
        "rust-greeting"
    }

    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(Greeting {
            name: String::new(),
        })
    }
}

#[napi_derive::module_init]
fn init() {
    gpuix_native::register_custom_element(|| Box::new(GreetingFactory))
        .expect("rust-greeting registers once");
}
