# CSS values, the cascade, and `className`

Status: draft, waiting for sign-off. Branch: `feat/css-values-and-classname`.

Revision 2. An architecture review of revision 1 changed eight decisions. The section
"What revision 2 changed" lists them, with the reason for each.

## What this adds

GPUIX today takes a finished style object from JavaScript. Every length is a pixel number.
Every colour is a string that `parse_color` reads once. Nothing is left to work out.

This document specifies four layers:

0. `gpuix-css`, a crate that parses CSS values and resolves `var()`. It does not link gpui.
1. A resolved style seam in `gpuix-native`. One function turns a style plus its inherited
   environment into a `gpui::StyleRefinement`, and the result is cached per element.
2. A `className` prop in `@gpuix/react`, resolved through one function and one root option.
3. `@gpuix/tailwind`, a thin adapter that turns Tailwind classes into CSS declarations.

The goal is not "make Tailwind work". The goal is to make GPUIX understand CSS, so that
Tailwind works because it emits CSS.

## What the code does today

These facts come from gpuix at `9f0fb6d` and the pinned zed fork at `4d80927`. Read them
before you design against them.

### GPUIX

- `className` is in `RESERVED_PROPS` at `packages/react/src/reconciler/host-config.ts:134`.
  The reconciler drops it without a message.
- `apply_styles` at `renderer.rs:2971-3277` is 307 lines and 52 `if let Some` branches.
  It runs for every element on every frame.
- `renderer.rs:2571` already calls `apply_styles(refinement, hover_style)` on a bare
  `StyleRefinement`, because `apply_styles` is generic over `E: Styled`.
- `RetainedElement` (`retained_tree.rs:13-31`) holds `style: Option<StyleDesc>` and
  `subtree_revision: u64`.
- `RetainedTree::set_style` (`retained_tree.rs:170-181`) compares the old style against the new
  one and marks the element changed only when they differ.
- `packages/native/src/color.rs` uses `csscolorparser 0.8.3`. Named colours, hex, `rgb()`,
  `hsl()`, `hwb()`, `lab()`, `lch()`, `oklab()`, `oklch()` and relative `from` syntax all parse.
  `color-mix()` does not.
- `DimensionValue` in `style.rs` accepts a number, `"N%"` or `"auto"`, and applies to `width`,
  `height` and the min and max variants only. Every other length field is a bare `f64`.
- `StyleDesc.boxShadow` holds one `BoxShadowValue`, not a list.
- `StyleDesc.background` is only a fallback colour for `backgroundColor` (`renderer.rs:3150`).
- `Inherited` (`renderer.rs:1833-1868`) carries two fields, `selectable` and `selection_wash`.
- `renderer.rs` is 3,683 lines.
- Four call sites send a style: `host-config.ts:125` (`sendStyle`), `:369` (`commitUpdate`),
  `:395` (`hideInstance`), `:399` (`unhideInstance`). `setStyle` replaces, never merges.

### GPUI, in the pinned fork

- `Styled::style(&mut self) -> &mut StyleRefinement` (`crates/gpui/src/styled.rs:24`).
- `Style` derives `Refineable`, with `#[refineable(Debug, PartialEq, Serialize, Deserialize,
  JsonSchema)]` (`crates/gpui/src/style.rs:178`). So `StyleRefinement` compares, serializes and
  deserializes.
- `Refineable::Refinement` is itself `Refineable` with the same `Refinement` type
  (`crates/refineable/src/refineable.rs:30`). Refinements merge into refinements.
- Every variant API takes the same type (`crates/gpui/src/elements/div.rs`):
  `hover` at 806, `group_hover` at 816, `focus` at 1213, `active` at 1500,
  `group_active` at 1509, `group_drag_over` at 1150, all
  `impl FnOnce(StyleRefinement) -> StyleRefinement`. `group(name)` at 737.
  There is no `group_focus`.
- `Style.padding` is `Edges<DefiniteLength>`. `margin` and `inset` are `Edges<Length>`.
  `gap` is `Size<DefiniteLength>`. `flex_basis` is `Length`. `border_widths` is
  `Edges<AbsoluteLength>`. Percentages, rems and `auto` already work in all of them.
  GPUIX flattens each one to a pixel number and throws the capability away.
- `linear_gradient(angle, from, to)` takes exactly two colour stops. No radial, no conic.
- taffy is pinned `=0.13.0` and has calc. GPUI's `Length` has no calc variant, so nothing
  reaches taffy's resolver.

## What revisions 2 and 3 changed

| # | Change | Reason |
|---|--------|--------|
| 1 | `resolve()` returns a `gpui::StyleRefinement`, cached per element. `apply_styles` is deleted. | Revision 1 stacked CSS work on a 52-branch per-frame scan, then set a 2% budget to contain it. GPUI already has the type and the merge. |
| 2 | ~~One `when` list replaces `hover`, `active`, `focus`, `groupHover`, `groupActive`, `groupFocus` and `media`.~~ **Reversed in revision 3.** `style` carries no conditions at all. | Revision 2 replaced seven fields with one list. Revision 3 removed the list. A CSS style attribute holds declarations, so a condition belongs in a class. See layer 2. |
| 3 | `gpuix-css` is its own crate and does not depend on gpui. | Revision 1 put pure value tests behind Metal, the zed submodule and a macOS runner. |
| 4 | `hideInstance` joins the call sites that funnel through one style function. | Revision 1 named three of four. The missing one is the only one that destroys state. |
| 5 | The resolver is a `createRoot` option, not a global. `invalidateClassNameCache` is gone. | One adapter is a hypothetical seam. A global also serialises tests that vitest runs in parallel. |
| 6 | Rust keeps `StyleDesc`. Every numeric field deserializes through one `Numeric` type, and colour fields stay strings. | The full move to declarations-only is real but it is not this branch. See follow-ups. |
| 7 | The steady-state gate asserts a counter, not a duration. | A 2% wall-clock band on a CI runner is noise. It would be muted, and a muted gate reads as coverage. |
| 8 | The cascade lives in `packages/native/src/cascade.rs`. | `renderer.rs` is already 3,683 lines. |

