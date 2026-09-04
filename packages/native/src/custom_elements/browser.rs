use std::{cell::RefCell, rc::Rc};

use serde::{Deserialize, Serialize};

use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use crate::renderer::EventCallback;

#[cfg(all(target_os = "macos", feature = "native-browser-cef"))]
#[path = "browser_cef.rs"]
mod cef_backend;

#[cfg(all(target_os = "macos", feature = "native-browser-cef"))]
use cef_backend::BrowserRuntime;

pub struct BrowserFactory;

impl CustomElementFactory for BrowserFactory {
    fn element_type(&self) -> &str {
        "browser"
    }

    fn create(&self, id: u64) -> Box<dyn CustomElement> {
        Box::new(BrowserElement::new(id))
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserCommand {
    #[serde(default)]
    serial: u64,
    #[serde(default)]
    kind: String,
    value: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct BrowserConfig {
    source: String,
    profile_id: String,
    profile_path: String,
    incognito: bool,
    visible: bool,
    commands: Vec<BrowserCommand>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserStateEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    can_go_back: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    can_go_forward: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command_serial: Option<u64>,
}

pub struct BrowserElement {
    id: u64,
    config: BrowserConfig,
    runtime: Rc<RefCell<BrowserRuntime>>,
}

impl BrowserElement {
    fn new(id: u64) -> Self {
        Self {
            id,
            config: BrowserConfig::default(),
            runtime: Rc::new(RefCell::new(BrowserRuntime::default())),
        }
    }
}

impl CustomElement for BrowserElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        let runtime = self.runtime.clone();
        let config = self.config.clone();
        let callback = ctx.event_callback.clone();
        let id = self.id;
        let canvas = gpui::canvas(
            move |bounds, window, _cx| {
                if let Err(error) = runtime
                    .borrow_mut()
                    .update(&config, bounds, window, &callback, id)
                {
                    emit_value(&callback, id, "browserError", error);
                }
            },
            |_bounds, _, _window, _cx| {},
        )
        .size_full();
        super::custom_surface(
            gpui::div().id(gpui::SharedString::from(format!(
                "__gpuix_browser_{}",
                ctx.id
            ))),
            &ctx,
        )
        .child(canvas)
        .into_any_element()
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "source" => self.config.source = value.as_str().unwrap_or_default().to_string(),
            "profileId" => self.config.profile_id = value.as_str().unwrap_or_default().to_string(),
            "profilePath" => {
                self.config.profile_path = value.as_str().unwrap_or_default().to_string()
            }
            "incognito" => self.config.incognito = value.as_bool().unwrap_or(false),
            "visible" => self.config.visible = value.as_bool().unwrap_or(false),
            "command" => {
                self.config.commands = value.as_str().map(decode_commands).unwrap_or_default()
            }
            _ => {}
        }
    }

    fn supported_props(&self) -> &'static [&'static str] {
        &[
            "source",
            "profileId",
            "profilePath",
            "incognito",
            "visible",
            "command",
        ]
    }

    fn supported_events(&self) -> &'static [&'static str] {
        &["browserState", "browserOpen", "browserError"]
    }

    fn destroy(&mut self) {
        self.runtime.borrow_mut().destroy();
    }
}

fn decode_commands(wire: &str) -> Vec<BrowserCommand> {
    serde_json::from_str::<Vec<BrowserCommand>>(wire)
        .or_else(|_| serde_json::from_str::<BrowserCommand>(wire).map(|command| vec![command]))
        .unwrap_or_default()
}

fn emit_value(callback: &Option<EventCallback>, id: u64, event: &str, value: String) {
    crate::renderer::emit_event_full(callback, id, event, |payload| {
        payload.value = Some(value);
    });
}

fn emit_state(callback: &Option<EventCallback>, id: u64, state: BrowserStateEvent) {
    if let Ok(value) = serde_json::to_string(&state) {
        emit_value(callback, id, "browserState", value);
    }
}

#[cfg(not(all(target_os = "macos", feature = "native-browser-cef")))]
#[derive(Default)]
struct BrowserRuntime {
    reported: bool,
}

#[cfg(not(all(target_os = "macos", feature = "native-browser-cef")))]
impl BrowserRuntime {
    fn update(
        &mut self,
        config: &BrowserConfig,
        _bounds: gpui::Bounds<gpui::Pixels>,
        _window: &mut gpui::Window,
        callback: &Option<EventCallback>,
        id: u64,
    ) -> Result<(), String> {
        if !self.reported && !config.source.is_empty() {
            self.reported = true;
            emit_value(
                callback,
                id,
                "browserError",
                "This GPUix build does not include a native browser engine".to_string(),
            );
        }
        Ok(())
    }

    fn destroy(&mut self) {
        self.reported = false;
    }
}

pub fn initialize(root_cache_path: Option<&str>) {
    #[cfg(all(target_os = "macos", feature = "native-browser-cef"))]
    cef_backend::initialize(root_cache_path);
    #[cfg(not(all(target_os = "macos", feature = "native-browser-cef")))]
    let _ = root_cache_path;
}

pub fn do_message_loop_work() {
    #[cfg(all(target_os = "macos", feature = "native-browser-cef"))]
    cef_backend::do_message_loop_work();
}

pub fn shutdown() -> Result<(), String> {
    #[cfg(all(target_os = "macos", feature = "native-browser-cef"))]
    {
        return cef_backend::shutdown();
    }
    #[cfg(not(all(target_os = "macos", feature = "native-browser-cef")))]
    Ok(())
}

pub fn initialization_error() -> Option<String> {
    #[cfg(all(target_os = "macos", feature = "native-browser-cef"))]
    {
        return cef_backend::initialization_error();
    }
    #[cfg(not(all(target_os = "macos", feature = "native-browser-cef")))]
    None
}

pub fn available() -> bool {
    #[cfg(all(target_os = "macos", feature = "native-browser-cef"))]
    {
        return cef_backend::available();
    }
    #[cfg(not(all(target_os = "macos", feature = "native-browser-cef")))]
    false
}

pub fn engine() -> &'static str {
    if available() {
        "chromium"
    } else {
        "unavailable"
    }
}

pub fn profile_isolation() -> &'static str {
    if available() {
        "full"
    } else {
        "limited"
    }
}

#[cfg(test)]
mod tests {
    use super::decode_commands;

    #[test]
    fn command_wire_accepts_fifo_and_legacy_object() {
        let queued =
            decode_commands(r#"[{"serial":1,"kind":"clearData"},{"serial":2,"kind":"reload"}]"#);
        assert_eq!(queued.len(), 2);
        assert_eq!(queued[0].kind, "clearData");
        assert_eq!(queued[1].serial, 2);

        let legacy = decode_commands(r#"{"serial":3,"kind":"focus"}"#);
        assert_eq!(legacy.len(), 1);
        assert_eq!(legacy[0].kind, "focus");
    }
}
