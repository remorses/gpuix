//! Clipboard text for the JS binding and the text editor's paste.
//!
//! Only string entries are modelled. Images, files and metadata stay inside
//! GPUI: an app that copied a file should not receive a path to paste.

use gpui::{ClipboardEntry, ClipboardItem};

/// Write `text` as the only clipboard entry.
///
/// An empty string is a real entry, so it clears the clipboard instead of
/// leaving the previous contents behind.
pub fn write_text(cx: &gpui::App, text: String) {
    cx.write_to_clipboard(ClipboardItem::new_string(text));
}

/// The clipboard's text, or `None` when it holds no text at all.
pub fn read(cx: &gpui::App) -> Option<String> {
    text(cx.read_from_clipboard())
}

/// Text to paste into a text field. A file-only clipboard has no text: pasting
/// its paths would insert something the user never copied.
pub fn paste_text(item: Option<ClipboardItem>) -> Option<String> {
    let item = item?;
    if item
        .entries
        .iter()
        .any(|entry| matches!(entry, ClipboardEntry::ExternalPaths(_)))
    {
        return None;
    }
    text(Some(item))
}

/// Every string entry, joined in clipboard order.
///
/// An empty entry is `Some("")`. `ClipboardItem::text` cannot express that: it
/// returns `None` for empty text, which makes `copyText("")` look like the
/// clipboard never changed.
fn text(item: Option<ClipboardItem>) -> Option<String> {
    let item = item?;
    let mut answer = String::new();
    let mut found = false;
    for entry in &item.entries {
        if let ClipboardEntry::String(string) = entry {
            found = true;
            answer.push_str(string.text());
        }
    }
    found.then_some(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{ClipboardString, ExternalPaths, Image};
    use std::path::PathBuf;

    fn item(entries: Vec<ClipboardEntry>) -> Option<ClipboardItem> {
        Some(ClipboardItem { entries })
    }

    #[test]
    fn empty_string_is_text_not_absence() {
        assert_eq!(
            text(item(vec![ClipboardEntry::String(ClipboardString::new(
                String::new()
            ))])),
            Some(String::new())
        );
    }

    #[test]
    fn no_text_entries_is_absence() {
        assert_eq!(text(item(Vec::new())), None);
        assert_eq!(
            text(item(vec![ClipboardEntry::Image(Image::empty())])),
            None
        );
        assert_eq!(text(None), None);
    }

    #[test]
    fn joins_string_entries_in_order() {
        assert_eq!(
            text(item(vec![
                ClipboardEntry::String(ClipboardString::new("one".to_string())),
                ClipboardEntry::Image(Image::empty()),
                ClipboardEntry::String(ClipboardString::new("two".to_string())),
            ])),
            Some("onetwo".to_string())
        );
    }

    #[test]
    fn external_paths_are_not_text_editor_paste() {
        assert_eq!(
            paste_text(item(vec![
                ClipboardEntry::ExternalPaths(ExternalPaths(
                    [PathBuf::from("/tmp/image.png")].into_iter().collect()
                )),
                ClipboardEntry::String(ClipboardString::new("/tmp/image.png".to_string())),
            ])),
            None
        );
    }

    #[test]
    fn text_still_pastes_when_the_clipboard_also_has_an_image() {
        assert_eq!(
            paste_text(item(vec![
                ClipboardEntry::String(ClipboardString::new("caption".to_string())),
                ClipboardEntry::Image(Image::empty()),
            ])),
            Some("caption".to_string())
        );
    }
}
