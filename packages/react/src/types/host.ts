import type { EventPayload } from "@gpuix/native"

/**
 * A value `width`, `height` and the four min and max forms take.
 *
 * A bare number is pixels. A string is read by the same length parser every
 * other length property uses, so `"6rem"`, `"1in"`, `"calc(100px + 2rem)"` and
 * `"var(--size)"` all work. On top of those it takes a percentage of the
 * parent, such as `"50%"`, and `"auto"` for the size the content takes.
 *
 * A value the parser cannot read drops the one declaration and leaves the rest
 * of the style alone, the way a browser drops a declaration it cannot parse.
 */
export type DimensionValue = number | string

export interface MotionStyle {
  width?: number
  /**
   * A length in pixels, or `"auto"` for the height the content takes.
   *
   * `"auto"` is measured at the width the element really gets, whether that
   * comes from a declared length, from `flex`, from a percentage or from a
   * stretched cross axis, so text wraps the way it will on screen. The
   * measurement repeats every frame, so the animation follows content that
   * changes while it runs.
   */
  height?: number | "auto"
  opacity?: Numeric
  top?: Numeric
  right?: Numeric
  bottom?: Numeric
  left?: Numeric
  borderRadius?: Numeric
  /**
   * A corner shape keyword, `superellipse(K)` text, or the number `K` itself.
   * Interpolates in the half-corner space CSS Borders 4 names, so
   * `round` to `square` moves at an even pace.
   */
  cornerShape?: number | string
}

export type MotionEase =
  | "linear"
  | "ease"
  | "easeIn"
  | "easeOut"
  | "easeInOut"
  | [number, number, number, number]

export interface MotionTransition {
  /** Duration in seconds. */
  duration?: number
  /** Delay in seconds. */
  delay?: number
  ease?: MotionEase
}

export interface MotionProps {
  initial?: MotionStyle | false
  animate: MotionStyle
  transition?: MotionTransition
}

/**
 * CSS `cursor` keywords GPUI can paint. An unlisted keyword is ignored, like
 * every other invalid style value.
 */
export type CursorValue =
  | "default"
  | "auto"
  | "pointer"
  | "text"
  | "vertical-text"
  | "crosshair"
  | "grab"
  | "grabbing"
  | "move"
  | "all-scroll"
  | "col-resize"
  | "row-resize"
  | "ew-resize"
  | "ns-resize"
  | "nwse-resize"
  | "nesw-resize"
  | "n-resize"
  | "e-resize"
  | "s-resize"
  | "w-resize"
  | "ne-resize"
  | "nw-resize"
  | "se-resize"
  | "sw-resize"
  | "not-allowed"
  | "no-drop"
  | "alias"
  | "copy"
  | "context-menu"

export interface BoxShadow {
  offsetX: number
  offsetY: number
  blurRadius: number
  spreadRadius: number
  color: string
}

/**
 * A style value that resolves to a number.
 *
 * A bare number is pixels, which is what the `style` prop has always taken.
 * A string is there for `var()` and for writing the unit, so `8`, `"8px"` and
 * `"var(--pad)"` all mean the same padding. Any other unit drops the
 * declaration rather than paint the wrong size, so `"2rem"` does nothing.
 */
export type Numeric = number | string

export interface LinearGradientStop {
  color: string
  /** Position along the gradient from 0 to 1. */
  position: number
}

export interface LinearGradientBackground {
  type: "linear-gradient"
  /** CSS angle in degrees. 0 points up and values increase clockwise. */
  angle: number
  stops: [LinearGradientStop, LinearGradientStop]
  colorSpace?: "srgb" | "oklab"
}

export interface StyleDesc {
  display?: string
  visibility?: string
  flexDirection?: string
  flexWrap?: string
  flexGrow?: Numeric
  flexShrink?: Numeric
  flexBasis?: Numeric
  alignItems?: string
  alignSelf?: string
  alignContent?: string
  justifyContent?: string
  gap?: Numeric
  rowGap?: Numeric
  columnGap?: Numeric
  gridTemplateColumns?: Numeric
  gridTemplateRows?: Numeric
  gridColumnMin?: "zero" | "min-content" | "max-content"
  gridRowMin?: "zero" | "min-content" | "max-content"