Revision 4 widens layer 4. The goal is Tailwind in its entirety. `first:`, `last:`, `odd:`,
`even:` and `only:` become index conditions, and the walk evaluates them from the child index.
`space-x-*`, `divide-*`, `*:` and `**:` become child conditions, and they flow down with
`Inherited`. Every variant that still drops now names its prerequisite. See "What still
drops, and why".

## Layer 0: the `gpuix-css` crate

New crate at `packages/native/css`, named `gpuix-css`. It depends on `lightningcss` with
`default-features = false`, and on nothing else. It must not depend on gpui.

`gpuix-native` adds it as a path dependency. `csscolorparser` is removed.

### Why a separate crate

`gpuix-native` links gpui, gpui_platform, gpui_macos, core-text, core-graphics, fifteen
tree-sitter grammars and the whole zed submodule. The value tests in this specification are
pure functions: a CSS string goes in, a value comes out. Inside `gpuix-native` they need a
Metal toolchain, the zed checkout and a macOS runner, and CI runs macos-latest only.

In their own crate they run on Linux in milliseconds with `cargo test -p gpuix-css`.

This seam is real, not hypothetical. Two adapters already sit on it: the `style` prop and
`className`.

### Interface

```rust
pub fn parse(property: &str, value: &str) -> Result<Parsed, CssError>;
pub fn substitute(unparsed: &Unparsed, vars: &Vars) -> Result<Parsed, CssError>;

pub enum Parsed {
    Ready(Property<'static>),   // no var(), folded now
    Pending(Unparsed),          // contains var(), finish later
}
```

Three items. Everything else in the crate is private.

### What lightningcss provides

- `Property::parse_string(property_id, input, options)` parses a declaration into a typed value.
- `Property::Unparsed(UnparsedProperty)` holds a value that contains `var()`.
  `UnparsedProperty::substitute_variables(&self, vars)` finishes it.
- `Property::Custom(CustomProperty)` holds a `--x` declaration.
- `CssColor` covers every CSS colour grammar, plus `color-mix()` and `currentColor`.
  `Calc<V>` folds during the parse.
- `MediaQuery` and `MediaCondition` parse. There is no evaluator, because lightningcss is a
  compiler. Write that in `gpuix-css` and keep it pure: it takes a size, not a window.

Tailwind v4 ships the same library. `@tailwindcss/node@4.3.3` depends on `lightningcss@1.32.0`.

Do not gate the crate behind a cargo feature. Measure the `.node` size before and after, and
put both numbers in the pull request body.

### What folds now, and what does not

A value with no `var()` folds the moment it arrives, once, forever:

```
padding: calc(1rem + 2px)     ->  Ready
background-color: oklch(...)  ->  Ready
width: calc(1 / 2 * 100%)     ->  Ready, 50%
```

A value that mentions `var()` becomes `Pending`, because it depends on the element's ancestors.

`calc()` that mixes a percentage with a length is rejected. Name the expression in the error:

```
calc(100% - 20px)  ->  error: mixed percentage and length in calc, not supported
```

taffy can do this. GPUI cannot express it without a calc variant on `Length` in the zed fork,
and every future upstream sync would carry that patch. It is a follow-up.

### Supported functions

Ship `calc()`, `min()`, `max()`, `clamp()`, `var()`, `color-mix()`, `linear-gradient()`.

Leave out `radial-gradient()`, `conic-gradient()`, `env()`, `attr()`, `image-set()`.

`linear-gradient()` maps to `gpui::linear_gradient(angle, from, to)`, which takes exactly two
stops. If a gradient declares more, keep the first and the last, drop the rest, warn once.

### Units

Keep `rem` symbolic, so a `rem` reaches GPUI as `AbsoluteLength::Rems`. A change to the window
rem size then reflows with no style re-resolution.

A `calc()` that mixes `rem` with `px` must fold, so it takes the rem size as a parameter. That
is the one place `rem` stops being live. Say so in the reference documentation.

`border-*` is `AbsoluteLength` in GPUI, so it takes `px` and `rem` and rejects percentages.
Report that as an error rather than rounding to zero.

## Layer 1: the resolved style seam

### The problem this replaces

`apply_styles` is a shallow module. Its interface is 307 lines long, because you cannot know
what it does without reading all of it. It runs per element per frame, scanning 52 optional
fields that are almost always `None`.

Revision 1 added CSS parsing, variable substitution and media evaluation on top of that loop.

### The seam

```rust
// packages/native/src/style/resolve.rs
pub fn resolve(style: &StyleDesc, env: &Cascade) -> StyleRefinement;
```

One function. It returns GPUI's own type.

This works because `Styled::style()` returns `&mut StyleRefinement`, `Style` derives
`Refineable`, and a `Refinement` is itself `Refineable`, so refinements merge into refinements.
`renderer.rs:2571` already calls `apply_styles` on a bare `StyleRefinement`, so the pattern is
in the codebase already. It was never named.

### The cache

Add one field to `RetainedElement`:

```rust
pub resolved: Option<Resolved>,

pub struct Resolved {
    refinement: StyleRefinement,
    variants: Vec<(Condition, StyleRefinement)>,
    /// The cascade generation that produced this. Compare before reuse.
    generation: u64,
}
```

`RetainedTree::set_style` already compares the old style against the new one
(`retained_tree.rs:173`). Clear `resolved` in the branch that already exists. There is no new
invalidation point to invent.

