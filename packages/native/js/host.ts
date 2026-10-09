import type {
  DebugFrameOverlayStats,
  EdgeInsets,
  ElementBounds,
  EventPayload,
  GpuixRenderer,
  HighlightMatch,
  PathPromptOptions,
  WindowInsets as NativeWindowInsets,
} from "../index.js"

export type {
  DebugFrameOverlayStats,
  EdgeInsets,
  ElementBounds,
  HighlightMatch,
  NativeWindowInsets,
  PathPromptOptions,
}

export * from "./host-runtime.js"
export * from "./mutations.js"
export * from "./renderer-state.js"

export type DimensionValue = number | string

export interface MotionStyle {
  width?: number
  height?: number
  opacity?: number
  top?: number
  right?: number
  bottom?: number
  left?: number
  borderRadius?: number
}

export type MotionEase =
  | "linear"
  | "ease"
  | "easeIn"
  | "easeOut"
  | "easeInOut"
  | [number, number, number, number]

export interface MotionTweenTransition {
  type?: "tween"
  /** Duration in seconds. */
  duration?: number
  /** Delay in seconds. */
  delay?: number
  ease?: MotionEase
}

/** Critically-underdamped integrator. Defaults are gelatinous, not snappy. */
export interface MotionSpringTransition {
  type: "spring"
  stiffness?: number
  damping?: number
  mass?: number
  velocity?: number
  /** Delay in seconds. */
  delay?: number
}

export type MotionTransition = MotionTweenTransition | MotionSpringTransition

export interface MotionProps {
  initial?: MotionStyle | false
  animate: MotionStyle
  /** Target applied while this node is leaving `AnimatePresence`. */
  exit?: MotionStyle
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
  flexGrow?: number
  flexShrink?: number
  flexBasis?: number
  alignItems?: string
  alignSelf?: string
  alignContent?: string
  justifyContent?: string
  gap?: number
  rowGap?: number
  columnGap?: number
  gridTemplateColumns?: number
  gridTemplateRows?: number
  gridColumnMin?: "zero" | "min-content" | "max-content"
  gridRowMin?: "zero" | "min-content" | "max-content"

  width?: DimensionValue
  height?: DimensionValue
  minWidth?: DimensionValue
  minHeight?: DimensionValue
  maxWidth?: DimensionValue
  maxHeight?: DimensionValue

  padding?: number
  paddingTop?: number
  paddingRight?: number
  paddingBottom?: number
  paddingLeft?: number

  margin?: number
  marginTop?: number
  marginRight?: number
  marginBottom?: number
  marginLeft?: number

  position?: string
  top?: number
  right?: number
  bottom?: number
  left?: number

  background?: string | LinearGradientBackground
  backgroundColor?: string
  color?: string
  opacity?: number

  borderWidth?: number
  borderTopWidth?: number
  borderRightWidth?: number
  borderBottomWidth?: number
  borderLeftWidth?: number
  borderColor?: string
  borderRadius?: number
  borderTopLeftRadius?: number
  borderTopRightRadius?: number
  borderBottomLeftRadius?: number
  borderBottomRightRadius?: number
  boxShadow?: BoxShadow
  /** CSS `outline`: a line outside the border box. Takes no layout space, so it
   *  can appear on focus without moving anything. Width defaults to 1. */
  outlineWidth?: number
  outlineColor?: string
  /** Gap between the border box and the outline. Negative draws it inside. */
  outlineOffset?: number

  fontSize?: number
  fontFamily?: string
  fontWeight?: string | number
  textAlign?: string
  lineHeight?: number
  whiteSpace?: "normal" | "nowrap"
  textOverflow?: "ellipsis" | "ellipsis-start"
  lineClamp?: number
  textDecoration?: "underline" | "line-through" | "none"

  overflow?: string
  overflowX?: string
  overflowY?: string

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

  // Pseudo-selector styles — applied by GPUI natively (no JS round-trip).
  // Nesting is one level deep: a state style cannot contain another.
  hover?: StateStyleDesc
  active?: StateStyleDesc
  /** While focused after keyboard input (Tab), like CSS `:focus-visible`.
   *  A mouse press never shows it. Needs a focusable element. */
  focusVisible?: StateStyleDesc
  /** `false` keeps this focusable element at full opacity while another
   *  control has keyboard focus. Default dims it. Not inherited. */
  keyboardFocusDim?: boolean
}

export type StateStyleDesc = Omit<
  StyleDesc,
  "hover" | "active" | "focusVisible" | "keyboardFocusDim"
>

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
 * heading scale is a framework render and needs no native rebuild.
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