  width?: DimensionValue
  height?: DimensionValue
  minWidth?: DimensionValue
  minHeight?: DimensionValue
  maxWidth?: DimensionValue
  maxHeight?: DimensionValue

  padding?: Numeric
  paddingTop?: Numeric
  paddingRight?: Numeric
  paddingBottom?: Numeric
  paddingLeft?: Numeric

  margin?: Numeric
  marginTop?: Numeric
  marginRight?: Numeric
  marginBottom?: Numeric
  marginLeft?: Numeric

  position?: string
  top?: number
  right?: number
  bottom?: number
  left?: number

  /** A colour or a `linear-gradient()`. The shorthand, so both longhands
   *  win over it. */
  background?: string | LinearGradientBackground
  backgroundColor?: string
  /** A `linear-gradient()` or `none`, painted over `backgroundColor` the
   *  way a browser paints it. Stop positions are percentages.
   *
   *  An easing function between two stops bends the mix, which CSS cannot
   *  do yet: `linear-gradient(to top, black, ease-in-out, transparent)`.
   *  Takes `ease`, `ease-in`, `ease-out`, `ease-in-out` or `cubic-bezier()`,
   *  following csswg-drafts issue 1332. A straight mix looks dense near the
   *  solid stop and thin near the clear one, and an eased scrim does not. */
  backgroundImage?: string
  /** How `backgroundImage` mixes with `backgroundColor`: any CSS
   *  `<blend-mode>` except `plus-darker`. */
  backgroundBlendMode?: string
  /** How the element and its children mix with what is under them: any CSS
   *  `<blend-mode>` except `plus-darker`. */
  mixBlendMode?: string
  /** A CSS filter list on the element and its children, or `none`. `blur()`,
   *  `brightness()`, `contrast()`, `grayscale()`, `hue-rotate()`, `invert()`,
   *  `opacity()`, `saturate()` and `sepia()` are painted. `drop-shadow()`
   *  and `url()` drop the whole property. The functions other than `blur()`
   *  fold into one colour matrix, so a long list costs one pass. */
  filter?: string
  /** The same list applied to what is under the element, clipped to its
   *  corners like CSS `backdrop-filter`. */
  backdropFilter?: string
  /** A `linear-gradient()` whose alpha keeps or drops each pixel of the
   *  element and its children, over the border box. Over a `backdropFilter`
   *  blur the alpha scales the blur radius instead, unlike CSS, which fades
   *  a sharp copy over the blur. Add an easing between the stops for a
   *  progressive blur under a header. */
  maskImage?: string
  color?: string
  opacity?: number

  borderWidth?: Numeric
  borderTopWidth?: Numeric
  borderRightWidth?: Numeric
  borderBottomWidth?: Numeric
  borderLeftWidth?: Numeric
  borderColor?: string
  borderRadius?: number
  borderTopLeftRadius?: Numeric
  borderTopRightRadius?: Numeric
  borderBottomLeftRadius?: Numeric
  borderBottomRightRadius?: Numeric
  borderStartStartRadius?: Numeric
  borderStartEndRadius?: Numeric
  borderEndStartRadius?: Numeric
  borderEndEndRadius?: Numeric

  /**
   * CSS Borders 4 `corner-shape`, one to four of `round`, `squircle`,
   * `square`, `bevel`, `scoop`, `notch` or `superellipse(K)`, read
   * top-left, top-right, bottom-right, bottom-left like `borderRadius`.
   * The shape only shows where the corner has a radius. A value the spec
   * rejects drops the whole property. Logical names assume `horizontal-tb`
   * and `ltr`.
   */
  cornerShape?: string
  cornerTopLeftShape?: string
  cornerTopRightShape?: string
  cornerBottomRightShape?: string
  cornerBottomLeftShape?: string
  cornerStartStartShape?: string
  cornerStartEndShape?: string
  cornerEndStartShape?: string
  cornerEndEndShape?: string
  /** Two shapes, in the order the side runs: left to right, or top to bottom. */
  cornerTopShape?: string
  cornerRightShape?: string
  cornerBottomShape?: string
  cornerLeftShape?: string
  cornerBlockStartShape?: string
  cornerBlockEndShape?: string
  cornerInlineStartShape?: string
  cornerInlineEndShape?: string
  /**
   * Radius and shape together, in either order: `"8px squircle"`. A part
   * you leave out resets, so `corner: "bevel"` also sets the radius to 0.
   * `/` (elliptical radii) is not supported and makes the value invalid.
   * A narrower property wins over a wider one, and a single-purpose one
   * over a shorthand: `cornerTopLeftShape` beats `cornerTopLeft`, which
   * beats `cornerTop`, which beats `cornerShape` and `borderRadius`, which
   * beat `corner`.
   */
  corner?: string
  cornerTopLeft?: string
  cornerTopRight?: string
  cornerBottomRight?: string
  cornerBottomLeft?: string
  cornerStartStart?: string
  cornerStartEnd?: string
  cornerEndStart?: string
  cornerEndEnd?: string
  cornerTop?: string
  cornerRight?: string
  cornerBottom?: string
  cornerLeft?: string
  cornerBlockStart?: string
  cornerBlockEnd?: string
  cornerInlineStart?: string
  cornerInlineEnd?: string
  boxShadow?: BoxShadow

