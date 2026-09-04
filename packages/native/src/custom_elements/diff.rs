//! `<diff>` — a virtualized, syntax-highlighted, selectable unified diff.
//!
//! Ported from Comet (https://github.com/zeronsh/comet), MIT.
//! Original: `crates/ui/src/changes.rs`.
//!
//! ```tsx
//! <diff
//!   patch={unifiedPatch}
//!   wordDiff
//!   maxLines={24}
//!   collapsedPaths={['pnpm-lock.yaml']}
//!   onToggleFile={(e) => {}}
//!   onShowMore={(e) => {}}
//! />
//! ```
//!
//! This element owns parsed diff data. The `scroll` path uses `gpui::list()`
//! so the closure can capture an `Rc` and build only visible rows. The default
//! flow path renders the same rows in a column so a parent can be the scroller.

use std::collections::HashSet;
use std::rc::Rc;

use gpui::{px, BorderStyle, Font, Hsla, SharedString};

use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use crate::diff::{
    annotate_word_diffs, file_notices, flatten_rows, gutter_width, parse_patch, DiffLine, DiffRow,
    FileDiff, LineKind,
};
use crate::renderer::emit_event_full;
use crate::syntax::cache::highlight_cached;
use crate::syntax::HighlightSpan;
use crate::text::runs::runs_for_spans;
use crate::text::{range_rects, SharedSelection};
use crate::theme::Theme;

/// How far past the viewport the list pre-builds rows.
const OVERDRAW: f32 = 1024.0;

// ── Factory ──────────────────────────────────────────────────────────

pub struct DiffFactory;

impl CustomElementFactory for DiffFactory {
    fn element_type(&self) -> &str {
        "diff"
    }

    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(DiffElement::default())
    }
}

// ── Element ──────────────────────────────────────────────────────────

/// Everything a row needs, shared into the `'static` list closure.
///
/// Note what is NOT in here: resolved colours. Spans stay as neutral
/// `HighlightKind`s and the theme is applied while rendering a row, so changing
/// the `theme` prop recolours without reparsing and cannot go stale.
struct DiffData {
    files: Vec<FileDiff>,
    rows: Vec<DiffRow>,
    /// Per-file highlight, indexed by file. `None` when the language is
    /// unknown or the file is binary, which renders plain.
    highlights: Vec<Option<FileHighlight>>,
    show_word_diff: bool,
}

/// Neutral highlight spans for one file, addressed by side and source line.
///
/// A diff interleaves two versions of a file, so both sides are reconstructed
/// and highlighted separately: `old` from the deletions and context, `new` from
/// the additions and context. A deleted line then gets the colours its own
/// version of the file implies.
///
/// The earlier version of this keyed spans by line TEXT, which is unsound: the
/// same string inside and outside a comment, or in two hunks with different
/// parser state, collapses onto whichever interpretation was seen first.
struct FileHighlight {
    old: Vec<Vec<HighlightSpan>>,
    new: Vec<Vec<HighlightSpan>>,
}

impl FileHighlight {
    /// Spans for one diff line. Context lines prefer the post-change side,
    /// which is what the reader is looking at.
    fn spans_for(&self, line: &DiffLine) -> &[HighlightSpan] {
        fn pick<'a>(
            lines: &'a [Vec<HighlightSpan>],
            no: Option<u32>,
        ) -> Option<&'a [HighlightSpan]> {
            let ix = no?.saturating_sub(1) as usize;
            lines.get(ix).map(Vec::as_slice)
        }
        match line.kind {
            LineKind::Del => pick(&self.old, line.old_no),
            LineKind::Add => pick(&self.new, line.new_no),
            _ => pick(&self.new, line.new_no).or_else(|| pick(&self.old, line.old_no)),
        }
        .unwrap_or(&[])
    }
}