/** One `highlight` entry. See `HostProps.highlight`. */
export interface HighlightSpec {
  /**
   * Substring to match. Case-insensitive unless `caseSensitive` is set.
   *
   * A match never crosses a line, exactly like browser find. It DOES cross the
   * several host nodes a JSX renderer makes for one interpolated line, so
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

// Props passed to elements.
// Element IDs are auto-generated numeric IDs (not user-settable).
// Framework adapters add their own children, key, and ref fields.
export interface HostProps {
  style?: StyleDesc

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
  /** Fires on the focused element, then on ancestors that declare it. */
  onKeyDown?: (event: KeyEvent) => void
  onKeyUp?: (event: KeyEvent) => void

  // ── Focus events ───────────────────────────────────────────────
  onFocus?: (event: EventPayload) => void
  onBlur?: (event: EventPayload) => void

  // ── Scroll events ──────────────────────────────────────────────
  onScroll?: (event: EventPayload) => void

  // ── File drop (Finder / OS paths) ───────────────────────────────
  onFileDrop?: (event: EventPayload) => void

  // ── Text editor events ─────────────────────────────────────────
  onChange?: (event: EventPayload) => void
  /** Enter on `<input>`, or Enter on `<textarea>` when this listener is set. */
  onSubmit?: (event: EventPayload) => void

  // ── Native component events ─────────────────────────────────────
  onToggleFile?: (event: EventPayload) => void
  onShowMore?: (event: EventPayload) => void
  onLineClick?: (event: EventPayload) => void
  onLinkClick?: (event: EventPayload) => void
  onVisibleRange?: (event: EventPayload) => void
  /** Match count changed for this element's `highlight`. See `matchCount`. */
  onHighlight?: (event: EventPayload) => void
  /** A native `motion` track reached its current target. */
  onMotionComplete?: (event: EventPayload) => void

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
  /**
   * AccessKit role, as an ARIA token (`"button"`, `"heading"`).
   * A node is in the accessibility tree only with both an id (always set)
   * and a role. `"none"` / `"presentation"` produce no node.
   */
  role?: string
  /** Accessible name. Maps to GPUI `aria_label`. */
  "aria-label"?: string
  /** Extra description announced after name, role, and value. */
  "aria-description"?: string
  /** Author id exposed as `AXIdentifier` / UIA AutomationId. */
  "aria-id"?: string
  "aria-expanded"?: boolean
  "aria-selected"?: boolean
  /** String value reported to assistive technology. */
  "aria-valuetext"?: string
  /** Heading level, 1-based. */
  "aria-level"?: number
  /** Stable locator id for automation. */
  testId?: string
  /** Internal native animation description used by motion components. */
  motion?: MotionProps
}