  fontSize?: Numeric
  fontFamily?: string
  fontWeight?: string | number
  textAlign?: string
  lineHeight?: Numeric
  whiteSpace?: "normal" | "nowrap"
  textOverflow?: "ellipsis" | "ellipsis-start"
  lineClamp?: Numeric

  /** `visible`, `hidden`, `clip`, `scroll` or `auto`. `scroll` and `auto`
   *  make a scroll box with a scrollbar. The OS picks the kind of bar: an
   *  overlay bar that fades out after a scroll, or a classic bar in a
   *  gutter. A classic bar shows at all times for `scroll` and only while
   *  the content overflows for `auto`. */
  overflow?: string
  overflowX?: string
  overflowY?: string
  /** `auto`, `thin` or `none`. `none` paints no bar and reserves no gutter. */
  scrollbarWidth?: string
  /** `auto`, or the thumb colour then the track colour, as in CSS. */
  scrollbarColor?: string
  /** `auto`, `stable` or `stable both-edges`. `stable` reserves the gutter
   *  of a classic bar even while the content fits, and `both-edges` adds
   *  the same gutter at the start of the axis. Overlay bars reserve
   *  nothing, as in CSS. */
  scrollbarGutter?: string
  /** Space scrollIntoView keeps around this element, a number of pixels
   *  or "Npx", alone or as the CSS one-to-four shorthand. */
  scrollMargin?: Numeric
  scrollMarginTop?: Numeric
  scrollMarginRight?: Numeric
  scrollMarginBottom?: Numeric
  scrollMarginLeft?: Numeric
  /** Space scrollIntoView keeps inside this scroll box. */
  scrollPadding?: Numeric
  scrollPaddingTop?: Numeric
  scrollPaddingRight?: Numeric
  scrollPaddingBottom?: Numeric
  scrollPaddingLeft?: Numeric
  /** `auto`, `contain` or `none`, one word for both axes or two with the x
   *  axis first. A scroll box keeps a wheel event it can scroll with. At its
   *  end, `auto` hands the event to the nearest scroll box around it and
   *  `contain` or `none` keeps it. */
  overscrollBehavior?: string
  overscrollBehaviorX?: string
  overscrollBehaviorY?: string

  cursor?: CursorValue
  /** `"auto"` blocks hits behind this element **and its wheel**. `"none"` never
   *  blocks. Unset blocks clicks when the element paints a fill or is
   *  positioned, but lets the wheel reach the ancestor scroller, like HTML. */
  pointerEvents?: "auto" | "none"

  /** "none" opts this element and its subtree out of text selection.
   *  Inherited like the CSS property, so a toolbar can disable it once. */
  userSelect?: "text" | "none" | "auto"
  /** Selection wash colour for this subtree. Defaults to the theme accent at 35%. */
  selectionColor?: string

  // Pseudo-selector styles, applied by GPUI natively (no JS round-trip).
  // Nesting is one level deep: hover/active cannot contain hover/active.
  //
  // These two named fields are here for history. A CSS `style` attribute
  // holds declarations, not selectors, so the style prop gets no further
  // condition. A class resolver sends every other condition through
  // `selectors` below.
  hover?: StyleDeclarations
  active?: StyleDeclarations