Per frame, an element that has not changed does:

```rust
el.style().refine(&cached.refinement);
```

That is a merge of set fields. No branch scan, no parsing, no substitution.

### Applying variants

Every GPUI variant API takes `impl FnOnce(StyleRefinement) -> StyleRefinement`, so a cached
variant refinement applies directly:

```rust
for (condition, refinement) in &cached.variants {
    el = match condition {
        Condition::Hover  => el.hover(|_| refinement.clone()),
        Condition::Active => el.active(|_| refinement.clone()),
        Condition::Focus  => el.focus(|_| refinement.clone()),
        Condition::Group { name, state: GroupState::Hover }  => el.group_hover(name, |_| refinement.clone()),
        Condition::Group { name, state: GroupState::Active } => el.group_active(name, |_| refinement.clone()),
        // A media condition is not a GPUI variant. It is evaluated during the walk.
        Condition::Media { .. } => el.style().refine(refinement),
        // An index condition is not one either. The walk knows the child index
        // and the child count, and merges when the test holds.
        Condition::Index { .. } => el.style().refine(refinement),
    };
}
```

Use GPUI's plain `focus`, not `focus_visible` or `in_focus`. Tailwind's `focus:` is the
unqualified one.

A media condition is not a GPUI variant. Evaluate it against the window size during the walk
and merge the refinement when it matches.

An index condition works the same way. The retained tree stores children in order, and the
walk visits each child with its index and the child count. `first`, `last`, `odd`, `even` and
`only` are tests on those two numbers, so they need no selector engine. A list mutation
changes the numbers, and the next frame re-evaluates them, the same way a resize re-evaluates
a media condition.

### Delete `apply_styles`

The 307-line function becomes the private body of `resolve`, converted to write into a
`StyleRefinement` rather than to chain builder calls on `E: Styled`. Nothing else calls it.

Note the counterintuitive result: the body does not shrink much at first. The win is that it
runs once per style change instead of once per element per frame, that its output is a value
you can compare and serialize, and that the cascade and the variants get a return type.

## Layer 1b: the cascade

New module: `packages/native/src/cascade.rs`. `Inherited` moves here from `renderer.rs:1833`.

```rust
impl Cascade {
    pub fn root(theme: &Theme, window: Size<Pixels>) -> Self;
    pub fn descend(&self, style: Option<&StyleDesc>) -> Self;
    pub fn resolve(&self, style: &StyleDesc) -> Resolved;
}
```

Three methods hide inheritance, the variable map and condition evaluation. The tree walk in
`renderer.rs` calls `descend` going down and `resolve` at each node, and learns nothing about
any of it.

### Inheritance

**GPUI already does this, and revision 2 said the opposite.** Revision 2 claimed a `color` on a
div does not reach a nested `<text>`, called the fix a visible behaviour change, and asked for a
changeset naming it. That is wrong. It was written from reading `apply_styles`, which has no
inheritance in it, without checking what GPUI does underneath.

A `div` pushes its text style onto a window stack at `div.rs:1840`, and `window.text_style()`
composes the whole stack. A `<text>` with no style of its own paints with the nearest ancestor
declaration. `SelectableText` already documents its dependence on this at
`packages/native/src/text/paint.rs:155`.

Measured, not reasoned. Each row declares the property on the ancestor, on the text itself, and
nowhere, then compares the three screenshots byte for byte.

| Property | Ancestor against nothing | Ancestor against text |
| --- | --- | --- |
| `color` | 0.02 | identical |
| `fontSize` | 0.01 | identical |
| `fontWeight` | 0.01 | identical |
| `fontFamily` | 0.43 | identical |
| `lineHeight` | 0.86 | identical |
| `textAlign` | 0.48 | identical |

Lower means more different. The left column shows the declaration does something. The right
column shows the ancestor and the text produce the same pixels, which is inheritance.

`packages/react/src/__tests__/inheritance.test.tsx` pins all six, plus a nested case where the
nearer ancestor wins. The behaviour comes from the pinned fork rather than from this repository,
so a fork bump could remove it silently. That is what the test is for.

So there is no inheritance work for text properties, no behaviour change, and nothing for a
changeset to name. What is left of the original list:

- `userSelect` and `selectionColor` already inherit through `Cascade`, which is the old
  `Inherited` struct moved out of `renderer.rs`.
- `cursor` does not inherit. It is not in GPUI's `TextStyle`, and a screenshot cannot see a
  cursor, so this needs a different test before it is worth building. Left out of this branch.
- Custom properties are the real work, and the rest of this section covers them.

A note on why the earlier claim survived review: every reviewer, including this one, read the
GPUIX code and stopped there. The behaviour lives one layer down.

### Custom properties

**Built. What follows is what shipped, not a proposal.**

Declare them in `style`, exactly as on the web:

```tsx
<div style={{ "--pad": "8px", "--brand": "oklch(0.6 0.2 20)" }}>
  <div style={{ padding: "var(--pad)", color: "var(--brand)" }} />
</div>
```

Type them with a template literal pattern index signature:

```ts
export interface StyleDesc {
  [key: `--${string}`]: string | number | undefined
  // ...the existing keys
}
```

Verified with `tsc --strict`: `"-pad"` is rejected because it does not match the pattern, and
`color: 42` stays an error. React's own `CSSProperties` uses an open index signature, which
lets every typo through.

Resolution runs in three steps.

1. Serde collects every key that is not a known field into `StyleDesc.custom` through
   `#[serde(flatten)]`. `declared_variables` keeps the `--` names and sorts them.
2. During the walk, `descend` layers the node's variables over the inherited map. It runs
   before the node's own style resolves, so a declaration is in scope for the `var()` beside it.
