pub(crate) mod corners;
pub(crate) mod resolve;
pub(crate) mod vars;

/// A style value that resolves to a number.
///
/// A bare number is the common case and stays a number. Text goes through
/// `var()` first, then reads as a plain number or a `px` length, which are the
/// two forms the `style` prop already takes.
///
/// Every numeric field uses this, including the unitless ones such as `opacity`
/// and `flexGrow`, because `var()` is legal in any property and a field that
/// stayed `f64` would reject it.
///
/// `Deserialize` is hand written for the same reason as `StyleDesc`. An
/// `#[serde(untagged)]` enum buffers every value it reads before it picks a
/// variant, and 36 fields use this one.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Numeric {
    Number(f64),
    Text(String),
}

impl<'de> Deserialize<'de> for Numeric {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct NumericVisitor;

        impl Visitor<'_> for NumericVisitor {
            type Value = Numeric;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a number or a CSS value such as \"8px\" or \"var(--pad)\"")
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Numeric, E> {
                Ok(Numeric::Number(value))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Numeric, E> {
                Ok(Numeric::Number(value as f64))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Numeric, E> {
                Ok(Numeric::Number(value as f64))
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Numeric, E> {
                Ok(Numeric::Text(value.to_owned()))
            }

            fn visit_string<E: de::Error>(self, value: String) -> Result<Numeric, E> {
                Ok(Numeric::Text(value))
            }
        }

        deserializer.deserialize_any(NumericVisitor)
    }
}

impl Numeric {
    /// The number this holds, without resolving anything.
    ///
    /// Text always reads as `None` here, even plain `"8px"`. Only
    /// `Scope::number` reads text, so a caller that skips the scope cannot
    /// half-resolve a value.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Text(_) => None,
        }
    }
}

impl From<f64> for Numeric {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

use serde::{Deserialize, Deserializer, Serialize};

/// Font weight value — accepts both CSS strings ("bold", "700") and numbers (700).
/// JS style objects commonly use both `fontWeight: "bold"` and `fontWeight: 700`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum FontWeightValue {
    Num(f64),
    Str(String),
}