  // Conditioned blocks from a class resolver. `on` takes a canonical
  // selector spelling from the closed set the engine reads:
  // `:first-child`, `:last-child`, `:nth-child(odd)`, `:nth-child(even)`,
  // `:only-child` for the element's own position, and `& > *`,
  // `& > :not(:last-child)`, `& *` for rules on its children. Anything
  // else warns once in the engine and drops.
  selectors?: SelectorRule[]

  // Custom properties. A declaration here is in scope for `var()` on this
  // element and on everything below it, the same as in CSS.
  //
  // A number declares its own plain text, so `{ "--pad": 8 }` declares `8`.
  // The name needs both dashes: `"-pad"` is a type error rather than a
  // variable that silently never resolves.
  [name: `--${string}`]: string | number | undefined
}

/**
 * What `hover`, `active` and a selector rule may hold.
 *
 * No nesting, and no custom properties. A declaration inside a state has
 * nothing to apply to, because the cascade reads variables from the element
 * itself, not from one of its states.
 */
export type StyleDeclarations = Omit<
  StyleDesc,
  "hover" | "active" | "selectors" | `--${string}`
>

/** One conditioned block from a class resolver. */
export interface SelectorRule {
  on: string
  style: StyleDeclarations
}

// Element types supported by GPUIX
export type ElementType =
  | "div"
  | "text"
  | "img"
  | "svg"
  | "canvas"
  | "input"
  | "textarea"
  | "anchored"
  | "code"
  | "diff"
  | "markdown"
  | "virtual-list"

// ── Theme ────────────────────────────────────────────────────────────

/** Colours for one syntax capture class each. Every field is a CSS colour. */
export interface SyntaxTheme {
  comment?: string
  keyword?: string
  string?: string
  stringSpecial?: string
  escape?: string
  number?: string
  boolean?: string
  typeName?: string
  typeBuiltin?: string
  constructor?: string
  function?: string
  functionBuiltin?: string
  macroName?: string
  property?: string
  constant?: string
  variable?: string
  variableSpecial?: string
  parameter?: string
  operator?: string
  punctuation?: string
  tag?: string
  attribute?: string
  label?: string
  invalid?: string
}

/**
 * Every number that decides layout in the native text components.
 *
 * These live in the theme, not in Rust constants, so tuning a row height or a
 * heading scale is a React re-render and needs no native rebuild.
 */
export interface GpuixMetrics {
  // Code blocks. Shared by <code> and the markdown fenced block.
  codeTextSize?: number
  codeLineHeight?: number
  codeGutterDigitWidth?: number
  codeGutterPaddingRight?: number
  codeGutterMinWidth?: number

  // Diffs
  diffTextSize?: number
  diffLineHeight?: number
  diffFileHeaderHeight?: number
  diffHunkHeaderHeight?: number
  diffNoticeHeight?: number
  diffBodyBottomPad?: number
  diffGutterWidth?: number
  diffMarkerWidth?: number
  diffAccentBarWidth?: number
  diffRowPaddingX?: number

  // Markdown
  mdTextSize?: number
  mdLineHeight?: number
  mdBlockGap?: number
  /** `[h1, h2, h3, h4to6]`. A shorter array leaves the rest at their defaults. */
  mdHeadingSizes?: number[]
  mdHeadingLineHeights?: number[]
  mdTableCellPadding?: number
  mdTableMinColumnWidth?: number
  mdTableMinColumnContent?: number
  mdInlineCodeRadius?: number
  /**
   * The fenced-block card. `<code>` paints no card, so these are
   * markdown-only: style a `<code>` block with its own `style` prop instead.
   */
  mdCodePaddingX?: number
  mdCodePaddingY?: number
  mdCodeRadius?: number
  mdCodeHeaderPaddingY?: number
  mdCodeHeaderTextSize?: number
}

/**
 * Theme tokens for the native text components. Every field is optional and
 * layers on top of the built-in dark theme (or light, via `appearance`).
 */
export interface GpuixTheme {
  appearance?: "dark" | "light"
  bg?: string
  border?: string
  text?: string
  textMuted?: string
  textFaint?: string
  textDim?: string
  accent?: string
  caret?: string
  codeText?: string
  codeWash?: string
  diffAdd?: string
  diffDel?: string
  diffHunkBg?: string
  fontSans?: string
  fontMono?: string
  syntax?: SyntaxTheme
  metrics?: GpuixMetrics
}