#[derive(Default)]
pub struct DiffElement {
    patch: String,
    show_word_diff: bool,
    scroll: bool,
    max_lines: Option<usize>,
    collapsed: HashSet<String>,
    theme: Theme,
    /// Parsed data for the current patch. Rebuilt only when the props change.
    data: Option<Rc<DiffData>>,
    fingerprint: Option<u64>,
    /// Persists across frames so the scroll position survives a re-render.
    list_state: Option<gpui::ListState>,
    /// Metrics hash the list state was last sized for. Row heights come from
    /// the theme now, so a metrics-only change makes the list's cached
    /// measurements stale even though the parse is untouched.
    list_metrics: Option<u64>,
}

impl DiffElement {
    /// Returns the data plus whether it was rebuilt, so the caller knows the
    /// list state's cached row measurements are now stale.
    fn rebuild_if_needed(&mut self) -> (Rc<DiffData>, bool) {
        let fingerprint = self.fingerprint_props();
        if let (Some(data), Some(previous)) = (&self.data, self.fingerprint) {
            if previous == fingerprint {
                return (data.clone(), false);
            }
        }

        let mut files = parse_patch(&self.patch);
        if self.show_word_diff {
            annotate_word_diffs(&mut files);
        }
        let highlights = files.iter().map(file_highlight).collect();
        let collapsed = self.collapsed.clone();
        let rows = flatten_rows(&files, |path| collapsed.contains(path), self.max_lines);

        let data = Rc::new(DiffData {
            files,
            rows,
            highlights,
            show_word_diff: self.show_word_diff,
        });
        self.fingerprint = Some(fingerprint);
        self.data = Some(data.clone());
        (data, true)
    }

    /// Fingerprint of everything that changes the PARSED data. The theme is
    /// deliberately absent: it only affects paint, so a theme change must not
    /// throw away the parse.
    fn fingerprint_props(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.patch.hash(&mut hasher);
        self.show_word_diff.hash(&mut hasher);
        self.max_lines.hash(&mut hasher);
        let mut paths: Vec<&String> = self.collapsed.iter().collect();
        paths.sort();
        paths.hash(&mut hasher);
        hasher.finish()
    }
}

/// A patch only carries the lines its hunks cover, so a file's real line
/// numbers can be arbitrarily large while the visible content is tiny. This
/// bounds the per-side span table so a `@@ -4000000000,1 @@` header cannot ask
/// for a multi-gigabyte allocation.
const MAX_HIGHLIGHT_LINES: usize = 200_000;

/// Highlight a file by parsing each side's VISIBLE lines and mapping the spans
/// back to their real line numbers.
///
/// Only the visible lines are joined into the parse source. Padding the gaps
/// between hunks with blank lines would keep indexes aligned but not parser
/// state, and a hunk at line 600,000 would blow past the syntax byte limit and
/// leave the whole file unhighlighted.
///
/// Syntax state still cannot be recovered from source the patch never contained
/// — a block comment that opens above the hunk will not tint it. That is
/// inherent to highlighting a diff, and Comet accepts the same limit.
fn file_highlight(file: &FileDiff) -> Option<FileHighlight> {
    if file.binary || file.path.is_empty() {
        return None;
    }
    let mut old_visible: Vec<(u32, &str)> = Vec::new();
    let mut new_visible: Vec<(u32, &str)> = Vec::new();
    let (mut old_max, mut new_max) = (0u32, 0u32);
    for hunk in &file.hunks {
        for line in &hunk.lines {
            if line.kind == LineKind::Meta {
                continue;
            }
            if let Some(no) = line.old_no.filter(|no| *no > 0) {
                old_visible.push((no, &line.text));
                old_max = old_max.max(no);
            }
            if let Some(no) = line.new_no.filter(|no| *no > 0) {
                new_visible.push((no, &line.text));
                new_max = new_max.max(no);
            }
        }
    }

    // A rename can change the extension, so each side is detected from its own
    // path. `old.js -> new.py` must not parse the deleted JavaScript as Python.
    let old_path = file.old_path.as_deref().unwrap_or(&file.path);
    let old = highlight_side(&old_visible, old_max, old_path);
    let new = highlight_side(&new_visible, new_max, &file.path);
    (!old.is_empty() || !new.is_empty()).then_some(FileHighlight { old, new })
}

