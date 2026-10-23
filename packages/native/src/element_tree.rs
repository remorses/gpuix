/// Event types for Rust → JS communication.
/// Element IDs are f64 (JS numbers) — lossless for integers up to 2^53.
///
/// EventPayload is the single struct sent across the napi boundary for ALL
/// event types. Fields are optional — each event type populates only the
/// fields it needs. This avoids N different napi structs while keeping the
/// FFI surface small.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use napi_derive::napi;

/// Event payload sent back to JS when a user interacts with an element.
#[derive(Debug, Clone)]
#[cfg_attr(
    all(target_arch = "wasm32", target_os = "unknown"),
    derive(serde::Serialize)
)]
#[cfg_attr(
    all(target_arch = "wasm32", target_os = "unknown"),
    serde(rename_all = "camelCase")
)]
#[cfg_attr(not(all(target_arch = "wasm32", target_os = "unknown")), napi(object))]
pub struct EventPayload {
    /// Numeric element ID (matches the ID assigned in JS via createElement).
    pub element_id: f64,

    /// Event type string. The JS handler prop is `on` + this name capitalized:
    /// "click" → `onClick`, "mouseDown" → `onMouseDown`.
    pub event_type: String,

    // ── Mouse position ───────────────────────────────────────────────
    /// Mouse X position in window coordinates (pixels).
    pub x: Option<f64>,
    /// Mouse Y position in window coordinates (pixels).
    pub y: Option<f64>,

    // ── Mouse button ─────────────────────────────────────────────────
    /// Which mouse button: 0=left, 1=middle, 2=right.
    /// Populated for: mouseDown, mouseUp, click, mouseDownOutside, contextMenu.
    pub button: Option<u32>,

    /// Number of consecutive clicks (1=single, 2=double, 3=triple).
    /// Populated for: mouseDown, mouseUp, click.
    pub click_count: Option<u32>,

    /// Whether this is a right-click (convenience for click events).
    /// true when button==2 or ClickEvent::is_right_click().
    pub is_right_click: Option<bool>,

    /// Which mouse button is currently held during a mouseMove.
    /// Same encoding as `button`: 0=left, 1=middle, 2=right.
    /// Populated for: mouseMove.
    pub pressed_button: Option<u32>,

    // ── Keyboard ─────────────────────────────────────────────────────
    /// Key name, e.g. "a", "enter", "escape", "down", "left", "f1".
    /// Populated for: keyDown, keyUp.
    pub key: Option<String>,

    /// The character produced by the key press (e.g. "ß" for option-s).
    /// May differ from `key` when modifiers are active.
    /// Populated for: keyDown, keyUp.
    pub key_char: Option<String>,

    /// Whether this is a key-repeat event (key held down).
    /// Populated for: keyDown.
    pub is_held: Option<bool>,

    /// Id of the GPUI key dispatch that produced this event. Every element
    /// `keyDown` and the trailing `windowKeyDown` of one keystroke share it, so
    /// JS can apply `preventDefault` / `stopPropagation` across them.
    /// Populated for: keyDown, keyUp, windowKeyDown, windowKeyUp.
    pub keystroke_id: Option<f64>,

    // ── Scroll ───────────────────────────────────────────────────────
    /// Scroll delta on the X axis (pixels or lines, see `precise`).
    /// Populated for: scroll.
    pub delta_x: Option<f64>,

    /// Scroll delta on the Y axis (pixels or lines, see `precise`).
    /// Populated for: scroll.
    pub delta_y: Option<f64>,

    /// true = pixel-precise (trackpad), false = line-based (mouse wheel).
    /// Populated for: scroll.
    pub precise: Option<bool>,

    /// Touch phase for scroll: "started", "moved", "ended".
    /// Populated for: scroll (trackpad gestures).
    pub touch_phase: Option<String>,

    // ── Hover ────────────────────────────────────────────────────────
    /// true = mouse entered element, false = mouse left element.
    /// Populated for: mouseEnter, mouseLeave.
    pub hovered: Option<bool>,

    // ── Custom element payloads ──────────────────────────────────────
    /// Element-defined string payload.
    /// Populated for: `<diff>` toggleFile (the file path), showMore (the
    /// hidden line count), and lineClick (the line text); `<markdown>`
    /// linkClick (the URL); `selectionChange` (joined selected text, or
    /// absent when the selection is empty).
    pub value: Option<String>,

    /// Line number on the pre-change side. Populated for: `<diff>` lineClick.
    pub old_line: Option<f64>,

    /// Line number on the post-change side. Populated for: `<diff>` lineClick.
    pub new_line: Option<f64>,

    /// First visible logical index. Populated for: `<virtual-list>` visibleRange.
    pub start_index: Option<f64>,

    /// Exclusive end of the visible logical range. Populated for: visibleRange.
    pub end_index: Option<f64>,