/** One `highlight` entry. See `Props.highlight`. */
export interface HighlightSpec {
  /**
   * Substring to match. Case-insensitive unless `caseSensitive` is set.
   *
   * A match never crosses a line, exactly like browser find. It DOES cross the
   * several host nodes React makes for one interpolated line, so
   * `<text>Hello {name}!</text>` matches `Hello Tommy`.
   */
  query?: string
  caseSensitive?: boolean
  /** Only match when neither neighbour is alphanumeric or `_`. */
  wholeWord?: boolean
  /**
   * Explicit `[start, end)` pairs in UTF-16 code units, the units `indexOf` and
   * `RegExp.exec` return. They index the declaring subtree's text, with a
   * newline between lines.
   *
   * A pair that splits a surrogate pair is rejected, not snapped. Native text
   * (`<code>`, `<markdown>`, `<diff>`) is not part of that text; use `query`.
   */
  ranges?: Array<[number, number]>
  /** Any CSS colour. Defaults to the theme accent at 30% alpha. */
  color?: string
  /** Colour for the match at `activeIndex`. Defaults to accent at 65%. */
  activeColor?: string
  /** Index of the match to highlight differently, for a find-bar cursor. */
  activeIndex?: number
  /**
   * How many MATCHES come before this subtree in your document, so `activeIndex`
   * is compared against `matchIndexOffset + n` for the nth match here.
   *
   * It is a match count, not a row index. Rows hold different numbers of
   * matches, so a row index cannot stand in for it.
   *
   * Only needed for virtualized content: a `<virtual-list>` mounts a window of
   * its rows, so native can only number what that window contains. Sum
   * `findRanges` over the rows before `windowStart`. Defaults to 0.
   *
   * A negative or fractional value is refused and the whole spec is dropped,
   * because a bad offset silently marks the wrong match.
   */
  matchIndexOffset?: number
  /** Corner radius of the wash. Defaults to 2. */
  radius?: number
}

/** One highlight wash painted in the last frame. Test-facing. */
export interface HighlightMatch {
  elementId: number
  /** Index of the run within that element. 0 for a plain `<text>`. */
  sub: number
  /** The run's full string, so `text.slice(start, end)` is the match. */
  text: string
  start: number
  end: number
  active: boolean
  /** One box per visual row, so a soft-wrapped match has two. */
  rects: Array<{ x: number; y: number; width: number; height: number }>
}

// Props passed to elements.
// Element IDs are auto-generated numeric IDs (not user-settable).
// Use React refs to get an element's ID: ref.current.id
export interface Props {
  // `key` must live here, not in `JSX.IntrinsicAttributes`. TypeScript 5 ignores
  // that member for intrinsic elements, and React's DOM types work only because
  // `DetailedHTMLProps` already carries `key`. Without this field every
  // `<div key={...} />` inside a `.map()` fails to typecheck.
  key?: React.Key | null
  // A style attribute holds declarations, not selectors, so the prop cannot
  // carry `selectors`. A class resolver is the only writer of that field.
  style?: Omit<StyleDesc, "selectors">
  /**
   * Class tokens, separated by spaces, read by the root's resolver.
   *
   * `string | undefined` is the whole type, so `clsx` and `cn` need no special
   * handling. Without a resolver on the root this does nothing and warns once.
   * A declaration in `style` beats one from a class in every state.
   */
  className?: string
  children?: React.ReactNode
  ref?: React.Ref<PublicInstance>

  // ── Mouse events ───────────────────────────────────────────────
  /** Primary button only, like the DOM. Use `onAuxClick` for the others. */
  onClick?: (event: EventPayload) => void
  /** Non-primary click, like the DOM `auxclick`. `isRightClick` says which. */
  onAuxClick?: (event: EventPayload) => void
  onMouseDown?: (event: EventPayload) => void
  onMouseUp?: (event: EventPayload) => void
  onMouseEnter?: (event: EventPayload) => void
  onMouseLeave?: (event: EventPayload) => void
  onMouseMove?: (event: EventPayload) => void
  /** Fires when user clicks OUTSIDE this element. Use for "click outside to close". */
  onMouseDownOutside?: (event: EventPayload) => void

  // ── Keyboard events (need focus: autoFocus, or a click on the element) ──
  onKeyDown?: (event: EventPayload) => void
  onKeyUp?: (event: EventPayload) => void

