//! What an element inherits from its ancestors, and how a style resolves
//! against it.
//!
//! CSS resolves a property in two steps. A declaration on the element wins. If
//! there is none, an inherited property takes the parent's computed value. The
//! walk in `renderer.rs` calls `descend` on the way down and `resolve` at each
//! node, and learns nothing about either step.
//!
//! # Why this is an `Arc`
//!
//! Once a property inherits, an element's resolved style stops depending only
//! on that element. It depends on every ancestor as well. So the resolved-style
//! cache has to know which inherited context produced it, and it has to answer
//! "is that context still current" once per element per frame.
//!
//! Comparing a dozen fields that many times is the wrong price. Instead
//! `descend` returns the parent's own `Arc` when an element declares nothing
//! inheritable, which is the common case. Checking the cache is then a pointer
//! comparison, and an element only re-resolves when an ancestor actually
//! changed something it inherits.

use std::sync::Arc;

use gpuix_css::color::Rgba;

use crate::style::StyleDesc;

/// The computed value of every inherited property at one point in the tree.
#[derive(Debug, Clone, PartialEq)]
struct Values {
    /// False once an ancestor sets `userSelect: "none"`.
    selectable: bool,
    /// True once an ancestor sets a `cursor` other than `auto`. CSS inherits
    /// the cursor, and GPUI keeps a parent's cursor over a child that sets
    /// none, so this only has to tell selectable text not to show its I-beam.
    cursor_declared: bool,
    /// Selection wash colour for this subtree.
    selection_wash: Rgba,
    /// The computed `color` here, which is what `currentColor` names.
    ///
    /// GPUI inherits text colour itself through the window text style stack, so
    /// this is a second copy of the same value. It exists because `currentColor`
    /// has to resolve while the style resolves, and the GPUI stack only exists
    /// during paint. The root starts at `black`, which is what
    /// `TextStyle::default` uses, so the two agree unless something declares a
    /// colour this does not follow.
    color: Rgba,
    /// Whether the window is in the dark appearance, which `light-dark()`
    /// reads.
    ///
    /// It is a window-wide fact rather than something an element declares, but
    /// it sits here because it is an input to resolving a colour, and this is
    /// what the resolved-style cache already keys on.
    dark: bool,
    /// The root font size in pixels, which is what `rem` is a multiple of.
    ///
    /// It comes from the window rather than a constant here, so a call to
    /// `set_rem_size` reaches every `rem` length. It sits in the cascade
    /// because that is what the resolved-style cache already keys on, so a
    /// change to it invalidates exactly the styles that read a `rem`.
    rem_size: f32,
    /// Every custom property in scope, nearest declaration winning.
    ///
    /// Held behind its own `Arc` so an element that declares no variables
    /// shares its parent's map instead of copying it. Most elements declare
    /// none, so most of the tree shares one allocation.
    variables: Arc<Variables>,
}

/// Custom properties in scope, sorted by name.
///
/// A sorted list beats a map here. There are rarely more than a handful, a
/// linear scan over contiguous memory beats hashing at that size, and it
/// compares cheaply, which the cascade needs on every descend.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Variables(Vec<(String, String)>);

impl Variables {
    /// The declared text of `name`, which includes the leading dashes.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .binary_search_by(|(known, _)| known.as_str().cmp(name))
            .ok()
            .map(|index| self.0[index].1.as_str())
    }

    /// This scope with `declared` layered over it, nearest declaration winning.
    pub fn layer(&self, declared: &[(String, String)]) -> Self {
        let mut next = self.0.clone();
        for (name, value) in declared {
            match next.binary_search_by(|(known, _)| known.as_str().cmp(name)) {
                Ok(index) => next[index].1 = value.clone(),
                Err(index) => next.insert(index, (name.clone(), value.clone())),
            }
        }
        Self(next)
    }
}

/// The inherited context for one element.
///
/// Cloning is a refcount bump. Two cascades that compare equal by pointer are
/// the same context, which is what the resolved-style cache tests.
#[derive(Debug, Clone)]
pub(crate) struct Inherited(Arc<Values>);

impl Inherited {
    /// The context at the root of the tree, before any element declares
    /// anything.
    ///
    /// Takes plain values rather than a `Theme` so that nothing about
    /// inheritance depends on the renderer. The caller reads the theme.
    pub fn root(accent: Rgba, dark: bool, rem_size: f32) -> Self {
        let wash = Rgba { a: 0.35, ..accent };
        Self(Arc::new(Values {
            selectable: true,
            cursor_declared: false,
            selection_wash: wash,
            color: Rgba::BLACK,
            dark,
            rem_size,
            variables: Arc::new(Variables::default()),
        }))
    }