    /// Border-box size of the element, in pixels. With `x` and `y`, its position
    /// in window coordinates. Populated for: layout.
    pub width: Option<f64>,
    pub height: Option<f64>,

    /// Scroll offset of the element, the pair `getScrollOffset` returns: 0 at the
    /// start and negative as the content moves. Populated for: scrollChange.
    pub scroll_x: Option<f64>,
    pub scroll_y: Option<f64>,

    /// Matches found by this element's `highlight` prop. Counted once per match
    /// even when it is split across several painted runs, and it counts every
    /// retained match, not only the ones currently on screen.
    /// Populated for: highlight.
    pub match_count: Option<f64>,

    /// Logical native motion target that reached completion.
    /// Populated for: motionComplete.
    pub motion_generation: Option<f64>,

    /// Absolute filesystem paths from a Finder / OS file drop.
    /// Populated for: fileDrop.
    pub paths: Option<Vec<String>>,

    /// Any JSON value, like the DOM's `CustomEvent.detail`. For elements added
    /// with `register_custom_element`, whose data fits no field above.
    pub detail: Option<serde_json::Value>,

    // ── Modifiers ────────────────────────────────────────────────────
    pub modifiers: Option<EventModifiers>,
}

impl Default for EventPayload {
    fn default() -> Self {
        Self {
            element_id: 0.0,
            event_type: String::new(),
            x: None,
            y: None,
            button: None,
            click_count: None,
            is_right_click: None,
            pressed_button: None,
            key: None,
            key_char: None,
            is_held: None,
            keystroke_id: None,
            delta_x: None,
            delta_y: None,
            precise: None,
            touch_phase: None,
            hovered: None,
            value: None,
            old_line: None,
            new_line: None,
            start_index: None,
            end_index: None,
            width: None,
            height: None,
            scroll_x: None,
            scroll_y: None,
            match_count: None,
            motion_generation: None,
            paths: None,
            detail: None,
            modifiers: None,
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(
    all(target_arch = "wasm32", target_os = "unknown"),
    derive(serde::Serialize)
)]
#[cfg_attr(not(all(target_arch = "wasm32", target_os = "unknown")), napi(object))]
pub struct EventModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub cmd: bool,
}

impl Default for EventModifiers {
    fn default() -> Self {
        Self {
            shift: false,
            ctrl: false,
            alt: false,
            cmd: false,
        }
    }
}

/// One highlight wash painted in the last frame, with the boxes it drew.
///
/// The rects matter: a quad never lands in `getPaintedText()`, and a match that
/// soft-wraps must produce one box per visual row. Without the geometry the only
/// way to assert either is a screenshot.
#[derive(Debug, Clone)]
#[cfg_attr(
    all(target_arch = "wasm32", target_os = "unknown"),
    derive(serde::Serialize)
)]
#[cfg_attr(
    all(target_arch = "wasm32", target_os = "unknown"),
    serde(rename_all = "camelCase")
)]
#[cfg_attr(not(all(target_arch = "wasm32", target_os = "unknown")), napi(object))]
pub struct HighlightMatch {
    /// Numeric id of the element that painted the run.
    pub element_id: f64,
    /// Index of the run within that element. 0 for a plain `<text>`.
    pub sub: f64,
    /// The full string of the run, so `text.slice(start, end)` is the match.
    pub text: String,
    /// UTF-16 code-unit offsets into `text`, the units JS strings use.
    pub start: f64,
    pub end: f64,
    pub active: bool,
    pub rects: Vec<HighlightRect>,
}

#[derive(Debug, Clone)]
#[cfg_attr(
    all(target_arch = "wasm32", target_os = "unknown"),
    derive(serde::Serialize)
)]
#[cfg_attr(not(all(target_arch = "wasm32", target_os = "unknown")), napi(object))]
pub struct HighlightRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl From<crate::text::PaintedHighlight> for HighlightMatch {
    fn from(painted: crate::text::PaintedHighlight) -> Self {
        Self {
            element_id: painted.element_id as f64,
            sub: painted.sub as f64,
            text: painted.text.to_string(),
            start: painted.start as f64,
            end: painted.end as f64,
            active: painted.active,
            rects: painted
                .rects
                .into_iter()
                .map(|(x, y, width, height)| HighlightRect {
                    x: x as f64,
                    y: y as f64,
                    width: width as f64,
                    height: height as f64,
                })
                .collect(),
        }
    }
}

/// Convert GPUI Modifiers → our napi EventModifiers.
impl From<gpui::Modifiers> for EventModifiers {
    fn from(m: gpui::Modifiers) -> Self {
        Self {
            shift: m.shift,
            ctrl: m.control,
            alt: m.alt,
            cmd: m.platform, // platform = Cmd on macOS, Win on Windows
        }
    }
}