  // ── Focus events ───────────────────────────────────────────────
  onFocus?: (event: EventPayload) => void
  onBlur?: (event: EventPayload) => void

  // ── Scroll events ──────────────────────────────────────────────
  onScroll?: (event: EventPayload) => void

  // ── Text editor events ─────────────────────────────────────────
  onChange?: (event: EventPayload) => void
  onSubmit?: (event: EventPayload) => void

  // ── Native component events ─────────────────────────────────────
  onToggleFile?: (event: EventPayload) => void
  onShowMore?: (event: EventPayload) => void
  onLineClick?: (event: EventPayload) => void
  onLinkClick?: (event: EventPayload) => void
  onVisibleRange?: (event: EventPayload) => void
  /** Match count changed for this element's `highlight`. See `matchCount`. */
  onHighlight?: (event: EventPayload) => void

  // ── Highlight ──────────────────────────────────────────────────
  /**
   * Paint a background wash behind matched or explicitly given text ranges.
   *
   * Scoped by position: on the root it searches the window, on a container it
   * searches that container. The nearest declaration wins, so a nested
   * `highlight` replaces an ancestor's for its own subtree.
   */
  highlight?: HighlightSpec | HighlightSpec[] | null

  // ── Focus props ────────────────────────────────────────────────
  /** Take keyboard focus when the element first mounts. Required for `<input>`:
   *  without it, or a click, the field never receives key events. */
  autoFocus?: boolean
  /** Native GPUI tab order. Use 0 for normal keyboard focus. */
  tabIndex?: number
  /** Stable locator id for automation. */
  testId?: string
  /** Internal native animation description used by motion components. */
  motion?: MotionProps
}

// Props for native text editor elements.
export interface InputProps extends Props {
  /** External editor value. Native edits apply immediately and report through onChange. */
  value?: string
  placeholder?: string
  readOnly?: boolean
  theme?: GpuixTheme
}

export interface TextareaProps extends InputProps {
  minRows?: number
  maxRows?: number
}

type VirtualListShared = {
  // See the note on `Props.key`.
  key?: React.Key | null
  /** No `hover` or `active`: gpui's `List` has no interactive element identity,
   *  so it cannot hold the pressed or hovered state those styles read. Put them
   *  on a wrapping `<div>` instead. */
  style?: Omit<StyleDesc, "selectors" | "hover" | "active">
  children?: React.ReactNode
  ref?: React.Ref<PublicInstance>
  alignment?: "top" | "bottom"
  followTail?: boolean
  overdraw?: number
  onVisibleRange?: (event: EventPayload) => void
}

/** A variable-height list that builds only rows near its viewport. */
export type VirtualListProps =
  | (VirtualListShared & {
      estimatedItemHeight?: number
      itemCount?: never
      windowStart?: never
    })
  | (VirtualListShared & {
      itemCount: number
      estimatedItemHeight: number
      windowStart?: number
    })

// Props for native <img> rendering.
export interface ImgProps extends Props {
  src?: string
  objectFit?: "fill" | "contain" | "cover" | "scaleDown" | "none"
  alt?: string
}

// Props for monochrome SVGs tinted by style.color.
export interface SvgProps extends Props {
  /** Desktop local path. Use source for portable browser rendering. */
  src?: string
  /** Raw SVG markup rendered directly by GPUI. */
  source?: string
}

/**
 * Props for the <code> custom element — a syntax-highlighted code block.
 *
 * It paints **no surface of its own**: no fill, border, radius, padding or
 * language header. `style` is the surface, and `fontFamily`, `fontSize`,
 * `fontWeight`, `lineHeight` and `color` there beat the theme. Wrap it, or
 * style it, to get a card.
 *
 * Rows are a fixed height, so `fontSize` alone scales that height by the
 * theme's ratio. Lines never wrap and the block is its own horizontal
 * scroller, so `whiteSpace` and `overflowX` do nothing.
 */
export interface CodeProps extends Props {
  /** The source to display. Rendered one div per line at an exact line height. */
  code?: string
  /** Language alias such as "ts", "rust", "bash". Beats `path` for detection. */
  language?: string
  /** File path, used for extension-based language detection. */
  path?: string
  showLineNumbers?: boolean
  theme?: GpuixTheme
}

