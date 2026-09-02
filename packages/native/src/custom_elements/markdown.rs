//! `<markdown>` — GitHub-flavoured markdown rendered natively and selectable.
//!
//! ```tsx
//! <markdown source={text} theme={{ accent: '#7c86ff' }} onLinkClick={(e) => {}} />
//! ```
//!
//! Every paragraph, heading, table cell and code line registers into the shared
//! selection registry in document order, so a drag can start in a heading and
//! end inside a fenced code block, and Cmd+C copies the whole span.

use std::rc::Rc;
use std::sync::Arc;

use gpui::SharedString;

use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use crate::markdown::parser::{parse, BlockTree};
use crate::markdown::render::{render_tree, MdContext};
use crate::renderer::emit_event_full;
use crate::theme::Theme;

pub struct MarkdownFactory;

impl CustomElementFactory for MarkdownFactory {
    fn element_type(&self) -> &str {
        "markdown"
    }

    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(MarkdownElement::default())
    }
}

#[derive(Default)]
pub struct MarkdownElement {
    source: String,
    theme: Theme,
    /// Parsed tree for the current source. `Rc` so a frame clones a pointer
    /// rather than every block, string and inline run in the document.
    tree: Option<Rc<BlockTree>>,
    parsed_len: Option<usize>,
    parsed_hash: Option<u64>,
}

impl MarkdownElement {
    fn tree(&mut self) -> Rc<BlockTree> {
        let hash = hash64(&self.source);
        let stale = self.parsed_hash != Some(hash) || self.parsed_len != Some(self.source.len());
        if stale || self.tree.is_none() {
            self.tree = Some(Rc::new(parse(&self.source)));
            self.parsed_hash = Some(hash);
            self.parsed_len = Some(self.source.len());
        }
        self.tree.clone().expect("just parsed")
    }
}

fn hash64(source: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

impl CustomElement for MarkdownElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut gpui::Window,
        _cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        let theme = self.theme.clone();
        let tree = self.tree();

        // Link clicks are hit-tested per byte range inside the painted text, so
        // clicking prose emits nothing and clicking the second link emits the
        // second URL.
        let on_link: Option<Arc<dyn Fn(&str)>> = if ctx.events.contains("linkClick") {
            let callback = ctx.event_callback.clone();
            let element_id = ctx.id;
            Some(Arc::new(move |url: &str| {
                let url = url.to_string();
                emit_event_full(&callback, element_id, "linkClick", |p| {
                    p.value = Some(url);
                });
            }))
        } else {
            None
        };

        let mut md = MdContext::new(
            ctx.id,
            ctx.selection.clone(),
            ctx.selectable,
            ctx.selection_wash,
            theme.clone(),
            on_link,
            ctx.highlight_set.clone(),
        );
        let body = render_tree(&tree, &mut md, window);

        let container = gpui::div()
            .id(SharedString::from(format!("__gpuix_markdown_{}", ctx.id)))
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .text_color(theme.text)
            .font_family(theme.font_sans.clone())
            .text_size(gpui::px(theme.metrics.md_text_size))
            .line_height(gpui::px(theme.metrics.md_line_height));

        super::custom_surface(container, &ctx)
            .child(body)
            .into_any_element()
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "source" => self.source = value.as_str().unwrap_or("").to_string(),
            "theme" => self.theme = Theme::from_prop(Some(&value)),
            _ => {}
        }
    }

    fn supported_props(&self) -> &'static [&'static str] {
        &["source", "theme"]
    }

    fn supported_events(&self) -> &'static [&'static str] {
        &["linkClick", "click", "mouseEnter", "mouseLeave"]
    }

    fn destroy(&mut self, _window: Option<&mut gpui::Window>) {
        self.tree = None;
    }
}
