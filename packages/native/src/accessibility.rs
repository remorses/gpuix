//! GPUI accessibility props from React custom_props.
//!
//! A node reaches AccessKit only with both an `.id(...)` and a `.role(...)`.
//! GPUIX already sets the id. This module sets the role and ARIA fields.
//! Prop names match React DOM: `aria-label`, not `ariaLabel`.
//!
//! `aria-activedescendant` is deliberately unmapped. The web prop is a
//! container-side id reference, while GPUI's `aria_active_descendant()` takes
//! no argument and must be called on the selected child. An arbitrary id
//! cannot be turned into a child-side call, so the attribute is dropped.
//!
//! `aria-invalid`, `aria-required`, `aria-modal`, `aria-current`, and
//! `aria-haspopup` have no builder in the pinned GPUI fork.

use std::collections::HashMap;

use gpui::prelude::*;
use gpui::{AccessibleAction, Orientation, Role, Toggled};

use crate::element_tree::EventPayload;
use crate::renderer::{emit_event_full, EventCallback};

/// Apply `role` and `aria-*` from React, plus a default role when the app
/// did not set one. `none` / `presentation` produce no node.
pub(crate) fn apply_accessibility<E>(
    mut el: E,
    props: &HashMap<String, serde_json::Value>,
    default_role: Option<Role>,
) -> E
where
    E: StatefulInteractiveElement,
{
    let explicit = props.get("role").and_then(|value| value.as_str());
    if matches!(explicit, Some("none" | "presentation")) {
        return el;
    }

    let role = explicit
        .and_then(role_from_aria)
        .or(default_role)
        .filter(|role| *role != Role::GenericContainer);
    let Some(role) = role else {
        return el;
    };
    el = el.role(role);

    if let Some(id) = string_prop(props, "aria-id") {
        el = el.accessibility_id(id);
    }
    if let Some(label) = string_prop(props, "aria-label") {
        el = el.aria_label(label);
    }
    if let Some(description) = string_prop(props, "aria-description") {
        el = el.aria_description(description);
    }
    if let Some(value) = string_prop(props, "aria-valuetext") {
        el = el.aria_value(value);
    }
    if let Some(expanded) = bool_prop(props, "aria-expanded") {
        el = el.aria_expanded(expanded);
    }
    if let Some(selected) = bool_prop(props, "aria-selected") {
        el = el.aria_selected(selected);
    }
    if let Some(level) = usize_prop(props, "aria-level") {
        el = el.aria_level(level);
    }
    if let Some(keyshortcuts) = string_prop(props, "aria-keyshortcuts") {
        el = el.aria_keyshortcuts(keyshortcuts);
    }
    if let Some(placeholder) = string_prop(props, "aria-placeholder") {
        el = el.aria_placeholder(placeholder);
    }
    // AccessKit has one toggled state for both: checkboxes report it as
    // checked, toggle buttons as pressed.
    if let Some(toggled) =
        toggled_prop(props, "aria-checked").or_else(|| toggled_prop(props, "aria-pressed"))
    {
        el = el.aria_toggled(toggled);
    }
    if let Some(orientation) = orientation_prop(props, "aria-orientation") {
        el = el.aria_orientation(orientation);
    }
    if let Some(value) = number_prop(props, "aria-valuenow") {
        el = el.aria_numeric_value(value);
    }
    if let Some(value) = number_prop(props, "aria-valuemin") {
        el = el.aria_min_numeric_value(value);
    }
    if let Some(value) = number_prop(props, "aria-valuemax") {
        el = el.aria_max_numeric_value(value);
    }
    if let Some(position) = usize_prop(props, "aria-posinset") {
        el = el.aria_position_in_set(position);
    }
    if let Some(size) = usize_prop(props, "aria-setsize") {
        el = el.aria_size_of_set(size);
    }
    if let Some(index) = usize_prop(props, "aria-rowindex") {
        el = el.aria_row_index(index);
    }
    if let Some(index) = usize_prop(props, "aria-colindex") {
        el = el.aria_column_index(index);
    }
    if let Some(count) = usize_prop(props, "aria-rowcount") {
        el = el.aria_row_count(count);
    }
    if let Some(count) = usize_prop(props, "aria-colcount") {
        el = el.aria_column_count(count);
    }
    el
}