// Props for the <diff> custom element — a unified diff viewer.
export interface DiffProps extends Props {
  /** A unified git patch (the output of `git diff`). */
  patch?: string
  /** Highlight the words that changed inside paired +/- lines. */
  wordDiff?: boolean
  /** File paths rendered as a header only. Collapsed bodies cost one row. */
  collapsedPaths?: string[]
  /**
    * Use the virtualized `list()` scroller. Off by default so a parent
    * list can be the only scroll container. Requires a bounded height.
   */
  scroll?: boolean
  /** Paint this many line rows, then a Show more row. */
  maxLines?: number
  theme?: GpuixTheme
  /** Fires when a file header is clicked. `event.value` is the file path. */
  onToggleFile?: (event: EventPayload) => void
  /** Fires when Show more is clicked. `event.value` is the hidden line count. */
  onShowMore?: (event: EventPayload) => void
  /** Fires when a diff line is clicked. `event.value` is the line text,
   *  `event.oldLine` / `event.newLine` are its line numbers. */
  onLineClick?: (event: EventPayload) => void
}

// Props for the <markdown> custom element.
export interface MarkdownProps extends Props {
  /** GitHub-flavoured markdown. Tables, strikethrough and task lists are on. */
  source?: string
  theme?: GpuixTheme
  /** Fires when a block containing links is clicked. `event.value` is the URL. */
  onLinkClick?: (event: EventPayload) => void
}

// Props for the <anchored> custom element.
export interface AnchoredProps extends Props {
  position?: { x: number; y: number }
  side?: "top" | "right" | "bottom" | "left"
  align?: "start" | "center" | "end"
  gap?: number
  anchor?:
    | "topLeft"
    | "topCenter"
    | "topRight"
    | "rightCenter"
    | "bottomRight"
    | "bottomCenter"
    | "bottomLeft"
    | "leftCenter"
  offset?: { x: number; y: number }
  fit?: "switch" | "snap"
  snapMargin?: number
  deferred?: boolean
  priority?: number
  occlude?: boolean
}

/// Native renderer transport. React sends one atomic batch per commit.
export interface NativeRenderer {
  /** Apply one React commit. Returns every element id destroyed by the batch. */
  applyBatch(json: string): Array<number>

  // ── Focus API ──────────────────────────────────────────────────
  focusElement?(elementId: number): void
  focusNext?(): void
  focusPrevious?(): void
  blur?(): void
  setWindowKeyEvents?(keyDown: boolean, keyUp: boolean, eventId: number): void

  // ── Scroll API ─────────────────────────────────────────────────
  /** Set the scroll offset of a scrollable element (overflow: "scroll").
   *  x and y are negative pixel values (scroll down = more negative y). */
  scrollTo?(elementId: number, x: number, y: number): void
  /** Scroll a child into view by its index in the children list.
   *  `offsetInItem` is in pixels; a negative value anchors the viewport top
   *  above the item, resolved against measured row heights at layout time. */
  scrollToItem?(elementId: number, index: number, offsetInItem?: number): void
  /** Scroll every ancestor scroll box so the element shows, like the web
   *  scrollIntoView. block places it on the y axis and inline on the x
   *  axis: "start", "center", "end" or "nearest". The defaults match the
   *  web: "start" and "nearest". */
  scrollIntoView?(elementId: number, block?: string, inline?: string): void
  /** Get the current scroll offset [x, y] or null if element is not scrollable. */
  getScrollOffset?(elementId: number): Array<number> | null
  /** The logical scroll anchor of a `<virtual-list>`:
   *  `[itemIndex, offsetInItemPx, viewportHeightPx]`, or null for anything
   *  else. `itemIndex == item count` is gpui's at-end sentinel. */
  getListScrollTop?(elementId: number): Array<number> | null

  // ── Selection API ──────────────────────────────────────────────
  /** The current text selection joined in document order, or null. */
  getSelectedText?(): string | null
  /** Drop the current selection. */
  clearSelection?(): void

  // ── Highlight API ──────────────────────────────────────────────
  /** Every highlight wash painted in the last frame, in paint order.
   *  A quad never appears in getPaintedText(), so this is how `highlight`
   *  is asserted without a screenshot. */
  getPaintedHighlights?(): HighlightMatch[]