// Props for native text editor elements.
export interface InputProps extends HostProps {
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

export type VirtualListShared = {
  /** No `hover` or `active`: gpui's `List` has no pressed or hovered style
   *  state. Put those on a wrapping `<div>` instead. */
  style?: Omit<StyleDesc, "hover" | "active">
  alignment?: "top" | "bottom"
  followTail?: boolean
  overdraw?: number
  onVisibleRange?: (event: EventPayload) => void
  role?: string
  "aria-label"?: string
  "aria-description"?: string
  "aria-id"?: string
  "aria-expanded"?: boolean
  "aria-selected"?: boolean
  "aria-valuetext"?: string
  "aria-level"?: number
  testId?: string
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
export interface ImgProps extends HostProps {
  /** Filesystem path, data URL, or http(s) URL. */
  src?: string
  objectFit?: "fill" | "contain" | "cover" | "scaleDown" | "none"
  alt?: string
}

/** Byte order of a `setImagePixels` buffer. Alpha is straight in both. */
export type ImagePixelFormat = "rgba" | "bgra"

export interface ImagePixelsOptions {
  /** Default `"rgba"`. `"bgra"` is GPUI's native order and skips a per-pixel pass. */
  format?: ImagePixelFormat
}

// Props for monochrome SVGs tinted by style.color.
export interface SvgProps extends HostProps {
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
export interface CodeProps extends HostProps {
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
export interface DiffProps extends HostProps {
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
export interface MarkdownProps extends HostProps {
  /** GitHub-flavoured markdown. Tables, strikethrough and task lists are on. */
  source?: string
  theme?: GpuixTheme
  /** Fires when a block containing links is clicked. `event.value` is the URL. */
  onLinkClick?: (event: EventPayload) => void
}

// Props for the <anchored> custom element.
export interface AnchoredProps extends HostProps {
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
  /** `"window"` covers the whole viewport and tracks resizes natively. Ignores
   *  position, side, align, anchor, offset and fit. */
  fill?: "window"
}

export type MutationHost = Pick<GpuixRenderer, "applyBatch">
export type WindowSizeHost = Pick<GpuixRenderer, "getWindowSize">
export type WindowInsetsHost = Pick<GpuixRenderer, "getWindowInsets">
export type SelectionHost = Pick<GpuixRenderer, "getSelectedText">
/** `applyBatch` plus the live renderer methods a host may also expose. */
export type NativeRenderer = MutationHost &
  Partial<
    Pick<
      GpuixRenderer,
      | "focusElement"
      | "focusNext"
      | "focusPrevious"
      | "getFocusedElementId"
      | "focusNextWithin"
      | "focusPreviousWithin"
      | "blur"
      | "setWindowKeyEvents"
      | "setWindowSelectionChange"
      | "getElementBounds"
      | "scrollTo"
      | "scrollToItem"
      | "scrollIntoView"
      | "setImage"
      | "setImagePixels"
      | "getScrollOffset"
      | "getListScrollTop"
      | "getSelectedText"
      | "clearSelection"
      | "getPaintedHighlights"
      | "getWindowSize"
      | "getWindowInsets"
      | "setWindowTitle"
      | "promptForPaths"
      | "activateWindow"
      | "minimizeWindow"
      | "zoomWindow"
      | "toggleFullscreen"
      | "setDebugFrameOverlay"
      | "getDebugFrameOverlay"
      | "cycleDebugFrameOverlay"
      | "resetDebugFrameOverlayStats"
      | "getDebugFrameOverlayStats"
    >
  >

/** Commit-phase mutation facade shared by framework adapters. */
export interface MutationRenderer {
  createElement(id: number, elementType: string): void
  destroyElement(id: number): Array<number>
  appendChild(parentId: number, childId: number): void
  insertBefore(parentId: number, childId: number, beforeId: number): void
  setStyle(id: number, style: object): void
  setText(id: number, content: string): void
  setEventListener(id: number, eventType: string, hasHandler: boolean): void
  setRoot(id: number): void
  setKeyboardFocusDim(enabled: boolean): void
  setCustomProp(id: number, key: string, value: object | string | number | boolean | null): void
  flushMutations(): void
}

export type DebugFrameOverlayMode = "hidden" | "minimal" | "full"

/** Method syntax makes the parameter bivariant, so an `onKeyDown` typed with
 *  `KeyEvent` can share the map. Dispatch builds the `KeyEvent` for key types. */
export type HostEventHandler = { handle(event: EventPayload): void }["handle"]

export type EventHandlerMap = Map<number, Map<string, HostEventHandler>>

/**
 * A `keyDown` / `keyUp` event. Element handlers and the window handler of one
 * keystroke share the flags, like one DOM event bubbling to `window`.
 */
export interface KeyEvent extends EventPayload {
  /** True after any handler of this keystroke called `preventDefault()`. */
  readonly defaultPrevented: boolean
  /** Cancel the default action, for example Tab moving focus. */
  preventDefault(): void
  /** Skip ancestor `onKeyDown` handlers and the window handler. The default
   *  action still runs, like the DOM. */
  stopPropagation(): void
}

export type WindowKeyEventHandler = (
  event: KeyEvent,
  renderer: NativeRenderer
) => void

export type WindowEventHandler = (
  event: EventPayload,
  renderer: NativeRenderer
) => void

export interface WindowKeyEventHandlers {
  /** Window-level GPUI listener. Runs after element `onKeyDown` handlers.
   *  Key actions can consume an event before this runs. */
  onKeyDown?: WindowKeyEventHandler
  /** Window-level GPUI listener. */
  onKeyUp?: WindowKeyEventHandler
  /** Window-level text selection. Fires when the selected ranges change. */
  onSelectionChange?: WindowEventHandler
  /**
   * Tab and Shift+Tab move focus through `tabIndex` elements. Defaults to
   * true. Call `event.preventDefault()` in any `onKeyDown` to keep one Tab.
   */
  tabNavigation?: boolean
  /**
   * While a control has keyboard focus, every other focusable element dims.
   * Defaults to true. Set false to draw your own focus look with
   * `focusVisible`. Per element, use `style.keyboardFocusDim: false`.
   */
  keyboardFocusDim?: boolean
}

export interface RootEventHandlers extends WindowKeyEventHandlers {
  onEvent?: (event: EventPayload) => void
  onUncaughtError?: (
    error: Error,
    errorInfo: { componentStack?: string }
  ) => void
}

export interface ElementIdAllocator {
  nextElementId: number
}