impl<'de> Deserialize<'de> for FontWeightValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct WeightVisitor;

        impl Visitor<'_> for WeightVisitor {
            type Value = FontWeightValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a number or a weight name such as \"bold\"")
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<FontWeightValue, E> {
                Ok(FontWeightValue::Num(value))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<FontWeightValue, E> {
                Ok(FontWeightValue::Num(value as f64))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<FontWeightValue, E> {
                Ok(FontWeightValue::Num(value as f64))
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<FontWeightValue, E> {
                Ok(FontWeightValue::Str(value.to_owned()))
            }

            fn visit_string<E: de::Error>(self, value: String) -> Result<FontWeightValue, E> {
                Ok(FontWeightValue::Str(value))
            }
        }

        deserializer.deserialize_any(WeightVisitor)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxShadowValue {
    pub offset_x: f64,
    pub offset_y: f64,
    pub blur_radius: f64,
    pub spread_radius: f64,
    pub color: String,
}

/// A `background` value.
///
/// The `style` prop sends CSS text. It also takes the object form
/// `{ "type": "linear-gradient", "angle": 90, "stops": [...] }`. That form is
/// the only way to ask for a colour space, because lightningcss does not read
/// `in oklab` inside `linear-gradient()`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum BackgroundValue {
    Text(String),
    Gradient(LinearGradientValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GradientKind {
    #[serde(rename = "linear-gradient")]
    LinearGradient,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinearGradientValue {
    #[serde(rename = "type")]
    pub kind: GradientKind,
    /// Degrees clockwise from `to top`.
    pub angle: f64,
    pub stops: Vec<LinearGradientStopValue>,
    /// `srgb` or `oklab`. Unset means `srgb`.
    #[serde(rename = "colorSpace", default, skip_serializing_if = "Option::is_none")]
    pub color_space: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinearGradientStopValue {
    pub color: String,
    /// Where on the gradient line, from 0 to 1.
    pub position: f64,
}

impl<'de> Deserialize<'de> for BackgroundValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};

        struct BackgroundVisitor;

        impl<'de> Visitor<'de> for BackgroundVisitor {
            type Value = BackgroundValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("CSS background text or a linear-gradient object")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<BackgroundValue, E> {
                Ok(BackgroundValue::Text(value.to_owned()))
            }

            fn visit_string<E: de::Error>(self, value: String) -> Result<BackgroundValue, E> {
                Ok(BackgroundValue::Text(value))
            }

            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<BackgroundValue, M::Error> {
                LinearGradientValue::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(BackgroundValue::Gradient)
            }
        }

        deserializer.deserialize_any(BackgroundVisitor)
    }
}

impl BackgroundValue {
    /// Whether the value paints anything, read without variables or the
    /// window. Text that does not read counts as painted, because a blocked
    /// click is the smaller mistake.
    fn paints(&self) -> bool {
        match self {
            BackgroundValue::Text(text) => text_paints(text),
            BackgroundValue::Gradient(gradient) => gradient.stops.iter().any(|stop| {
                gpuix_css::color::read(&stop.color, &gpuix_css::color::ColorContext::default())
                    .map_or(true, |reading| reading.color.a > 0.0)
            }),
        }
    }
}

fn text_paints(text: &str) -> bool {
    use gpuix_css::background::Fill;
    match gpuix_css::background::read(text, &gpuix_css::color::ColorContext::default()) {
        Ok(Some(reading)) => match reading.fill {
            Fill::Color(color) => color.a > 0.0,
            Fill::LinearGradient(gradient) => gradient.stops.iter().any(|stop| stop.color.a > 0.0),
        },
        Ok(None) => false,
        Err(_) => true,
    }
}

/// What a sizing property resolves to.
///
/// `width` and its family take `auto` and resolve a percentage against the
/// parent, which the other length properties do not, so they have their own
/// resolved type. `Scope::dimension` is the only thing that builds one.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum DimensionValue {
    Pixels(f64),
    /// A share of the parent, where `1.0` is the whole of it.
    Percentage(f64),
    #[default]
    Auto,
}

/// Declares `StyleDesc` and its `Deserialize` from one field list.
///
/// The wire name beside each field drives both directions, so what JS writes
/// and what Rust reads cannot drift apart.
///
/// The `Deserialize` is hand written to keep `#[serde(flatten)]` off the read
/// path. Flatten makes serde buffer the whole object into an intermediate value
/// before it reads one field, and every `setStyle` call pays for that. Measured
/// on a small style, buffering was 246 ns of a 320 ns parse.
macro_rules! style_desc {
    ($( $(#[$meta:meta])* $field:ident : $ty:ty = $name:literal ),* $(,)?) => {
        /// Style description that can be serialized from JS
        /// Note: This is only used for JSON deserialization, not direct napi binding
        #[derive(Debug, Clone, Default, PartialEq, Serialize)]
        pub struct StyleDesc {
            $(
                $(#[$meta])*
                #[serde(rename = $name)]
                pub $field: $ty,
            )*

            /// Custom property declarations on this element, such as `--pad: 8px`.
            ///
            /// Only the keys starting with `--` land here. Anything else is a
            /// typo or a field a newer client knows about, and both are
            /// ignored, which is what happened to them before this field
            /// existed.
            #[serde(flatten)]
            pub custom: std::collections::HashMap<String, serde_json::Value>,
        }

        /// Every wire name the reader knows, in declaration order.
        ///
        /// One test reads this against what `Serialize` writes, which proves
        /// the two halves of the macro agree.
        #[cfg(test)]
        const FIELDS: &[&str] = &[$( $name, )*];

        impl StyleDesc {
            /// Reads a style from JSON straight into a box.
            ///
            /// `StyleDesc` is over 1,700 bytes and every element in the tree
            /// holds one, so the tree keeps a pointer rather than the struct.
            /// Reading into the box means the value is never built on the stack
            /// and then copied there.
            pub fn from_json_boxed(text: &str) -> serde_json::Result<Box<StyleDesc>> {
                let mut json = serde_json::Deserializer::from_str(text);
                let style = Self::deserialize_boxed(&mut json)?;
                json.end()?;
                Ok(style)
            }

            /// The same read, from any deserializer.
            ///
            /// The batch path already holds a `serde_json::Value`, so it needs
            /// this rather than the text above.
            pub fn deserialize_boxed<'de, D>(deserializer: D) -> Result<Box<StyleDesc>, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer.deserialize_map(read::Boxed)
            }
        }

        impl<'de> Deserialize<'de> for StyleDesc {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer.deserialize_map(read::Owned)
            }
        }

        /// The reader for `StyleDesc`.
        ///
        /// Two visitors over one `fill`, so a style read into a box is never
        /// built on the stack first, and a nested `hover` still reads through
        /// the ordinary `Deserialize`.
        mod read {
            use super::StyleDesc;
            use serde::de::{Deserializer, Error, IgnoredAny, MapAccess, Visitor};

            /// One key of a style object, matched without allocating.
            ///
            /// Only a custom property keeps its name, because that name is the
            /// map key. Every other key is either a known field, which the
            /// variant already names, or ignored.
            #[allow(non_camel_case_types)]
            enum Key {
                $( $field, )*
                Custom(String),
                Ignore,
            }

            struct KeyVisitor;

            impl Visitor<'_> for KeyVisitor {
                type Value = Key;

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("a style property name")
                }

                fn visit_str<E: Error>(self, value: &str) -> Result<Key, E> {
                    Ok(match value {
                        $( $name => Key::$field, )*
                        name if name.starts_with("--") => Key::Custom(name.to_owned()),
                        _ => Key::Ignore,
                    })
                }
            }

            impl<'de> serde::Deserialize<'de> for Key {
                fn deserialize<D>(deserializer: D) -> Result<Key, D::Error>
                where
                    D: Deserializer<'de>,
                {
                    deserializer.deserialize_str(KeyVisitor)
                }
            }

            /// Writes every key of `map` into `style`.
            ///
            /// A repeated key takes the later value, the way a repeated
            /// declaration does in a CSS rule.
            fn fill<'de, M>(style: &mut StyleDesc, map: &mut M) -> Result<(), M::Error>
            where
                M: MapAccess<'de>,
            {
                while let Some(key) = map.next_key::<Key>()? {
                    match key {
                        $( Key::$field => style.$field = map.next_value()?, )*
                        Key::Custom(name) => {
                            let value: serde_json::Value = map.next_value()?;
                            style.custom.insert(name, value);
                        }
                        Key::Ignore => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                Ok(())
            }

            pub struct Owned;

            impl<'de> Visitor<'de> for Owned {
                type Value = StyleDesc;

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("a style object")
                }

                fn visit_map<M>(self, mut map: M) -> Result<StyleDesc, M::Error>
                where
                    M: MapAccess<'de>,
                {
                    let mut style = StyleDesc::default();
                    fill(&mut style, &mut map)?;
                    Ok(style)
                }
            }

            pub struct Boxed;

            impl<'de> Visitor<'de> for Boxed {
                type Value = Box<StyleDesc>;

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("a style object")
                }

                fn visit_map<M>(self, mut map: M) -> Result<Box<StyleDesc>, M::Error>
                where
                    M: MapAccess<'de>,
                {
                    let mut style = Box::<StyleDesc>::default();
                    fill(&mut style, &mut map)?;
                    Ok(style)
                }
            }
        }
    };
}