/// `aria-label`, or `alt` when that is empty. Used by `<img>`.
pub(crate) fn apply_image_label<E>(
    el: E,
    props: &HashMap<String, serde_json::Value>,
    alt: &str,
) -> E
where
    E: StatefulInteractiveElement,
{
    if props.get("aria-label").is_some() || alt.is_empty() {
        return el;
    }
    el.aria_label(alt.to_string())
}

/// VoiceOver Press. GPUI auto-adds Click only for `.on_click()`. GPUIX
/// click is `on_mouse_up`, so register the action by hand and emit the
/// same JS `click` event.
pub(crate) fn apply_a11y_click<E>(
    el: E,
    events: &std::collections::HashSet<String>,
    element_id: u64,
    callback: &Option<EventCallback>,
) -> E
where
    E: StatefulInteractiveElement,
{
    if !events.contains("click") {
        return el;
    }
    let callback = callback.clone();
    el.on_a11y_action(AccessibleAction::Click, move |_data, _window, _cx| {
        emit_event_full(
            &callback,
            element_id,
            "click",
            |payload: &mut EventPayload| {
                payload.button = Some(0);
                payload.click_count = Some(1);
                payload.is_right_click = Some(false);
            },
        );
    })
}

fn string_prop(props: &HashMap<String, serde_json::Value>, key: &str) -> Option<String> {
    props
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

fn bool_prop(props: &HashMap<String, serde_json::Value>, key: &str) -> Option<bool> {
    props.get(key).and_then(|value| value.as_bool())
}

fn usize_prop(props: &HashMap<String, serde_json::Value>, key: &str) -> Option<usize> {
    props
        .get(key)
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
}

/// ARIA numbers are `f64`. A `null`, a string, or a non-finite value is
/// ignored, and negatives stay valid for `aria-valuemin` / `aria-valuemax`.
fn number_prop(props: &HashMap<String, serde_json::Value>, key: &str) -> Option<f64> {
    props
        .get(key)
        .and_then(|value| value.as_f64())
        .filter(|value| value.is_finite())
}

/// `aria-checked` and `aria-pressed` are tri-state: a boolean, or `"mixed"`.
fn toggled_prop(props: &HashMap<String, serde_json::Value>, key: &str) -> Option<Toggled> {
    match props.get(key)? {
        serde_json::Value::Bool(true) => Some(Toggled::True),
        serde_json::Value::Bool(false) => Some(Toggled::False),
        serde_json::Value::String(token) => match token.as_str() {
            "true" => Some(Toggled::True),
            "false" => Some(Toggled::False),
            "mixed" => Some(Toggled::Mixed),
            _ => None,
        },
        _ => None,
    }
}

fn orientation_prop(props: &HashMap<String, serde_json::Value>, key: &str) -> Option<Orientation> {
    match string_prop(props, key)?.as_str() {
        "horizontal" => Some(Orientation::Horizontal),
        "vertical" => Some(Orientation::Vertical),
        _ => None,
    }
}

/// ARIA token → AccessKit role. Unknown tokens are ignored, like a bad style.
fn role_from_aria(token: &str) -> Option<Role> {
    Some(match token {
        "alert" => Role::Alert,
        "alertdialog" => Role::AlertDialog,
        "application" => Role::Application,
        "article" => Role::Article,
        "banner" => Role::Banner,
        "blockquote" => Role::Blockquote,
        "button" => Role::Button,
        "caption" => Role::Caption,
        "cell" => Role::Cell,
        "checkbox" => Role::CheckBox,
        "code" => Role::Code,
        "columnheader" => Role::ColumnHeader,
        "combobox" => Role::ComboBox,
        "complementary" => Role::Complementary,
        "contentinfo" => Role::ContentInfo,
        "definition" => Role::Definition,
        "deletion" => Role::ContentDeletion,
        "dialog" => Role::Dialog,
        "document" => Role::Document,
        "emphasis" => Role::Emphasis,
        "feed" => Role::Feed,
        "figure" => Role::Figure,
        "form" => Role::Form,
        "generic" => Role::GenericContainer,
        "grid" => Role::Grid,
        "gridcell" => Role::GridCell,
        "group" => Role::Group,
        "heading" => Role::Heading,
        "img" => Role::Image,
        "insertion" => Role::ContentInsertion,
        "link" => Role::Link,
        "list" => Role::List,
        "listbox" => Role::ListBox,
        "listitem" => Role::ListItem,
        "log" => Role::Log,
        "main" => Role::Main,
        "marquee" => Role::Marquee,
        "math" => Role::Math,
        "menu" => Role::Menu,
        "menubar" => Role::MenuBar,
        "menuitem" => Role::MenuItem,
        "menuitemcheckbox" => Role::MenuItemCheckBox,
        "menuitemradio" => Role::MenuItemRadio,
        "meter" => Role::Meter,
        "navigation" => Role::Navigation,
        "none" | "presentation" => Role::GenericContainer,
        "note" => Role::Note,
        "option" => Role::ListBoxOption,
        "paragraph" => Role::Paragraph,
        "progressbar" => Role::ProgressIndicator,
        "radio" => Role::RadioButton,
        "radiogroup" => Role::RadioGroup,
        "region" => Role::Region,
        "row" => Role::Row,
        "rowgroup" => Role::RowGroup,
        "rowheader" => Role::RowHeader,
        "scrollbar" => Role::ScrollBar,
        "search" => Role::Search,
        "searchbox" => Role::SearchInput,
        "separator" => Role::Splitter,
        "slider" => Role::Slider,
        "spinbutton" => Role::SpinButton,
        "status" => Role::Status,
        "strong" => Role::Strong,
        "subscript" => Role::GenericContainer,
        "superscript" => Role::GenericContainer,
        "switch" => Role::Switch,
        "tab" => Role::Tab,
        "table" => Role::Table,
        "tablist" => Role::TabList,
        "tabpanel" => Role::TabPanel,
        "term" => Role::Term,
        "textbox" => Role::TextInput,
        "time" => Role::Time,
        "timer" => Role::Timer,
        "toolbar" => Role::Toolbar,
        "tooltip" => Role::Tooltip,
        "tree" => Role::Tree,
        "treegrid" => Role::TreeGrid,
        "treeitem" => Role::TreeItem,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{number_prop, orientation_prop, role_from_aria, toggled_prop};
    use gpui::{Orientation, Role, Toggled};
    use serde_json::json;
    use std::collections::HashMap;

    fn props(value: serde_json::Value) -> HashMap<String, serde_json::Value> {
        HashMap::from([("aria".to_string(), value)])
    }

    #[test]
    fn maps_common_aria_tokens() {
        assert_eq!(role_from_aria("button"), Some(Role::Button));
        assert_eq!(role_from_aria("heading"), Some(Role::Heading));
        assert_eq!(role_from_aria("textbox"), Some(Role::TextInput));
        assert_eq!(role_from_aria("img"), Some(Role::Image));
        assert_eq!(role_from_aria("option"), Some(Role::ListBoxOption));
        assert_eq!(role_from_aria("none"), Some(Role::GenericContainer));
        assert_eq!(role_from_aria("presentation"), Some(Role::GenericContainer));
        assert_eq!(role_from_aria("Button"), None);
        assert_eq!(role_from_aria("not-a-role"), None);
    }

    #[test]
    fn aria_pressed_is_tristate() {
        for (value, expected) in [
            (json!(true), Some(Toggled::True)),
            (json!(false), Some(Toggled::False)),
            (json!("true"), Some(Toggled::True)),
            (json!("false"), Some(Toggled::False)),
            (json!("mixed"), Some(Toggled::Mixed)),
            (json!("Mixed"), None),
            (json!(1), None),
            (json!(null), None),
        ] {
            assert_eq!(toggled_prop(&props(value), "aria"), expected);
        }
    }

    #[test]
    fn aria_orientation_accepts_only_the_two_tokens() {
        for (value, expected) in [
            (json!("horizontal"), Some(Orientation::Horizontal)),
            (json!("vertical"), Some(Orientation::Vertical)),
            (json!("diagonal"), None),
            (json!(true), None),
        ] {
            assert_eq!(orientation_prop(&props(value), "aria"), expected);
        }
    }

    #[test]
    fn aria_numbers_are_finite_and_optional() {
        for (value, expected) in [
            (json!(42), Some(42.0)),
            (json!(-12.5), Some(-12.5)),
            // JS serializes `NaN` and `Infinity` to `null`, so both arrive as null.
            (json!(null), None),
            (json!("42"), None),
            (json!(false), None),
        ] {
            assert_eq!(number_prop(&props(value), "aria"), expected);
        }
        assert_eq!(number_prop(&HashMap::new(), "aria"), None);
    }
}
