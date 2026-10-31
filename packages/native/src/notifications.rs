//! OS notifications, translated from GPUI.
//!
//! GPUI owns delivery (`App::show_system_notification`); this module only holds
//! the napi payloads and builds the GPUI value. Posting when delivery is
//! unavailable is a no-op upstream, so nothing here invents an error.

use napi_derive::napi;

/// One button offered on a notification.
#[napi(object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationAction {
    /// Identifies the action in `NotificationResponse.actionId`.
    pub id: String,
    /// The button's label.
    pub label: String,
}

/// A notification to post.
#[napi(object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationOptions {
    /// Stable identity: posting the same tag replaces the earlier one where
    /// the platform supports it, and responses carry it back.
    pub tag: String,
    /// The headline.
    pub title: String,
    /// The line under the title. Absent is an empty body.
    pub body: Option<String>,
    /// Buttons. Platforms that cannot show them drop them.
    pub actions: Option<Vec<NotificationAction>>,
}

/// What the reader activated: the notification and the action, if any.
#[napi(object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationResponse {
    pub tag: String,
    pub action_id: Option<String>,
}

impl NotificationOptions {
    /// The GPUI value this payload names.
    pub fn into_gpui(self) -> gpui::SystemNotification {
        gpui::SystemNotification {
            tag: self.tag.into(),
            title: self.title.into(),
            body: self.body.unwrap_or_default().into(),
            actions: self
                .actions
                .unwrap_or_default()
                .into_iter()
                .map(|action| gpui::SystemNotificationAction {
                    id: action.id.into(),
                    label: action.label.into(),
                })
                .collect(),
        }
    }
}

impl From<gpui::SystemNotificationResponse> for NotificationResponse {
    fn from(response: gpui::SystemNotificationResponse) -> Self {
        Self {
            tag: response.tag.to_string(),
            action_id: response.action_id.map(|id| id.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_gpui_value() {
        let notification = NotificationOptions {
            tag: "upload-1".into(),
            title: "Upload complete".into(),
            body: Some("3 files".into()),
            actions: Some(vec![NotificationAction {
                id: "open".into(),
                label: "Open".into(),
            }]),
        }
        .into_gpui();

        assert_eq!(notification.tag, "upload-1");
        assert_eq!(notification.title, "Upload complete");
        assert_eq!(notification.body, "3 files");
        assert_eq!(notification.actions.len(), 1);
        assert_eq!(notification.actions[0].id, "open");
    }

    #[test]
    fn absent_body_and_actions_are_empty() {
        let notification = NotificationOptions {
            tag: "t".into(),
            title: "Title".into(),
            body: None,
            actions: None,
        }
        .into_gpui();

        assert_eq!(notification.body, "");
        assert!(notification.actions.is_empty());
    }

    #[test]
    fn maps_an_action_response_back() {
        let response = gpui::SystemNotificationResponse {
            tag: "upload-1".into(),
            action_id: Some("open".into()),
        };

        assert_eq!(
            NotificationResponse::from(response),
            NotificationResponse {
                tag: "upload-1".into(),
                action_id: Some("open".into()),
            }
        );
    }
}