3. `Scope::value` substitutes textually, and the existing value parsers read the result as if
   the author had written it in place.

Substitution is textual and does not go through `gpuix-css`. The plan routed it through typed
`Property` values, which would mean parsing and re-emitting every inline style. Substituting
text keeps `StyleDesc` typed, keeps `gpuix-css` off the per-element path, and matches what CSS
says a custom property is: text, held uninterpreted until a property reads it.

Three rules keep this fast.

The variable map is shared by pointer. A node that declares no variables passes the parent's map
down unchanged, and a node that redeclares the value it already has keeps the same pointer too.

`Resolved.cascade` holds the cascade a resolution read, or `None` when it read nothing
inherited. `None` is the common case, and a resolution marked `None` survives every cascade
change. Only an element that used `var()` or `currentColor` is ever invalidated by an ancestor.
The key is the whole `Cascade` rather than the variable map alone, so an ancestor changing
`userSelect` also invalidates a `var()` reader below it. That is one pointer to compare instead
of two, and both changes are rare.

`descend` is memoized per element on the parent cascade pointer, and the root cascade is
memoized on the theme. Without both, a declaration would build a new `Arc` on every frame and
the whole subtree below it would re-resolve on every frame, which is exactly what the cache
exists to stop. `packages/react/src/__tests__/custom-properties.test.tsx` pins this with a
counter: ten frames over twenty readers under one declaration must add zero resolutions.

While wiring this, a second cache thrash turned up and is fixed. `sendStyle` skips the napi call
for an empty style at mount, but `commitUpdate` always sends `{}`. So the first update on every
element with no `style` prop read as a change and resolved a style with nothing in it.
`set_style` now stores `None` for a style that declares nothing, which makes the two paths agree.

Order inside one element does not matter. `var()` resolves against the element's final set of
custom properties, not against the position of the declaration. Tailwind depends on this:
`text-sm` emits `line-height: var(--tw-leading, var(--text-sm--line-height))` while `leading-6`
emits `--tw-leading`, and both land on the same element in either order.

`var()` supports a fallback, including an empty one. Tailwind emits `var(--tw-ring-inset,)`.

### currentColor

**Built.** `Cascade` tracks the computed `color` and `Scope::color` resolves the keyword against
it. Tailwind needs this because `ring-*` emits `var(--tw-ring-color, currentcolor)`.

The root starts at `gpui::black()`, which is what `TextStyle::default` uses, so the cascade's
copy of the colour and GPUI's own text style stack agree without setting a colour on the root
wrapper. Setting one would change how every unstyled `<text>` paints, and that is a separate
decision.

Two limits. Only a bare `currentColor` resolves, so one nested inside `color-mix()` falls
through to the colour parser and fails there. And `color: currentColor` declares nothing,
because CSS computes it to `inherit`.

### Window media queries

**Not built.** Nothing emits a media condition until `className` lands in layer 3, and building
an evaluator with no producer would repeat the mistake that `group` was. The design below stands.

`gpuix-css` parses the condition. `Cascade` holds the window size and evaluates it.

On a resize, bump the cascade generation and re-resolve. This is the same invalidation the
variable cascade already needs, so it costs no new machinery.

Treat `@media (hover: hover)` as always true. Tailwind wraps every `hover:` utility in it, and
a desktop window always has a pointer.

Container queries stay out of this branch. A container query makes style depend on layout and
layout depend on style, and terminating that loop needs real containment rules.

## Layer 2: `StyleDesc` changes

### Numeric fields take text

**Built.** Every numeric field on `StyleDesc` was `Option<f64>`, which cannot hold
`var(--pad)`. All 36 now deserialize through one `Numeric` type:

```rust
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Numeric {
    Number(f64),
    Text(String),
}
```

In TypeScript that is `number | string`. A bare number still means pixels, so `8`, `"8px"` and
`"var(--pad)"` all declare the same padding. `Scope::length` hands the text to
`gpuix_css::length`, which reads a number, a `px` or `rem` length, a percentage, and any
`calc()`, `min()`, `max()` or `clamp()` over them. A unit it cannot fold drops the declaration,
because painting 2 pixels for `2vw` is worse than painting nothing. The unitless fields such as
`opacity` and `flexGrow` widened too, since `var()` is legal in any property and a field left
as `f64` would reject it.

### Lengths, `calc()` and `rem`

**Built.** `packages/native/css/src/length.rs` reads one length and returns pixels, a fraction
or a bare number. lightningcss does the parsing, which means `calc()`, `min()`, `max()` and
`clamp()` fold while the value parses, with no evaluator of ours in the middle. Two shapes need
handling before the handoff.

A bare `1.5` reads as `1.5px` in lightningcss, because CSS quirks mode says so. A number is
read first and never reaches the parser.

`rem` is rewritten to pixels before parsing, against the window rem size. lightningcss holds
`rem` as a relative length and will not add it to a `px`, so `calc(1rem + 4px)` would come back
unfolded. This deviates from what layer 3 planned, which was to keep `rem` symbolic all the way
to `AbsoluteLength::Rems` so a rem size change reflowed with no re-resolution. Folding early
costs that: the root cascade keys on the window rem size, so a `set_rem_size` call re-resolves
every style that reads a rem. Nothing in GPUIX calls it today.

### A `lineHeight` string is a multiple, a number is pixels

**Built.** A JS number keeps the old GPUIX meaning, so `lineHeight: 20` is 20 px, as in React
Native. A string follows CSS: a bare number in a string is a multiple of the font size and
reaches GPUI as `gpui::relative(n)`. A percentage is the same multiple. A length keeps its
unit. Zero or less declares nothing.