style_desc! {
    // Display
    display: Option<String> = "display",
    visibility: Option<String> = "visibility",

    // Flexbox
    flex_direction: Option<String> = "flexDirection",
    flex_wrap: Option<String> = "flexWrap",
    flex_grow: Option<Numeric> = "flexGrow",
    flex_shrink: Option<Numeric> = "flexShrink",
    flex_basis: Option<Numeric> = "flexBasis",
    align_items: Option<String> = "alignItems",
    align_self: Option<String> = "alignSelf",
    align_content: Option<String> = "alignContent",
    justify_content: Option<String> = "justifyContent",
    gap: Option<Numeric> = "gap",
    row_gap: Option<Numeric> = "rowGap",
    column_gap: Option<Numeric> = "columnGap",
    grid_template_columns: Option<Numeric> = "gridTemplateColumns",
    grid_template_rows: Option<Numeric> = "gridTemplateRows",
    grid_column_min: Option<String> = "gridColumnMin",
    grid_row_min: Option<String> = "gridRowMin",

    // Sizing. These read the same CSS lengths as every other length property,
    // and `auto` and a percentage on top of them.
    width: Option<Numeric> = "width",
    height: Option<Numeric> = "height",
    min_width: Option<Numeric> = "minWidth",
    min_height: Option<Numeric> = "minHeight",
    max_width: Option<Numeric> = "maxWidth",
    max_height: Option<Numeric> = "maxHeight",

    // Spacing (padding)
    padding: Option<Numeric> = "padding",
    padding_top: Option<Numeric> = "paddingTop",
    padding_right: Option<Numeric> = "paddingRight",
    padding_bottom: Option<Numeric> = "paddingBottom",
    padding_left: Option<Numeric> = "paddingLeft",

    // Spacing (margin)
    margin: Option<Numeric> = "margin",
    margin_top: Option<Numeric> = "marginTop",
    margin_right: Option<Numeric> = "marginRight",
    margin_bottom: Option<Numeric> = "marginBottom",
    margin_left: Option<Numeric> = "marginLeft",

    // Position
    position: Option<String> = "position",
    top: Option<Numeric> = "top",
    right: Option<Numeric> = "right",
    bottom: Option<Numeric> = "bottom",
    left: Option<Numeric> = "left",

    // Background & Colors
    background: Option<BackgroundValue> = "background",
    background_color: Option<String> = "backgroundColor",
    background_image: Option<String> = "backgroundImage",
    /// How `backgroundImage` mixes with `backgroundColor`, a `<blend-mode>`.
    background_blend_mode: Option<String> = "backgroundBlendMode",
    /// How the element mixes with what is under it, a `<blend-mode>`.
    mix_blend_mode: Option<String> = "mixBlendMode",
    /// A CSS filter list on the element and its children, or `none`.
    filter: Option<String> = "filter",
    /// A CSS filter list on what is under the element, or `none`.
    backdrop_filter: Option<String> = "backdropFilter",
    /// A gradient whose alpha keeps or drops each pixel, or `none`.
    mask_image: Option<String> = "maskImage",
    color: Option<String> = "color",
    opacity: Option<Numeric> = "opacity",

    // Border
    border_width: Option<Numeric> = "borderWidth",
    border_top_width: Option<Numeric> = "borderTopWidth",
    border_right_width: Option<Numeric> = "borderRightWidth",
    border_bottom_width: Option<Numeric> = "borderBottomWidth",
    border_left_width: Option<Numeric> = "borderLeftWidth",
    border_color: Option<String> = "borderColor",
    border_radius: Option<Numeric> = "borderRadius",
    border_top_left_radius: Option<Numeric> = "borderTopLeftRadius",
    border_top_right_radius: Option<Numeric> = "borderTopRightRadius",
    border_bottom_left_radius: Option<Numeric> = "borderBottomLeftRadius",
    border_bottom_right_radius: Option<Numeric> = "borderBottomRightRadius",
    border_start_start_radius: Option<Numeric> = "borderStartStartRadius",
    border_start_end_radius: Option<Numeric> = "borderStartEndRadius",
    border_end_start_radius: Option<Numeric> = "borderEndStartRadius",
    border_end_end_radius: Option<Numeric> = "borderEndEndRadius",

    // Corner shape (CSS Borders 4, section 3.9). `corner*` shorthands take a
    // radius list and a shape list in either order; `*Shape` takes shapes only.
    corner_shape: Option<String> = "cornerShape",
    corner_top_left_shape: Option<String> = "cornerTopLeftShape",
    corner_top_right_shape: Option<String> = "cornerTopRightShape",
    corner_bottom_right_shape: Option<String> = "cornerBottomRightShape",
    corner_bottom_left_shape: Option<String> = "cornerBottomLeftShape",
    corner_start_start_shape: Option<String> = "cornerStartStartShape",
    corner_start_end_shape: Option<String> = "cornerStartEndShape",
    corner_end_start_shape: Option<String> = "cornerEndStartShape",
    corner_end_end_shape: Option<String> = "cornerEndEndShape",
    corner_top_shape: Option<String> = "cornerTopShape",
    corner_right_shape: Option<String> = "cornerRightShape",
    corner_bottom_shape: Option<String> = "cornerBottomShape",
    corner_left_shape: Option<String> = "cornerLeftShape",
    corner_block_start_shape: Option<String> = "cornerBlockStartShape",
    corner_block_end_shape: Option<String> = "cornerBlockEndShape",
    corner_inline_start_shape: Option<String> = "cornerInlineStartShape",
    corner_inline_end_shape: Option<String> = "cornerInlineEndShape",
    corner: Option<String> = "corner",
    corner_top_left: Option<String> = "cornerTopLeft",
    corner_top_right: Option<String> = "cornerTopRight",
    corner_bottom_right: Option<String> = "cornerBottomRight",
    corner_bottom_left: Option<String> = "cornerBottomLeft",
    corner_start_start: Option<String> = "cornerStartStart",
    corner_start_end: Option<String> = "cornerStartEnd",
    corner_end_start: Option<String> = "cornerEndStart",
    corner_end_end: Option<String> = "cornerEndEnd",
    corner_top: Option<String> = "cornerTop",
    corner_right: Option<String> = "cornerRight",
    corner_bottom: Option<String> = "cornerBottom",
    corner_left: Option<String> = "cornerLeft",
    corner_block_start: Option<String> = "cornerBlockStart",
    corner_block_end: Option<String> = "cornerBlockEnd",
    corner_inline_start: Option<String> = "cornerInlineStart",
    corner_inline_end: Option<String> = "cornerInlineEnd",
    box_shadow: Option<BoxShadowValue> = "boxShadow",

    // Text
    font_size: Option<Numeric> = "fontSize",
    font_family: Option<String> = "fontFamily",
    font_weight: Option<FontWeightValue> = "fontWeight",
    text_align: Option<String> = "textAlign",
    line_height: Option<Numeric> = "lineHeight",
    white_space: Option<String> = "whiteSpace",
    text_overflow: Option<String> = "textOverflow",
    line_clamp: Option<Numeric> = "lineClamp",

    // Overflow
    overflow: Option<String> = "overflow",
    overflow_x: Option<String> = "overflowX",
    overflow_y: Option<String> = "overflowY",
    /// `auto`, `contain` or `none`, one for both axes or one per axis.
    overscroll_behavior: Option<String> = "overscrollBehavior",
    overscroll_behavior_x: Option<String> = "overscrollBehaviorX",
    overscroll_behavior_y: Option<String> = "overscrollBehaviorY",
    /// `auto`, `thin` or `none`.
    scrollbar_width: Option<String> = "scrollbarWidth",
    /// `auto`, or a thumb colour and a track colour.
    scrollbar_color: Option<String> = "scrollbarColor",
    /// `auto`, `stable` or `stable both-edges`.
    scrollbar_gutter: Option<String> = "scrollbarGutter",
    /// Space `scrollIntoView` keeps around the element, a number of
    /// pixels or `"Npx"`, alone or as the one-to-four shorthand.
    scroll_margin: Option<Numeric> = "scrollMargin",
    scroll_margin_top: Option<Numeric> = "scrollMarginTop",
    scroll_margin_right: Option<Numeric> = "scrollMarginRight",
    scroll_margin_bottom: Option<Numeric> = "scrollMarginBottom",
    scroll_margin_left: Option<Numeric> = "scrollMarginLeft",
    /// Space `scrollIntoView` keeps inside this scroll box.
    scroll_padding: Option<Numeric> = "scrollPadding",
    scroll_padding_top: Option<Numeric> = "scrollPaddingTop",
    scroll_padding_right: Option<Numeric> = "scrollPaddingRight",
    scroll_padding_bottom: Option<Numeric> = "scrollPaddingBottom",
    scroll_padding_left: Option<Numeric> = "scrollPaddingLeft",

    // View transitions. The name pairs the element that leaves with the
    // element that arrives across one `startViewTransition` call.
    view_transition_name: Option<String> = "viewTransitionName",

    // Cursor
    cursor: Option<String> = "cursor",
    /// `"auto"` blocks mouse hits behind this element. `"none"` never does.
    /// Unset: block when this element paints a fill or is absolutely positioned.
    pointer_events: Option<String> = "pointerEvents",

    // Text selection. "none" opts an element and its subtree out of the
    // selection registry, so buttons and toolbars never start a drag.
    // Inherited down the tree like the CSS property of the same name.
    user_select: Option<String> = "userSelect",
    /// Selection wash colour for this subtree. Defaults to the theme accent at
    /// 35% opacity, the same tone Comet uses.
    selection_color: Option<String> = "selectionColor",

    // Pseudo-selector styles, applied by GPUI natively (no JS round-trip).
    // Uses Box to avoid infinite-size struct (StyleDesc contains StyleDesc).
    //
    // These two named fields are here for history. A CSS `style` attribute
    // holds declarations, not selectors, so the style prop gets no further
    // condition. A class resolver sends every other condition through
    // `selectors` below.
    hover: Option<Box<StyleDesc>> = "hover",
    active: Option<Box<StyleDesc>> = "active",

    // Conditioned blocks from a class resolver. The `style` prop type does
    // not carry this field, because a style attribute cannot hold a selector.
    selectors: Option<Vec<SelectorRule>> = "selectors",
}