    /// The context for the children of an element carrying `style`.
    ///
    /// Returns this same context, by pointer, when the element declares nothing
    /// that inherits. That is what keeps the cache check cheap, so the
    /// comparison against the old value is load-bearing rather than an
    /// optimisation.
    pub fn descend(&self, style: Option<&StyleDesc>) -> Self {
        let Some(style) = style else {
            return self.clone();
        };
        let mut next = (*self.0).clone();

        match style.user_select.as_deref() {
            Some("none") => next.selectable = false,
            Some("text") | Some("auto") => next.selectable = true,
            _ => {}
        }
        match style.cursor.as_deref() {
            Some("auto") => next.cursor_declared = false,
            Some(_) => next.cursor_declared = true,
            None => {}
        }
        if let Some(text) = style.selection_color.as_deref() {
            let context = gpuix_css::color::ColorContext {
                current_color: next.color,
                dark: next.dark,
            };
            if let Ok(color) = gpuix_css::color::color(text, &context) {
                next.selection_wash = color;
            }
        }

        let declared = crate::style::declared_variables(style);
        if !declared.is_empty() {
            let layered = next.variables.layer(&declared);
            if layered != *next.variables {
                next.variables = Arc::new(layered);
            }
        }

        if let Some(text) = style.color.as_deref() {
            // Variables layer first, so `color: var(--fg)` computes the same
            // colour here that it paints in the style itself. `currentColor` on
            // `color` means the inherited value, so it declares nothing.
            let scope = crate::style::vars::Scope::new(
                &next.variables,
                next.color,
                next.dark,
                next.rem_size,
            );
            if let Some(color) = scope.color(text) {
                next.color = color;
            }
        }

        if next == *self.0 {
            return self.clone();
        }
        Self(Arc::new(next))
    }

    /// Whether two contexts are the same one.
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Whether this subtree takes part in text selection.
    pub fn selectable(&self) -> bool {
        self.0.selectable
    }

    /// Whether an ancestor set a `cursor` that this subtree inherits.
    pub fn cursor_declared(&self) -> bool {
        self.0.cursor_declared
    }

    /// The selection wash colour for this subtree.
    pub fn selection_wash(&self) -> Rgba {
        self.0.selection_wash
    }

    /// Every custom property in scope.
    ///
    /// The render path reads these through `scope()`. This is the read side
    /// the module's own tests go through, so it compiles under `cfg(test)`.
    #[cfg(test)]
    pub fn variables(&self) -> &Arc<Variables> {
        &self.0.variables
    }

    /// The computed `color` here.
    ///
    /// Same as `variables`: the render path reads it through `scope()`.
    #[cfg(test)]
    pub fn color(&self) -> Rgba {
        self.0.color
    }