/// Parse the visible lines of one side and scatter the resulting spans into a
/// table indexed by real line number.
fn highlight_side(visible: &[(u32, &str)], max_line: u32, path: &str) -> Vec<Vec<HighlightSpan>> {
    if visible.is_empty() || max_line as usize > MAX_HIGHLIGHT_LINES {
        return Vec::new();
    }
    let source = visible
        .iter()
        .map(|(_, text)| *text)
        .collect::<Vec<_>>()
        .join("\n");
    let Some(document) = highlight_cached(&source, Some(path), None) else {
        return Vec::new();
    };
    let mut lines = vec![Vec::new(); max_line as usize];
    for ((number, _), spans) in visible.iter().zip(document.lines.iter()) {
        // `number` is 1-based and non-zero by construction above.
        lines[*number as usize - 1] = spans.clone();
    }
    lines
}

impl CustomElement for DiffElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        let (data, rebuilt) = self.rebuild_if_needed();
        let theme = self.theme.clone();
        let metrics = theme.metrics;

        if data.rows.is_empty() {
            // Through `custom_surface` like the non-empty branch, or an empty
            // diff would be the one state with no automation bounds and no
            // click or hover events.
            let empty = gpui::div()
                .id(SharedString::from(format!("__gpuix_diff_empty_{}", ctx.id)))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .text_color(theme.text_faint);
            return super::custom_surface(empty, &ctx)
                .child(ctx.chrome_text("No changes", None))
                .into_any_element();
        }

        let element_id = ctx.id;
        let selection = ctx.selection.clone();
        let selectable = ctx.selectable;
        let wash = ctx.selection_wash;
        let highlight_set = ctx.highlight_set.clone();
        let callback = ctx.event_callback.clone();
        let wants_toggle = ctx.events.contains("toggleFile");
        let wants_line_click = ctx.events.contains("lineClick");
        let wants_show_more = ctx.events.contains("showMore");
        let radius = ctx
            .style
            .and_then(|style| ctx.cascade.scope().number(&style.border_radius))
            .unwrap_or(0.0) as f32;
        let row_theme = theme.clone();

        let body = if self.scroll {
            // The list state must outlive the frame, otherwise the scroll offset
            // resets on every render. It must be reset whenever the ROWS change,
            // not merely when their count changes: gpui caches measured heights per
            // index, and a new patch with the same row count would keep them.
            let state = self
                .list_state
                .get_or_insert_with(|| {
                    gpui::ListState::new(data.rows.len(), gpui::ListAlignment::Top, px(OVERDRAW))
                })
                .clone();
            let metrics_hash = {
                use std::hash::Hasher;
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                metrics.hash_diff_layout_into(&mut hasher);
                hasher.finish()
            };
            if rebuilt || self.list_metrics != Some(metrics_hash) {
                // `reset_with_uniform_height` clears `logical_scroll_top`, so a live
                // patch update or a collapse would jump the reader back to the top.
                // Capture the anchor and put it back, clamped to the new row count.
                let anchor = state.logical_scroll_top();
                state.reset_with_uniform_height(
                    data.rows.len(),
                    px(crate::diff::estimated_row_height(&data.rows, &metrics)),
                );
                if anchor.item_ix > 0 && !data.rows.is_empty() {
                    let mut anchor = anchor;
                    anchor.item_ix = anchor.item_ix.min(data.rows.len() - 1);
                    state.scroll_to(anchor);
                }
                self.list_metrics = Some(metrics_hash);
            }

            let row_data = data.clone();
            let selection = selection.clone();
            let callback = callback.clone();
            let row_theme = row_theme.clone();
            gpui::list(state, move |ix, _window, _app| {
                render_row(
                    &row_data,
                    ix,
                    RowContext {
                        element_id,
                        selection: &selection,
                        selectable,
                        wash,
                        highlight_set: highlight_set.clone(),
                        callback: &callback,
                        wants_toggle,
                        wants_line_click,
                        wants_show_more,
                        theme: &row_theme,
                        radius,
                    },
                )
            })
            .with_sizing_behavior(gpui::ListSizingBehavior::Auto)
            .flex_1()
            .into_any_element()
        } else {
            self.list_state = None;
            self.list_metrics = None;
            let mut column = gpui::div().flex().flex_col();
            for ix in 0..data.rows.len() {
                column = column.child(render_row(
                    &data,
                    ix,
                    RowContext {
                        element_id,
                        selection: &selection,
                        selectable,
                        wash,
                        highlight_set: highlight_set.clone(),
                        callback: &callback,
                        wants_toggle,
                        wants_line_click,
                        wants_show_more,
                        theme: &row_theme,
                        radius,
                    },
                ));
            }
            column.into_any_element()
        };

        let mut container = gpui::div()
            .id(SharedString::from(format!("__gpuix_diff_{}", ctx.id)))
            .flex()
            .flex_col()
            .bg(theme.bg);
        if self.scroll {
            container = container.min_h_0();
        }

        super::custom_surface(container, &ctx)
            .child(body)
            .into_any_element()
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "patch" => self.patch = value.as_str().unwrap_or("").to_string(),
            "wordDiff" => self.show_word_diff = value.as_bool().unwrap_or(false),
            "scroll" => self.scroll = value.as_bool().unwrap_or(false),
            "maxLines" => {
                self.max_lines = value.as_u64().map(|n| n as usize);
            }
            "collapsedPaths" => {
                self.collapsed = value
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default()
            }
            "theme" => self.theme = Theme::from_prop(Some(&value)),
            _ => {}
        }
    }

    fn supported_props(&self) -> &'static [&'static str] {
        &[
            "patch",
            "wordDiff",
            "scroll",
            "maxLines",
            "collapsedPaths",
            "theme",
        ]
    }

    fn supported_events(&self) -> &'static [&'static str] {
        &[
            "toggleFile",
            "showMore",
            "lineClick",
            "click",
            "mouseEnter",
            "mouseLeave",
        ]
    }

    fn destroy(&mut self) {
        self.list_state = None;
        self.list_metrics = None;
        self.data = None;
    }
}