pub use crate::color::{parse_color, parse_color_hex};

/// One conditioned block from a class resolver.
///
/// `on` is a canonical selector spelling, and `Selector::parse` names the
/// closed set. An entry with a spelling outside it warns once and drops.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectorRule {
    pub on: String,
    pub style: Box<StyleDesc>,
}

impl StyleDesc {
    /// The state blocks this style declares, in specification order.
    ///
    /// This is the one place that knows the `style` prop spells its states as
    /// named fields. The class channel sends states as parsed selectors in
    /// `selectors`, and those follow the named fields here.
    pub(crate) fn states(
        &self,
    ) -> impl Iterator<Item = (crate::style::resolve::State, &StyleDesc)> {
        use crate::style::resolve::{Selector, State};
        [
            (State::Hover, self.hover.as_deref()),
            (State::Active, self.active.as_deref()),
        ]
        .into_iter()
        .filter_map(|(state, declared)| declared.map(|declared| (state, declared)))
        .chain(self.selectors.iter().flatten().filter_map(|rule| {
            match Selector::parse(&rule.on) {
                Some(Selector::State(state)) => Some((state, rule.style.as_ref())),
                _ => None,
            }
        }))
    }
}

/// Whether this style should insert a mouse hitbox.
///
/// GPUI only hit-tests elements that own a hitbox. A painted overlay without
/// one stays visible while clicks fall through. CSS `pointer-events` maps
/// here: `none` never blocks, `auto` always does. Unset follows the painted
/// surface: a fill or an absolute/fixed box blocks.
///
/// In-flow fills use BlockMouseExceptScroll so a parent scroller still gets
/// the wheel. `occlude()` (BlockMouse) is only for overlays that steal it.
pub fn should_occlude(style: &StyleDesc) -> bool {
    match style.pointer_events.as_deref() {
        Some("none") => return false,
        Some("auto") => return true,
        _ => {}
    }
    if matches!(style.position.as_deref(), Some("absolute") | Some("fixed")) {
        return true;
    }
    let Some(painted) = style
        .background_color
        .as_deref()
        .map(text_paints)
        .or_else(|| style.background.as_ref().map(BackgroundValue::paints))
    else {
        return false;
    };
    painted
}