    /// A scope for resolving one style against this context.
    pub fn scope(&self) -> crate::style::vars::Scope<'_> {
        crate::style::vars::Scope::new(
            &self.0.variables,
            self.0.color,
            self.0.dark,
            self.0.rem_size,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> Inherited {
        Inherited::root(Rgba { r: 0.4, g: 0.4, b: 0.9, a: 1.0 }, true, 16.0)
    }

    fn styled(build: impl FnOnce(&mut StyleDesc)) -> StyleDesc {
        let mut style = StyleDesc::default();
        build(&mut style);
        style
    }

    #[test]
    fn a_declared_cursor_reaches_the_subtree_until_auto_resets_it() {
        let root = Inherited::root(Rgba::BLACK, false, 16.0);
        assert!(!root.cursor_declared());
        let link = root.descend(Some(&styled(|s| s.cursor = Some("pointer".into()))));
        assert!(link.cursor_declared());
        assert!(link.descend(None).cursor_declared());
        let reset = link.descend(Some(&styled(|s| s.cursor = Some("auto".into()))));
        assert!(!reset.cursor_declared());
    }

    #[test]
    fn an_element_with_no_style_keeps_its_parent_context() {
        let parent = root();
        assert!(parent.same(&parent.descend(None)));
    }

    #[test]
    fn an_element_that_inherits_nothing_keeps_its_parent_context() {
        // The pointer has to stay the same here. A new Arc for every styled
        // element would make every element re-resolve on every frame.
        let parent = root();
        let style = styled(|s| s.padding = Some(8.0.into()));
        assert!(parent.same(&parent.descend(Some(&style))));
    }

    #[test]
    fn a_declaration_that_inherits_makes_a_new_context() {
        let parent = root();
        let style = styled(|s| s.user_select = Some("none".to_string()));
        let child = parent.descend(Some(&style));
        assert!(!parent.same(&child));
        assert!(parent.selectable());
        assert!(!child.selectable());
    }

    #[test]
    fn redeclaring_the_value_it_already_has_keeps_the_context() {
        // `userSelect: "text"` at the root is what the root already computes,
        // so it must not invalidate the subtree below it.
        let parent = root();
        let style = styled(|s| s.user_select = Some("text".to_string()));
        assert!(parent.same(&parent.descend(Some(&style))));
    }

    #[test]
    fn an_inherited_value_reaches_a_grandchild() {
        let parent = root();
        let style = styled(|s| s.user_select = Some("none".to_string()));
        let child = parent.descend(Some(&style));
        let grandchild = child.descend(None);
        assert!(!grandchild.selectable());
        assert!(child.same(&grandchild));
    }

    #[test]
    fn a_descendant_can_turn_selection_back_on() {
        let parent = root();
        let off = styled(|s| s.user_select = Some("none".to_string()));
        let on = styled(|s| s.user_select = Some("text".to_string()));
        let child = parent.descend(Some(&off));
        let grandchild = child.descend(Some(&on));
        assert!(grandchild.selectable());
    }

    fn declaring(pairs: &[(&str, &str)]) -> StyleDesc {
        let custom = pairs
            .iter()
            .map(|(name, value)| (name.to_string(), serde_json::json!(value)))
            .collect();
        StyleDesc {
            custom,
            ..Default::default()
        }
    }

    #[test]
    fn a_declared_variable_is_in_scope_below_it() {
        let parent = root();
        let child = parent.descend(Some(&declaring(&[("--brand", "#ff0000")])));
        assert_eq!(child.variables().get("--brand"), Some("#ff0000"));
        assert_eq!(parent.variables().get("--brand"), None);
    }

    #[test]
    fn a_variable_reaches_a_grandchild() {
        let parent = root();
        let child = parent.descend(Some(&declaring(&[("--brand", "#ff0000")])));
        let grandchild = child.descend(None);
        assert_eq!(grandchild.variables().get("--brand"), Some("#ff0000"));
    }

    #[test]
    fn a_nearer_declaration_wins() {
        let parent = root().descend(Some(&declaring(&[("--brand", "#ff0000")])));
        let child = parent.descend(Some(&declaring(&[("--brand", "#00ff00")])));
        assert_eq!(child.variables().get("--brand"), Some("#00ff00"));
        assert_eq!(parent.variables().get("--brand"), Some("#ff0000"));
    }

    #[test]
    fn a_second_declaration_leaves_the_first_alone() {
        let scope = root()
            .descend(Some(&declaring(&[("--a", "1px")])))
            .descend(Some(&declaring(&[("--b", "2px")])));
        assert_eq!(scope.variables().get("--a"), Some("1px"));
        assert_eq!(scope.variables().get("--b"), Some("2px"));
    }

    #[test]
    fn a_number_declares_the_same_thing_as_its_text() {
        let mut style = StyleDesc::default();
        style
            .custom
            .insert("--pad".to_string(), serde_json::json!(8));
        let scope = root().descend(Some(&style));
        assert_eq!(scope.variables().get("--pad"), Some("8"));
    }

    #[test]
    fn a_key_without_the_two_dashes_is_not_a_variable() {
        // Serde's flatten collects every unknown key, so a typo or a field a
        // newer client knows about lands in the same map. Only `--` names are
        // declarations.
        let mut style = StyleDesc::default();
        style
            .custom
            .insert("someFutureThing".to_string(), serde_json::json!("4"));
        let parent = root();
        assert!(parent.same(&parent.descend(Some(&style))));
    }

    #[test]
    fn redeclaring_a_variable_with_its_own_value_keeps_the_context() {
        // The cache below this element compares cascade pointers, so an
        // unchanged declaration must not build a new one.
        let parent = root().descend(Some(&declaring(&[("--brand", "#ff0000")])));
        let same = parent.descend(Some(&declaring(&[("--brand", "#ff0000")])));
        assert!(parent.same(&same));
    }

    #[test]
    fn an_undefined_value_declares_nothing() {
        // `undefined` in the style prop arrives as null. CSS has no way to
        // write an undeclared value, so it has to read as absent.
        let mut style = StyleDesc::default();
        style
            .custom
            .insert("--brand".to_string(), serde_json::Value::Null);
        let parent = root();
        assert!(parent.same(&parent.descend(Some(&style))));
    }

    fn colored(color: &str) -> StyleDesc {
        styled(|s| s.color = Some(color.to_string()))
    }

    #[test]
    fn a_declared_colour_becomes_the_current_colour() {
        let parent = root();
        assert_eq!(parent.color(), Rgba::BLACK);
        let child = parent.descend(Some(&colored("#ff0000")));
        assert_eq!(
            Some(child.color()),
            gpuix_css::color::color("#ff0000", &Default::default()).ok()
        );
    }

    #[test]
    fn the_current_colour_reaches_a_grandchild() {
        let child = root().descend(Some(&colored("#ff0000")));
        assert_eq!(child.descend(None).color(), child.color());
    }

    #[test]
    fn a_colour_written_as_a_variable_still_becomes_the_current_colour() {
        let scope = root().descend(Some(&declaring(&[("--fg", "#ff0000")])));
        let child = scope.descend(Some(&colored("var(--fg)")));
        assert_eq!(
            Some(child.color()),
            gpuix_css::color::color("#ff0000", &Default::default()).ok()
        );
    }

    #[test]
    fn current_color_on_color_itself_declares_nothing() {
        // CSS computes `color: currentColor` to `inherit`, so the element keeps
        // what it already had and the context below it does not change.
        let parent = root().descend(Some(&colored("#ff0000")));
        assert!(parent.same(&parent.descend(Some(&colored("currentColor")))));
    }

    #[test]
    fn an_unparseable_colour_leaves_the_current_colour_alone() {
        let parent = root().descend(Some(&colored("#ff0000")));
        let child = parent.descend(Some(&colored("not-a-colour")));
        assert_eq!(child.color(), parent.color());
    }
}