`packages/react/src/__tests__/css-lengths.test.tsx` pins all of these against a wrapped
paragraph, with a differ check first, so a line height that quietly did nothing would fail
rather than pass.

### Reading a style without buffering it

**Built.** `Numeric` and `FontWeightValue` were `#[serde(untagged)]` and `StyleDesc` was
`#[serde(flatten)]`. Each of those makes serde read the whole value into an intermediate tree
before it looks at one field, and every `setStyle` paid for it. All three now have a hand
written `Deserialize`. A `style_desc!` macro declares `StyleDesc` and its reader from one field
list, so the name JS writes and the name Rust reads come from the same literal. The wire format
did not change, and one test reads what `Serialize` writes against the names the reader knows,
which fails if the two halves ever disagree.

That left the cost of the struct itself. `StyleDesc` is 1,728 bytes, so the parse spent more
time moving it than reading it. `StyleDesc::from_json_boxed` writes into a `Box` from the
start, through the same `fill` the ordinary `Deserialize` uses, so the two cannot disagree.
Measured over 200,000 parses:

| shape | before | hand written | into a box | no flatten, no untagged |
| --- | --- | --- | --- | --- |
| two fields | 320 ns | 178 ns | 84 ns | 74 ns |
| eleven fields | 531 ns | 466 ns | 399 ns | 341 ns |

A `RetainedElement` holds that box rather than the struct, which takes it from 2,000 bytes to
280. A tree of 10,000 elements was carrying 17 MB of styles that were mostly empty. `BatchOp`
shrank the same way, since one `SetStyle` variant made every op in a batch as wide as a style.

### `style` holds declarations, never selectors

Revision 2 proposed a `when` list on `StyleDesc`, holding conditions such as hover, active,
group and media, plus a `group` field to name an ancestor. Revision 3 removes both. They were
built, tested and then deleted.

[CSS Style Attributes][css-style-attr] defines the attribute value as

> the syntax of the contents of a CSS declaration block (excluding the delimiting braces)

and nothing else. It cannot express `:hover`, it cannot express
`.card:hover &`, and it cannot express `@media`. Every condition in CSS comes from a selector or
an at-rule, and both of those live in a stylesheet reached through a class. GPUIX must behave
the same way, so `style` carries no conditions and every condition arrives through `className`.

The `group` field was the clearer mistake. It copied Tailwind's model, where `group` is a class
name rather than a property, and it duplicated `className` before `className` shipped. Naming an
ancestor is what a class already does.

`hover` and `active` stay on `StyleDesc` because they predate this plan and removing them now
would leave no way to express a hover until layer 3 lands. They are the two exceptions, they get
no siblings, and they are candidates for removal in the release that ships `className`. That
removal is a breaking change and needs its own sign-off.

### What GPUI actually models as style

Worth recording, because it took a trait-by-trait read of the pinned fork to establish and it
decides where conditions belong.

| Method | Trait |
| --- | --- |
| `group`, `hover`, `focus`, `in_focus`, `group_hover` | `InteractiveElement` |
| `active`, `group_active` | `StatefulInteractiveElement` |
| every layout, colour and text setter | `Styled` |

Not one conditional method is on `Styled`, and `Styled::style()` returns only the base
refinement. GPUI does not treat a condition as a style property either. It treats it as
interactivity. The existing `style.hover` already conflated the two before this plan started.

Three more facts from the same read, so layer 3 does not rediscover them:

- `hover()` holds `debug_assert!(hover_style.is_none(), "hover style already set")`. Two rules
  that resolve to the same condition on one element must merge into a single call, or a debug
  build panics.
- There is no `group_focus`. Tailwind's `group-focus:` has nothing to map onto.
- `focus()` needs `track_focus` and a focus handle. A plain `div` owns neither, so `:focus`
  needs focus-handle plumbing before it can work.

### Resolved conditions

`Resolved` currently holds `base`, `hover` and `active`. Layer 3 replaces those two fields with
`Vec<(Condition, StyleRefinement)>`, because a stylesheet can produce any number of conditions
and can produce the same one twice. `Condition` is an internal type built by the selector
parser. It never crosses the FFI boundary, so it needs no serde and no unknown-kind variant.

## Layer 3: the `className` seam

### The prop

**Built.** `className?: string` on the shared `Props` base. Both `jsx-runtime.d.ts` and
`jsx-dev-runtime.d.ts` map every intrinsic element to `Props`, so they needed no edit.

`string | undefined` is the whole type, so `clsx` and `cn` work with no special handling.

### What a resolver returns

**Built, and different from what this section first planned.** The plan had the resolver
return declarations, `Array<{ on: Condition | null; declarations: Array<[string, string]> }>`.
It returns a `StyleDesc` instead:

```ts
export type ClassNameResolver = (token: string) => StyleDesc | null
```

Declarations would have put a CSS property name to `StyleDesc` key table in TypeScript, next to
the one Rust already has, and the two would drift. `StyleDesc` already carries `hover` and
`active`, so the shape loses nothing that `setStyle` can carry today. The adapter owns every
piece of CSS knowledge and layer 3 is a merge with no table in it.

The cost is that only `hover` and `active` are reachable from a class. Focus, group and media
conditions need the resolved-style seam of layer 1 rather than `setStyle`, which takes a
`StyleDesc`. Layer 4 warns and drops them, as it already does for `group-focus:`.

### One function, four call sites

```ts
function computeStyle(props: Props, container: Container): StyleDesc
```

**Built.** All four call sites use it: `sendStyle`, `commitUpdate`, `hideInstance` and
`unhideInstance`.

`hideInstance` is the one revision 1 missed, and it is the only one that destroys state.