/// Map a CSS `cursor` keyword onto a GPUI cursor. Unknown keywords return
/// `None` so the property is ignored, like every other invalid style value.
///
/// `ResizeUpLeftDownRight` is the NorthWest/SouthEast cursor on every backend,
/// so it is `nwse-resize`. GPUI's doc comments and its browser backend named
/// the opposite CSS values until the pinned fork corrected them, so do not
/// "fix" this pair back by reading an older GPUI.
pub fn parse_cursor(name: &str) -> Option<gpui::CursorStyle> {
    use gpui::CursorStyle;
    Some(match name {
        "default" | "auto" => CursorStyle::Arrow,
        "pointer" => CursorStyle::PointingHand,
        "text" => CursorStyle::IBeam,
        "vertical-text" => CursorStyle::IBeamCursorForVerticalLayout,
        "crosshair" => CursorStyle::Crosshair,
        "grab" => CursorStyle::OpenHand,
        "grabbing" | "move" | "all-scroll" => CursorStyle::ClosedHand,
        "col-resize" => CursorStyle::ResizeColumn,
        "row-resize" => CursorStyle::ResizeRow,
        "ew-resize" => CursorStyle::ResizeLeftRight,
        "ns-resize" => CursorStyle::ResizeUpDown,
        "nwse-resize" | "nw-resize" | "se-resize" => CursorStyle::ResizeUpLeftDownRight,
        "nesw-resize" | "ne-resize" | "sw-resize" => CursorStyle::ResizeUpRightDownLeft,
        "w-resize" => CursorStyle::ResizeLeft,
        "e-resize" => CursorStyle::ResizeRight,
        "n-resize" => CursorStyle::ResizeUp,
        "s-resize" => CursorStyle::ResizeDown,
        "not-allowed" | "no-drop" => CursorStyle::OperationNotAllowed,
        "alias" => CursorStyle::DragLink,
        "copy" => CursorStyle::DragCopy,
        "context-menu" => CursorStyle::ContextualMenu,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_fill(fill: &str) -> StyleDesc {
        StyleDesc {
            background_color: Some(fill.to_owned()),
            ..Default::default()
        }
    }

    #[test]
    fn transparent_function_does_not_occlude() {
        assert!(!should_occlude(&with_fill("transparent")));
        assert!(!should_occlude(&with_fill("oklch(50% 0.2 30 / 0%)")));
    }

    #[test]
    fn invalid_fill_keeps_conservative_occlusion() {
        assert!(should_occlude(&with_fill("not-a-color")));
    }

    #[test]
    fn selectors_read_from_json() {
        let json = r#"{ "selectors": [{ "on": ":first-child", "style": { "color": "red" } }] }"#;
        let style: StyleDesc = serde_json::from_str(json).unwrap();
        let rules = style.selectors.unwrap();
        assert_eq!(rules[0].on, ":first-child");
        assert_eq!(rules[0].style.color.as_deref(), Some("red"));
    }

    #[test]
    fn every_name_the_writer_uses_is_a_name_the_reader_knows() {
        let written = serde_json::to_value(StyleDesc::default()).unwrap();
        let written = written.as_object().unwrap();
        for name in written.keys() {
            assert!(
                FIELDS.contains(&name.as_str()),
                "`{name}` is written but never read"
            );
        }
        assert_eq!(written.len(), FIELDS.len());
    }

    #[test]
    fn a_style_reads_the_same_shapes_it_always_did() {
        let style: StyleDesc = serde_json::from_str(
            r#"{
                "paddingTop": 8,
                "gap": "var(--gap)",
                "width": "100%",
                "height": "auto",
                "fontWeight": "bold",
                "lineClamp": null,
                "hover": { "color": "red" }
            }"#,
        )
        .unwrap();
        assert_eq!(style.padding_top, Some(Numeric::Number(8.0)));
        assert_eq!(style.gap, Some(Numeric::Text("var(--gap)".to_owned())));
        assert_eq!(style.width, Some(Numeric::Text("100%".to_owned())));
        assert_eq!(style.height, Some(Numeric::Text("auto".to_owned())));
        assert_eq!(
            style.font_weight,
            Some(FontWeightValue::Str("bold".to_owned()))
        );
        assert_eq!(style.line_clamp, None);
        assert_eq!(style.hover.unwrap().color.as_deref(), Some("red"));
    }

    #[test]
    fn a_custom_property_is_kept_and_any_other_unknown_key_is_dropped() {
        let style: StyleDesc = serde_json::from_str(
            r#"{ "--pad": "8px", "--depth": 3, "paddingg": 8, "somethingNew": true }"#,
        )
        .unwrap();
        assert_eq!(
            declared_variables(&style),
            vec![
                ("--depth".to_owned(), "3".to_owned()),
                ("--pad".to_owned(), "8px".to_owned()),
            ]
        );
        assert_eq!(style.custom.len(), 2);
    }

    #[test]
    fn a_repeated_key_takes_the_later_value() {
        let style: StyleDesc =
            serde_json::from_str(r#"{ "gap": 4, "gap": 8, "--pad": 1, "--pad": 2 }"#).unwrap();
        assert_eq!(style.gap, Some(Numeric::Number(8.0)));
        assert_eq!(
            declared_variables(&style),
            vec![("--pad".to_owned(), "2".to_owned())]
        );
    }

    #[test]
    fn the_boxed_read_and_the_ordinary_read_agree() {
        let json =
            r#"{ "gap": 8, "color": "red", "--pad": "4px", "hover": { "gap": 2 }, "nope": 1 }"#;
        assert_eq!(
            *StyleDesc::from_json_boxed(json).unwrap(),
            serde_json::from_str::<StyleDesc>(json).unwrap()
        );
    }

    #[test]
    fn the_boxed_read_rejects_trailing_text() {
        assert!(StyleDesc::from_json_boxed(r#"{ "gap": 8 } and then some"#).is_err());
    }

    #[test]
    fn a_style_survives_a_round_trip_through_json() {
        let style = StyleDesc {
            gap: Some(Numeric::Text("calc(1rem + 2px)".to_owned())),
            font_size: Some(Numeric::Number(14.0)),
            max_width: Some(Numeric::Number(320.0)),
            user_select: Some("none".to_owned()),
            custom: [("--pad".to_owned(), serde_json::json!("8px"))]
                .into_iter()
                .collect(),
            hover: Some(Box::new(StyleDesc {
                background_color: Some("#fff".to_owned()),
                ..Default::default()
            })),
            ..Default::default()
        };
        let text = serde_json::to_string(&style).unwrap();
        assert_eq!(serde_json::from_str::<StyleDesc>(&text).unwrap(), style);
    }

    #[test]
    fn the_object_form_of_a_gradient_reads_its_stops_and_colour_space() {
        let style: StyleDesc = serde_json::from_str(
            r##"{"background":{"type":"linear-gradient","angle":90,"stops":[{"color":"#ff0000","position":0},{"color":"#0000ff","position":1}],"colorSpace":"oklab"}}"##,
        )
        .unwrap();
        let Some(BackgroundValue::Gradient(gradient)) = style.background else {
            panic!("expected the object form to read as a gradient");
        };
        assert_eq!(gradient.angle, 90.0);
        assert_eq!(gradient.stops.len(), 2);
        assert_eq!(gradient.stops[1].color, "#0000ff");
        assert_eq!(gradient.color_space.as_deref(), Some("oklab"));
    }

    #[test]
    fn an_object_of_another_type_is_not_a_background() {
        let result = serde_json::from_str::<StyleDesc>(
            r##"{"background":{"type":"radial-gradient","angle":0,"stops":[]}}"##,
        );
        assert!(result.is_err());
    }

    #[test]
    fn transparent_gradient_does_not_occlude() {
        let style: StyleDesc = serde_json::from_str(
            r##"{"background":{"type":"linear-gradient","angle":0,"stops":[{"color":"transparent","position":0},{"color":"#00000000","position":1}]}}"##,
        )
        .unwrap();
        assert!(!should_occlude(&style));

        let text: StyleDesc =
            serde_json::from_str(r##"{"background":"linear-gradient(transparent, #00000000)"}"##)
                .unwrap();
        assert!(!should_occlude(&text));
        let painted: StyleDesc =
            serde_json::from_str(r##"{"background":"linear-gradient(transparent, red)"}"##).unwrap();
        assert!(should_occlude(&painted));
    }

    #[test]
    fn maps_the_timeline_cursors() {
        assert_eq!(
            parse_cursor("col-resize"),
            Some(gpui::CursorStyle::ResizeColumn)
        );
        assert_eq!(parse_cursor("grab"), Some(gpui::CursorStyle::OpenHand));
        assert_eq!(
            parse_cursor("grabbing"),
            Some(gpui::CursorStyle::ClosedHand)
        );
        assert_eq!(
            parse_cursor("pointer"),
            Some(gpui::CursorStyle::PointingHand)
        );
        assert_eq!(parse_cursor("default"), Some(gpui::CursorStyle::Arrow));
    }

    #[test]
    fn ignores_an_unknown_cursor() {
        assert_eq!(parse_cursor("zoom-in"), None);
        assert_eq!(parse_cursor("POINTER"), None);
    }
}

/// The custom properties an element declares, as declared text.
///
/// A number becomes a plain string, so `{ "--pad": 8 }` and `{ "--pad": "8" }`
/// mean the same thing. That matches the `style` prop, where a bare number is
/// already how a length is written.
pub fn declared_variables(style: &StyleDesc) -> Vec<(String, String)> {
    let mut declared: Vec<(String, String)> = style
        .custom
        .iter()
        .filter(|(name, _)| name.starts_with("--"))
        .filter_map(|(name, value)| {
            let text = match value {
                serde_json::Value::String(text) => text.clone(),
                serde_json::Value::Number(number) => number.to_string(),
                // `undefined` reaches Rust as null. CSS has no way to write an
                // undeclared value, so treat it as absent.
                _ => return None,
            };
            Some((name.clone(), text))
        })
        .collect();
    // A HashMap has no order, and the cascade compares these to decide whether
    // a subtree re-resolves. Without a sort the same declarations could compare
    // unequal from one frame to the next.
    declared.sort_by(|a, b| a.0.cmp(&b.0));
    declared
}