  // ── Window API ─────────────────────────────────────────────────
  getWindowSize?(): { width: number; height: number }
  getWindowInsets?(): NativeWindowInsets
  setWindowTitle?(title: string): void
  /** Bring the window forward and focus it. Reveals a `show: false` window. */
  activateWindow?(): void
  setDebugFrameOverlay?(mode: DebugFrameOverlayMode): string
  getDebugFrameOverlay?(): string
  cycleDebugFrameOverlay?(): string
  resetDebugFrameOverlayStats?(): void
  getDebugFrameOverlayStats?(): DebugFrameOverlayStats
}

/** Commit-phase facade used only by the React host config. */
export interface MutationRenderer {
  createElement(id: number, elementType: string): void
  destroyElement(id: number): Array<number>
  appendChild(parentId: number, childId: number): void
  insertBefore(parentId: number, childId: number, beforeId: number): void
  setStyle(id: number, style: object): void
  setText(id: number, content: string): void
  setEventListener(id: number, eventType: string, hasHandler: boolean): void
  setRoot(id: number): void
  setCustomProp(id: number, key: string, value: object | string | number | boolean | null): void
  flushMutations(): void
}

export type DebugFrameOverlayMode = "hidden" | "minimal" | "full"

export interface EdgeInsets {
  top: number
  right: number
  bottom: number
  left: number
}

export interface NativeWindowInsets {
  safeArea: EdgeInsets
  ime: EdgeInsets
  effective: EdgeInsets
}

export interface DebugFrameOverlayStats {
  currentMs?: number
  p90Ms?: number
  p99Ms?: number
  maxMs?: number
  frames: number
  samples: number
}

export type EventHandlerMap = Map<
  number,
  Map<string, (event: EventPayload) => void>
>

export type WindowKeyEventHandler = (
  event: EventPayload,
  renderer: NativeRenderer
) => void

export interface WindowKeyEventHandlers {
  /** Window-level GPUI listener. Key actions can consume an event before this runs. */
  onKeyDown?: WindowKeyEventHandler
  /** Window-level GPUI listener. */
  onKeyUp?: WindowKeyEventHandler
}

export interface RootEventHandlers extends WindowKeyEventHandlers {
  onEvent?: (event: EventPayload) => void
}

export interface ElementIdAllocator {
  nextElementId: number
}

// One React root. Event handlers stay on this object so two live roots
// can both use id 1. Ids come from an allocator that lives with the
// NativeRenderer, so a remount on the same renderer cannot reuse them.
export interface Container {
  renderer: MutationRenderer
  ids: ElementIdAllocator
  eventHandlers: EventHandlerMap
  windowKeyEventHandlers: WindowKeyEventHandlers
  windowKeyEventId: number
  onEvent?: (event: EventPayload) => void
  /** How this root reads `className`, or `null` when nothing registered one. */
  classNames: ClassNameCache | null
  /** Whether this root has already warned that it has no resolver. */
  warnedAboutClassName: boolean
}

/**
 * Reads one class token, such as `p-4`, into the style it declares.
 *
 * Returns `null` for a token it does not know. The token never holds a space,
 * because the root splits the string before it calls this.
 */
export type ClassNameResolver = (token: string) => StyleDesc | null

/** A resolver with what a root has already asked it. */
export interface ClassNameCache {
  resolve: ClassNameResolver
  /** One entry per token, holding `null` for a token the resolver rejected. */
  tokens: Map<string, StyleDesc | null>
  /** Whole strings, bounded, least recently used out first. */
  strings: Map<string, StyleDesc>
}

/** Options for a root. */
export interface RootOptions extends RootEventHandlers {
  /**
   * How to read `className` on this root's elements.
   *
   * This is an option on the root rather than a global, so two roots can hold
   * different resolvers and two test files can run at once.
   */
  resolveClassName?: ClassNameResolver
}

// Instance — minimal handle for React's reconciler.
// The real element state lives in Rust's RetainedTree.
export interface Instance {
  id: number
  type: ElementType
  props: Props
}

// Text instance for raw text nodes
export interface TextInstance {
  id: number
  text: string
  parentId: number | null
}

// Public instance exposed via refs
export type PublicInstance = Instance

// Host context passed down the tree
export interface HostContext {
  isInsideText: boolean
}