It used to send `{ visibility: "hidden" }` over the inline style, and `setStyle` replaces
rather than merges. That round-tripped only because `props.style` was the sole source of truth.
With `className` it is not, so hiding an element would discard every class-derived style and
`unhideInstance` would restore only the inline prop. React drives that pair for Suspense, so
the symptom is content that unstyles itself after it suspends.

`hideInstance` now sends `computeStyle(props, container)` with `visibility` overridden, and
with `hover` and `active` dropped, or a hover style that sets `visibility` would paint an
element React asked to hide. `host-config-style.test.tsx` pins both.

### Registering a resolver

**Built.** The resolver is an option on the root, not a global:

```ts
createRoot(renderer, { resolveClassName })
```

`createTestRoot` takes the same options, so a test registers a resolver over a fixed table.

One adapter means a hypothetical seam, and this specification plans exactly one, Tailwind v4.
It already puts v3 behind a different seam, `TailwindEngine`. A global buys nothing here.

A global costs something. Global mutable state plus a global cache means two tests with
different `appearance` settings cannot run at once, and vitest runs files concurrently, and
`createTestRoot()` drives a real renderer.

There is no precedent for a global setter in this package. `packages/react/src/index.ts`
exports `createRenderer`, `render` and `resetRender`, and `src` contains no global setter at all.

If no resolver is set and an element has a `className`, do nothing and print one development
warning. Never throw.

`invalidateClassNameCache()` does not exist. The cache's lifetime is the root's. Exporting a
manual invalidation would make the caller responsible for knowing when the cache is stale,
which is knowledge the module owns.

### Precedence

**Built.** [CSS Style Attributes][css-style-attr] settles this, and it is stricter than it first looks:

> These declarations are considered to have author origin and a specificity higher than any
> selector.

So `style` beats `className` always, key by key, and it beats a conditional rule too. Given
`style={{ backgroundColor: "red" }}` and a class that sets `background-color` on hover, the
element stays red while hovered. A browser behaves the same way, and only `!important` changes
it. GPUIX has no `!important`.

That is a constraint on the resolver, not a note. A condition resolved from `className` must not
write a key the `style` prop already set, or the element will change colour on hover where CSS
says it must not.

`motion` keeps overwriting its eight numeric keys every frame, ahead of both.

A conflict inside one class string is the adapter's problem. There is no specificity and no
selector engine among classes. That rule is flat: last write wins.

[css-style-attr]: https://www.w3.org/TR/css-style-attr/#cascading

### The cache

**Built.** Cache one class token, not one class string.

`clsx("p-4", isActive && "bg-blue-500", isLarge && "text-lg")` produces up to eight strings from
three tokens. Five toggles produce thirty-two. A token cache stores the tokens.

A cached token holds the `StyleDesc` the resolver returned, or `null` for a token it rejected,
so an unknown class is asked about once.

A bounded cache over whole strings sits in front, 256 entries, least recently used out first.
It matters because the same string usually repeats between two frames, and then neither the
split nor the merge runs. The token cache under it is unbounded, because the set of tokens an
application uses is fixed by its source code while the set of strings grows with every
combination of conditional classes. Both live on the root.

One test drives five class strings built from three tokens and asserts the resolver saw exactly
those three.

## Layer 4: `@gpuix/tailwind`

### Version and modularity

Target Tailwind v4 only. npm `latest` is `4.3.3`. v3 lives on as `v3-lts` at `3.4.19` and needs
entirely different code, because v4 has no JavaScript config.

Build the seam anyway:

```ts
interface TailwindEngine {
  resolve(classes: string[]): CachedToken[]
}
```

Default it to `@gpuix/tailwind/v4`. Put the v4 code behind the interface from the first commit,
so adding v3 later is a new file rather than a refactor.

### Loading

```ts
const resolveClassName = await createTailwindResolver({ css: "./src/app.css" })
createRoot(container, { resolveClassName })
```

`__unstable__loadDesignSystem` is async, but resolution during a commit is synchronous, so the
application awaits the resolver before it mounts. A lazy resolver would render the first frame
unstyled.

Options accept `{ css: path }` or `{ source: "...css text..." }`. The path form reads the file
and resolves `@import` through the filesystem, which works under `bun --hot`.

A packaged application has no `node_modules`, so the path form will not work there. That is a
named follow-up.

### Resolving a class

```
class string -> ds.getClassOrder() -> ds.candidatesToAst() -> walk -> declarations
```

`getClassOrder` gives Tailwind's real precedence, which sorts by property rather than by source
order, and matches what a browser produces. Apply in that order, last write wins.

A user who wants `tailwind-merge` semantics runs `twMerge()` on the string first. The adapter
does not depend on it.

An unknown class returns an empty array from `candidatesToAst`. That is the support test.

Send `var()` through untouched. Rust owns the cascade. A value the adapter pre-flattened would
not respond to an ancestor that overrides the variable.

### Harvesting `@property`

The AST contains `@property` rules with `initial-value`. Collect them into the element's
variable defaults before the class declarations apply.

This is not optional. A bare `border` emits `border-style: var(--tw-border-style)` and the value
lives only in `@property --tw-border-style { initial-value: solid }`. Without the harvest,
`border`, `shadow-*`, `ring-*` and `space-*` all resolve to nothing.

### Variant mapping

Every variant becomes a `Condition`:

| Tailwind             | `Condition`                                        |
|----------------------|----------------------------------------------------|
| `hover:`             | `{ kind: "hover" }`                                |
| `active:`            | `{ kind: "active" }`                               |
| `focus:`             | `{ kind: "focus" }`                                |
| `group-hover/name:`  | `{ kind: "group", name, state: "hover" }`          |
| `group-active/name:` | `{ kind: "group", name, state: "active" }`         |
| `sm: md: lg:`        | `{ kind: "media", query }`                         |
| `max-lg: min-lg:`    | `{ kind: "media", query }`                         |
| `first:` `last:` `only:` | `{ kind: "index", test }`                      |
| `odd:` `even:`       | `{ kind: "index", test }`                          |