// ── Row rendering ────────────────────────────────────────────────────

struct RowContext<'a> {
    element_id: u64,
    selection: &'a SharedSelection,
    selectable: bool,
    wash: Hsla,
    /// Inherited `highlight`, matched per painted row. See
    /// [`crate::text::search::washes_for_native_run`].
    highlight_set: Option<std::sync::Arc<crate::text::HighlightContext>>,
    callback: &'a Option<crate::renderer::EventCallback>,
    wants_toggle: bool,
    wants_line_click: bool,
    wants_show_more: bool,
    /// Applied while rendering, never baked into `DiffData`, so a theme change
    /// repaints without reparsing the patch.
    theme: &'a Theme,
    /// The element's own `borderRadius`, echoed onto the first row.
    ///
    /// GPUI clips a scroll container to its BOUNDS RECTANGLE, never to a
    /// rounded path, so a row that paints a background covers the parent's
    /// rounded corners. The file header is row 0 and is the only row that
    /// paints at the top edge, so it carries the radius itself. Below it the
    /// square edge is correct: it reads as "content continues".
    radius: f32,
}

impl RowContext<'_> {
    /// Selectable text for a row. `sub` must be the row's global index so the
    /// key is stable while the list scrolls.
    fn text(
        &self,
        sub: usize,
        text: String,
        runs: Option<Vec<gpui::TextRun>>,
        extra_wash: Option<Box<dyn Fn(&gpui::TextLayout, &mut gpui::Window)>>,
    ) -> gpui::AnyElement {
        // Content, not chrome: `userSelect: "none"` stops the drag, not the
        // find. `chrome_text` cannot paint a highlight wash, so it stays for
        // the gutter and the file header only.
        crate::text::selectable_text(crate::text::SelectableText {
            extra_wash,
            selectable: self.selectable,
            highlight: self
                .highlight_set
                .clone()
                .map(crate::text::HighlightSource::Native),
            ..crate::text::SelectableText::new(
                self.element_id,
                sub,
                SharedString::from(text),
                runs,
                self.selection.clone(),
                self.wash,
            )
        })
    }
}

