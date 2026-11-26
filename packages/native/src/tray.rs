//! Tray items: the small status icons the OS shows outside app windows.
//!
//! GPUI owns the item (`App::create_tray` -> `TrayHandle`) and event delivery
//! (`App::on_tray_event`). This module only holds the napi payloads and maps
//! them to the GPUI values. GPUI tray events carry no tray identity, so
//! GPUIX exposes one process-wide tray; multi-tray is out of scope.
//!
//! The handle must only be touched on the GPUI app/UI thread: macOS TrayHandle
//! wraps an NSStatusItem that AppKit owns, and the Windows/Linux backends
//! create their window/channel on the UI thread. `GpuixRenderer` stores it in
//! an app-thread local and routes every call through `update_app`, never from
//! the Node thread.

use napi_derive::napi;

/// Options for the process-wide tray item.
#[napi(object)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrayOptions {
    /// Path to an image shown in the tray. `.icns` and `.png` on macOS, `.ico`
    /// on Windows, anything the `image` crate decodes on Linux. When the image
    /// cannot be loaded, or this is `None`, the item falls back to `title`.
    pub icon: Option<String>,
    /// Text shown when there is no icon.
    pub title: Option<String>,
    /// Text shown while the pointer hovers over the item.
    pub tooltip: Option<String>,
}

impl TrayOptions {
    /// The GPUI value this payload names.
    pub fn into_gpui(self) -> gpui::TrayOptions {
        gpui::TrayOptions {
            icon: self.icon.map(std::path::PathBuf::from),
            title: self.title.map(Into::into),
            tooltip: self.tooltip.map(Into::into),
        }
    }
}

/// One entry of the tray item's menu.
#[napi(object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayMenuItem {
    /// Identifies the item in the `menuItem` event.
    pub id: String,
    /// The text shown for the item.
    pub label: String,
    /// Whether choosing the item reports an event. Defaults to true.
    pub enabled: Option<bool>,
    /// Whether the item is drawn with a checkmark. Defaults to false.
    pub checked: Option<bool>,
}

impl TrayMenuItem {
    /// The GPUI value this payload names.
    pub fn into_gpui(self) -> gpui::TrayMenuItem {
        gpui::TrayMenuItem {
            id: self.id.into(),
            label: self.label.into(),
            enabled: self.enabled.unwrap_or(true),
            checked: self.checked.unwrap_or(false),
        }
    }
}

/// An event reported by the tray item. Flat: GPUI tray menus have no
/// submenus, separators, or accelerators.
#[napi(object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayEvent {
    /// `"click"` when the item was clicked while it had no menu,
    /// `"menuItem"` when a menu item was chosen.
    pub r#type: String,
    /// The chosen menu item's `id`. `null` for `click`.
    pub id: Option<String>,
}

impl TrayEvent {
    /// The event name, `"click"` or `"menuItem"`.
    pub fn event_type(&self) -> &str {
        self.r#type.as_str()
    }
}

impl From<gpui::TrayEvent> for TrayEvent {
    fn from(event: gpui::TrayEvent) -> Self {
        match event {
            gpui::TrayEvent::Click => Self {
                r#type: "click".to_string(),
                id: None,
            },
            gpui::TrayEvent::MenuItem(id) => Self {
                r#type: "menuItem".to_string(),
                id: Some(id.to_string()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_gpui_options() {
        let options = TrayOptions {
            icon: Some("/tmp/icon.png".into()),
            title: Some("GPUIX".into()),
            tooltip: Some("Hover".into()),
        }
        .into_gpui();

        assert_eq!(options.icon, Some("/tmp/icon.png".into()));
        assert_eq!(options.title.as_deref(), Some("GPUIX"));
        assert_eq!(options.tooltip.as_deref(), Some("Hover"));
    }

    #[test]
    fn menu_item_defaults_enabled_and_unchecked() {
        let item = TrayMenuItem {
            id: "quit".into(),
            label: "Quit".into(),
            enabled: None,
            checked: None,
        }
        .into_gpui();

        assert!(item.enabled);
        assert!(!item.checked);
        assert_eq!(item.id.as_ref(), "quit");
    }

    #[test]
    fn maps_events_back() {
        assert_eq!(
            TrayEvent::from(gpui::TrayEvent::Click),
            TrayEvent {
                r#type: "click".into(),
                id: None,
            }
        );
        assert_eq!(
            TrayEvent::from(gpui::TrayEvent::MenuItem("open".into())),
            TrayEvent {
                r#type: "menuItem".into(),
                id: Some("open".into()),
            }
        );
    }
}