An unnamed group uses `""`.

Flatten `dark:` at resolve time from an `appearance: "dark" | "light"` option. Key the cache by
appearance and clear it when the value flips.

### Child conditions

`space-x-*`, `divide-*` and the `*:` variant compile to a selector on the children.
`space-x-*` and `divide-*` produce `:where(& > :not(:last-child))`, and `*:` produces
`& > *`. The class sits on the parent, and the declarations apply to the children.

The rules cross the FFI in one wire field. `StyleDesc` gains `selectors`, a list of
`{ on, style }` pairs, and the resolver is its only writer. The `style` prop type excludes
it. The spellings form a closed set: `:first-child`, `:last-child`, `:nth-child(odd)`,
`:nth-child(even)`, `:only-child`, `& > *`, `& > :not(:last-child)` and `& *`. An unknown
spelling warns once and drops.

The rules for the children ride in the walk context (`BuildCtx`), not in `Inherited`.
`Inherited` keys the resolution cache by pointer, so a refinement that changes per parent
would clear the cache of every child on every frame. The walk context costs nothing there,
because the rules apply at paint and never touch a cached resolution. Direct rules reach
one level and swap out at each depth. Descendant rules stack for the whole subtree. A rule
applies before the child's own declarations. `:where()` has specificity zero, so the
child's own declarations must win, and this order gives exactly that.

Two places sit outside the walk. A virtual-list row builds on its own, so it has no child
position and the index conditions do not apply to it. A custom element resolves its own
`StyleDesc`, so the rules of a parent stop at its border.

### What still drops, and why

The target is the whole of Tailwind, because the goal of this plan is CSS, and Tailwind emits
CSS. A variant the plan cannot build yet gets a named follow-up with its prerequisite, never a
permanent drop. Until its follow-up ships, the resolver warns once and drops: `group-focus:`,
`peer-*`, `has-*`, `motion-safe:`, `print:`.

| Variant | Prerequisite |
| --- | --- |
| `group-focus:` | GPUI has `group_hover` (`div.rs:816`) and `group_active` (`div.rs:1509`) but no `group_focus`. Track focus per group in GPUIX, or add the method upstream. |
| `peer-*` | The hover or focus state of an earlier sibling. The walk visits siblings in order, so it can carry the state of the peers it already passed. It needs the same state store as `group-focus:`. |
| `has-*` | The state of a descendant, which the walk has not reached yet. Read the state of the last frame, one frame late. A browser pays a comparable invalidation pass. |
| `motion-safe:` | The OS reduce-motion setting. GPUIX does not read it today. |
| `print:` | A print target. GPUIX does not print. |

`group-hover` is Tailwind's spelling, but the meaning is plain CSS: an ancestor in `:hover` plus
a descendant combinator. GPUI has `group(name)` and `group_hover(name, f)` natively. The
`Condition` names the CSS idea, not the Tailwind one.

### Reporting unsupported input

One option, `tolerance: "warn" | "error"`, default `"warn"`. `error` throws at resolve time.

The resolver always exposes `getUnsupported()`, whatever the tolerance is. It returns both
unknown classes and declarations that no GPUI style can hold, so a test asserts on one thing.

## Deviations from CSS

List these in the package README as well.

- No specificity and no selector engine. Precedence is flat and last write wins. The
  structural pseudo-classes do not need one. The walk reads the child index.
- `calc()` cannot mix a percentage with a length.
- A gradient has at most two colour stops. No radial or conic gradients.
- No `radial-gradient()`, `conic-gradient()`, `env()`, `attr()` or `image-set()`.
- No container queries.
- No transforms, transitions or CSS animations. Use `motion` instead.
- No `text-decoration`, no `z-index`.
- `border` sets `border-width` only. There is no border style beyond a solid fill.
- Percentage padding, margin and inset work. Percentage border width does not.
- `peer-*` and `has-*` wait on sibling and descendant state. See "What still drops, and why".
- Variant nesting is one level deep.
- `style` keeps `hover` and `active`, which a CSS style attribute cannot express. They predate
  this plan. They gain no siblings, and they are candidates for removal in the release that
  ships `className`.
- `group-focus` has no GPUI equivalent. `group-hover` and `group-active` do.

## Tests

Five tiers. The first three need no GPU.

### `gpuix-css` value tables

`cargo test -p gpuix-css`, on any platform, in milliseconds. Table-driven: a CSS value string
in, a `Parsed` out. Cover `calc`, every unit, `color-mix`, gradients, `var()` fallbacks
including the empty one, media conditions, and malformed input.

Port the existing `color.rs` tests unchanged. They already cover named colours, every colour
function family and relative syntax, so they become the proof that moving from `csscolorparser`
to `lightningcss` changed nothing.

### Resolution snapshots

`StyleRefinement` derives `Serialize` and `PartialEq` (`style.rs:178`), so a resolution test is
a value comparison with no window, no view and no GPU:

```rust
assert_eq!(resolve(&style, &cascade), expected);
```

Snapshot the serialized refinement for the wider cases. This is the tier that revision 1 could
not have, because `apply_styles` had no return value.

Cover: inheritance through three levels, a variable overridden in a subtree, a variable with an
empty fallback, a media condition on both sides of its boundary, and every `Condition` variant.

### The reconciler

`computeStyle` is a pure function, so test it directly. Cover the hide and unhide round trip
named in layer 3, className and style precedence, and the token cache under `clsx`-style input.

### Adapter snapshots and the coverage floor