fn render_row(data: &DiffData, ix: usize, ctx: RowContext) -> gpui::AnyElement {
    use gpui::prelude::*;

    let Some(row) = data.rows.get(ix).copied() else {
        return gpui::Empty.into_any_element();
    };
    let theme = ctx.theme;
    let m = &theme.metrics;

    match row {
        DiffRow::FileHeader { file } => {
            let Some(file_diff) = data.files.get(file as usize) else {
                return gpui::Empty.into_any_element();
            };
            file_header_row(file_diff, ix, &ctx, theme, ix == 0)
        }
        DiffRow::Notice { file, notice } => {
            let text = data
                .files
                .get(file as usize)
                .and_then(|f| file_notices(f).get(notice as usize).cloned())
                .unwrap_or_default();
            gpui::div()
                .h(px(m.diff_notice_height))
                .w_full()
                .flex_none()
                .flex()
                .items_center()
                .px(px(m.diff_row_padding_x))
                .text_size(px(11.0))
                .text_color(theme.text_faint)
                .child(crate::text::chrome_text(SharedString::from(text), None))
                .into_any_element()
        }
        DiffRow::HunkHeader { file, hunk } => {
            let header = data
                .files
                .get(file as usize)
                .and_then(|f| f.hunks.get(hunk as usize))
                .map(|h| h.header.clone())
                .unwrap_or_default();
            gpui::div()
                .h(px(m.diff_hunk_header_height))
                .w_full()
                .flex_none()
                .flex()
                .items_center()
                .px(px(m.diff_row_padding_x))
                .bg(theme.diff_hunk_bg)
                .font_family(theme.font_mono.clone())
                .text_size(px(11.0))
                .text_color(theme.text_faint)
                .child(crate::text::chrome_text(SharedString::from(header), None))
                .into_any_element()
        }
        DiffRow::Line { file, hunk, line } => {
            let Some(file_diff) = data.files.get(file as usize) else {
                return gpui::Empty.into_any_element();
            };
            let Some(diff_line) = file_diff
                .hunks
                .get(hunk as usize)
                .and_then(|h| h.lines.get(line as usize))
            else {
                return gpui::Empty.into_any_element();
            };
            let spans = data
                .highlights
                .get(file as usize)
                .and_then(|h| h.as_ref())
                .map(|h| h.spans_for(diff_line))
                .unwrap_or(&[]);
            diff_line_row(
                diff_line,
                spans,
                data.show_word_diff,
                gutter_width(file_diff, m),
                ix,
                &ctx,
                theme,
            )
        }
        DiffRow::BodyPad { .. } => gpui::div()
            .w_full()
            .h(px(m.diff_body_bottom_pad))
            .into_any_element(),
        DiffRow::ShowMore { remaining } => show_more_row(remaining, ix, &ctx, theme),
    }
}

fn show_more_row(remaining: u32, ix: usize, ctx: &RowContext, theme: &Theme) -> gpui::AnyElement {
    use gpui::prelude::*;

    let label = if remaining == 1 {
        "Show 1 more line".to_string()
    } else {
        format!("Show {remaining} more lines")
    };

    let mut row = gpui::div()
        .id(SharedString::from(format!("__gpuix_diff_more_{ix}")))
        .w_full()
        .h(px(theme.metrics.diff_file_header_height))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_size(px(theme.metrics.diff_text_size))
        .text_color(theme.text_dim)
        .hover(|s| s.bg(ink(theme, 0.05)));

    if ctx.wants_show_more {
        let callback = ctx.callback.clone();
        let element_id = ctx.element_id;
        row = row.on_click(move |_, _window, _cx| {
            emit_event_full(&callback, element_id, "showMore", |p| {
                p.value = Some(remaining.to_string());
            });
        });
    }

    row.child(crate::text::chrome_text(SharedString::from(label), None))
        .into_any_element()
}