Fixture files of class strings by category in `packages/tailwind/test/fixtures/`. Snapshot the
resolver's output, and print the matching `ds.candidatesToCss()` output inside the same snapshot
so a reviewer sees the CSS the mapping came from without running anything.

Hand-write edge cases: negative values, opacity modifiers, condition merging, `@theme` overrides,
and `text-sm` with `leading-6` in both orders.

Then walk all 23,337 entries of `ds.getClassList()`, resolve each one, and assert that the
supported fraction never drops below a number committed in the repository. On failure, print
every class that became unsupported.

This is the test that makes the suite hard to fake. A change that quietly breaks a utility
family fails here even when no snapshot covers it.

### Pixels

`comparePixels` pairs through `createTestRoot()`. Cover: a `className` and the equivalent
`style` render the same pixels, an inherited `color` reaches a nested `<text>`, a variable
override changes only its subtree, and a resize crosses a media query boundary.

## Performance gates

The reason to use GPUI instead of a browser is speed. A change that makes GPUIX slower has
failed, whatever else it does.

### Gate on counters, not on durations

The cache design makes a falsifiable claim: on a frame where nothing changed, zero styles are
resolved. Count the calls and assert the count.

```rust
assert_eq!(stats.style_resolutions, 0);    // steady state
assert_eq!(stats.style_resolutions, 1);    // after one setStyle
assert_eq!(stats.style_resolutions, 412);  // subtree under a changed variable
```

This is deterministic, it runs anywhere, and it fails for the exact reason a wall-clock gate
would be reaching for. It also names which elements re-resolved, which a timing number never
does.

A 2% wall-clock band on a shared macOS runner is noise. Thermal state and the GPU scheduler move
frame time more than that. Such a gate gets muted, and a muted gate reads as coverage.

The counter mechanism is half built. `renderer.rs:1085` and `:1105` already expose
`reset_debug_frame_overlay_stats` and `get_debug_frame_overlay_stats`, and there is a commit
titled "Add a chat performance regression test and overlay draw stats." Add one field.

### Gated

| Gate | Threshold |
|------|-----------|
| `style_resolutions` per steady-state frame, 10k elements with `var()` styles | exactly 0 |
| `style_resolutions` after one `setStyle` | exactly 1 |
| `style_resolutions` after one root variable change | exactly the subtree size |
| Mount, the existing 10k-row benchmark | within 10% of the baseline |

### Reported, not gated

Steady-state frame time, theme-change time and resize time. Record the baseline on `main`
before the first commit, and put both numbers in every pull request body. Read them. Do not
fail the build on them.

## Follow-ups

Write these into the specification. Do not build them now.

- **Declarations as the only input to Rust.** Once Rust owns the mapping, `StyleDesc` in Rust is
  a second hand-maintained mirror of CSS. Deleting it and sending declarations for the `style`
  prop as well would concentrate the mapping in one place. It is staged out of this branch for
  two reasons: the typed serde path is measurably cheaper than parsing CSS text for inline
  styles that carry dynamic numbers, and the migration touches every element type. Revisit once
  the counters from the performance section exist to measure it honestly.
- **`!important`.** Layer 3 gives `style` a specificity higher than any selector, per
  [CSS Style Attributes][css-style-attr]. That leaves a class with no way to override an inline
  declaration, because in the cascade an important author declaration is the only thing that
  outranks a normal inline one, and an important inline declaration outranks that in turn. This
  is not hypothetical: Tailwind's `bg-red-500!` compiles to `!important`, so every such utility
  is silently lost against any `style` prop touching the same key. Needs an importance flag on
  each declaration and four cascade levels rather than two. Decide before layer 4 ships, because
  adding importance later changes which declaration wins and is therefore breaking.
- **Conditions past hover and active.** A resolver returns a `StyleDesc`, which carries those
  two and nothing else, so focus, group and media conditions cannot reach an element from a
  class. Reaching them means a second napi call carrying the layer 1 `Resolved` shape rather
  than a `StyleDesc`. Layer 4 warns and drops them until then.
- Container queries. Needs containment rules to terminate the layout and style loop.
- Layout-time `calc()`. Needs a calc variant on GPUI's `Length` in the zed fork, wired to
  taffy's `calc_resolver`.
- Theme resolution for a packaged application with no `node_modules`.
- A Tailwind v3 engine behind the existing interface.
- `group_focus` in GPUI, which would make Tailwind's `group-focus:` resolvable.
- Radial and conic gradients, and gradients with more than two stops. Needs upstream GPUI work.
- Splitting `renderer.rs`. This branch removes the cascade and style application from it. The
  remaining 3,000 lines still hold the napi surface, window setup, the GPUI view, virtual lists,
  element builders, events and batch parsing. That split is its own piece of work.

## Delivery

One branch, `feat/css-values-and-classname`, for the prototype. Split it into sequenced pull
requests once the gates pass.

Land in this order. The first two carry no new dependency and no behaviour change, so they can
merge to `main` on their own even if the rest slips.

1. **The resolved style seam.** `resolve()` returns a `StyleRefinement`, cached on
   `RetainedElement`, invalidated where `set_style` already compares. Delete `apply_styles`.
   Add the `style_resolutions` counter. No new dependency.
2. **The `hideInstance` fix.** Failing test first. It is a real bug today, waiting only for a
   second source of style.
3. **The `gpuix-css` crate.** Split it before writing it. Retrofitting a crate seam is the
   expensive version.
4. **The `StyleDesc` widening.** Done, minus the conditions. `style` carries declarations only,
   so `hover` and `active` gain no siblings and every other condition waits for layer 3.
5. **The cascade.** `cascade.rs`, inheritance, custom properties, media queries.
6. **The `className` seam.**
7. **`@gpuix/tailwind`.**

Add a changeset. Never edit `CHANGELOG.md` by hand. Never publish locally.