fn file_header_row(
    file: &FileDiff,
    ix: usize,
    ctx: &RowContext,
    theme: &Theme,
    first: bool,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    let m = &theme.metrics;

    let mut header = gpui::div()
        .id(SharedString::from(format!("__gpuix_diff_hdr_{ix}")))
        .w_full()
        .h(px(m.diff_file_header_height))
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .px(px(12.0))
        .bg(ink(theme, 0.025))
        .border_t_1()
        .border_color(ink(theme, 0.04))
        .cursor_pointer()
        .hover(|s| s.bg(ink(theme, 0.05)));

    if first && ctx.radius > 0.0 {
        header = header.rounded_tl(px(ctx.radius)).rounded_tr(px(ctx.radius));
    }

    if ctx.wants_toggle {
        let callback = ctx.callback.clone();
        let element_id = ctx.element_id;
        let path = file.path.clone();
        header = header.on_click(move |_, _window, _cx| {
            emit_event_full(&callback, element_id, "toggleFile", |p| {
                p.value = Some(path.clone());
            });
        });
    }

    header
        .child(
            gpui::div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .font_family(theme.font_mono.clone())
                .text_size(px(12.0))
                .text_color(theme.text_dim)
                .child(crate::text::chrome_text(
                    SharedString::from(file.path.clone()),
                    None,
                )),
        )
        .child(
            gpui::div()
                .flex_none()
                .font_family(theme.font_mono.clone())
                .text_size(px(11.0))
                .text_color(theme.diff_add)
                .child(crate::text::chrome_text(
                    SharedString::from(format!("+{}", file.additions)),
                    None,
                )),
        )
        .child(
            gpui::div()
                .flex_none()
                .font_family(theme.font_mono.clone())
                .text_size(px(11.0))
                .text_color(theme.diff_del)
                // U+2212 MINUS SIGN, not a hyphen: it matches the plus sign's
                // width and vertical position in a monospace face.
                .child(crate::text::chrome_text(
                    SharedString::from(format!("−{}", file.deletions)),
                    None,
                )),
        )
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn diff_line_row(
    line: &DiffLine,
    highlight_spans: &[HighlightSpan],
    show_word_diff: bool,
    gutter_px: f32,
    ix: usize,
    ctx: &RowContext,
    theme: &Theme,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    let m = &theme.metrics;
    if line.kind == LineKind::Meta {
        return gpui::div()
            .h(px(m.diff_line_height))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .pl(px(m.diff_accent_bar_width
                + 2.0 * gutter_px
                + m.diff_marker_width
                + 12.0))
            .text_size(px(10.5))
            .text_color(theme.text_faint)
            .italic()
            .child(crate::text::chrome_text(
                SharedString::from(line.text.clone()),
                None,
            ))
            .into_any_element();
    }

    // Row tints: 5.5% washes over the pane tone, sampled from Comet.
    let mut add_bg = theme.diff_add;
    add_bg.a = 0.055;
    let mut del_bg = theme.diff_del;
    del_bg.a = 0.055;

    let (marker, marker_color, row_bg, accent, number_color) = match line.kind {
        LineKind::Add => (
            "+",
            theme.diff_add,
            Some(add_bg),
            Some(opacity(theme.diff_add, 0.55)),
            opacity(theme.diff_add, 0.9),
        ),
        LineKind::Del => (
            "−",
            theme.diff_del,
            Some(del_bg),
            Some(opacity(theme.diff_del, 0.55)),
            opacity(theme.diff_del, 0.9),
        ),
        _ => (
            "·",
            opacity(theme.text_faint, 0.5),
            None,
            None,
            opacity(theme.text_faint, 0.8),
        ),
    };

    let gutter = |no: Option<u32>, color: Hsla| {
        gpui::div()
            .w(px(gutter_px))
            .flex_none()
            .font_family(theme.font_mono.clone())
            .text_size(px(11.0))
            .text_color(color)
            .flex()
            .justify_end()
            .pr(px(8.0))
            .child(crate::text::chrome_text(
                SharedString::from(no.map(|n| n.to_string()).unwrap_or_default()),
                None,
            ))
    };

    let mono: Font = gpui::font(theme.font_mono.clone());
    let spans: Vec<(std::ops::Range<usize>, Hsla)> = highlight_spans
        .iter()
        .map(|span| (span.range.clone(), theme.syntax.color(span.kind)))
        .collect();
    let runs = runs_for_spans(&line.text, &spans, &mono, opacity(theme.text, 0.92));

    // Word-level wash: a rounded quad under only the tokens that changed. This
    // is what makes a one-character edit visible at a glance.
    let word_wash: Option<Box<dyn Fn(&gpui::TextLayout, &mut gpui::Window)>> =
        if show_word_diff && !line.word_ranges.is_empty() {
            let ranges = line.word_ranges.clone();
            let mut tint = match line.kind {
                LineKind::Add => theme.diff_add,
                _ => theme.diff_del,
            };
            tint.a = 0.28;
            Some(Box::new(move |layout, window| {
                for range in &ranges {
                    for rect in range_rects(layout, range, 1.0, 1.5) {
                        window.paint_quad(gpui::quad(
                            rect,
                            px(3.0),
                            tint,
                            px(0.0),
                            gpui::transparent_black(),
                            BorderStyle::default(),
                        ));
                    }
                }
            }))
        } else {
            None
        };

    let mut row = gpui::div()
        .id(SharedString::from(format!("__gpuix_diff_line_{ix}")))
        .h(px(m.diff_line_height))
        .w_full()
        .flex_none()
        .flex()
        .flex_row()
        .items_center();

    if let Some(bg) = row_bg {
        row = row.bg(bg);
    }

    if ctx.wants_line_click {
        let callback = ctx.callback.clone();
        let element_id = ctx.element_id;
        let text = line.text.clone();
        let old_no = line.old_no;
        let new_no = line.new_no;
        row = row.on_click(move |_, _window, _cx| {
            emit_event_full(&callback, element_id, "lineClick", |p| {
                p.value = Some(text.clone());
                p.old_line = old_no.map(f64::from);
                p.new_line = new_no.map(f64::from);
            });
        });
    }

    row
        // Accent bar: solid on +/− rows, an invisible spacer on context rows so
        // the columns always line up.
        .child({
            let mut bar = gpui::div()
                .w(px(m.diff_accent_bar_width))
                .h_full()
                .flex_none();
            if let Some(color) = accent {
                bar = bar.bg(color);
            }
            bar
        })
        .child(gutter(
            line.old_no,
            if line.kind == LineKind::Del {
                number_color
            } else {
                opacity(theme.text_faint, 0.8)
            },
        ))
        .child(gutter(
            line.new_no,
            if line.kind == LineKind::Add {
                number_color
            } else {
                opacity(theme.text_faint, 0.8)
            },
        ))
        .child(
            gpui::div()
                .w(px(m.diff_marker_width))
                .flex_none()
                .flex()
                .justify_center()
                .text_size(px(m.diff_text_size))
                .text_color(marker_color)
                .font_family(theme.font_mono.clone())
                .child(crate::text::chrome_text(SharedString::from(marker), None)),
        )
        .child(
            gpui::div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .pl(px(12.0))
                .font_family(theme.font_mono.clone())
                .text_size(px(m.diff_text_size))
                .whitespace_nowrap()
                .child(ctx.text(ix, line.text.clone(), Some(runs), word_wash)),
        )
        .into_any_element()
}

fn opacity(mut color: Hsla, alpha: f32) -> Hsla {
    color.a *= alpha;
    color
}

/// Translucent white on dark, translucent black on light.
fn ink(theme: &Theme, alpha: f32) -> Hsla {
    let lightness = if theme.bg.l < 0.5 { 1.0 } else { 0.0 };
    gpui::hsla(0.0, 0.0, lightness, alpha)
}
