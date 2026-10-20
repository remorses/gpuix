import { pushDismissLayer } from "@gpuix/native/host"
import type { DismissLayer, KeyEvent, StyleDesc } from "@gpuix/native/host"
import { atom, context, createContext, h, watch } from "ilha"
import type { View } from "ilha"

import { commitAll } from "../host.js"
import { useGpuixRequired } from "../root.js"
import type { DivProps, Handler } from "../types.js"

export type FloatingSide = "top" | "right" | "bottom" | "left"
export type FloatingAlign = "start" | "center" | "end"
export type StateStyle<State> = StyleDesc | ((state: State) => StyleDesc)

type VNode = ReturnType<typeof h>

export interface FloatingContentProps extends Omit<DivProps, "style"> {
  style?: StyleDesc
  side?: FloatingSide
  sideOffset?: number
  align?: FloatingAlign
  alignOffset?: number
  collisionPadding?: number
}

/**
 * ilha's `h()` types props as DOM attribute values. GPUIX props carry objects
 * and handlers typed against GPUIX events, so the parts build vnodes through
 * this wider signature.
 */
export const el: (
  type: string | ((props: never) => unknown),
  props: object | null,
  ...children: View[]
) => VNode = h as never

export function resolveStyle<State>(
  style: StateStyle<State> | undefined,
  state: State
): StyleDesc | undefined {
  return typeof style === "function" ? style(state) : style
}

export function mergeStyles(
  base: StyleDesc | undefined,
  override: StyleDesc | undefined
): StyleDesc | undefined {
  if (!base) {
    return override
  }
  if (!override) {
    return base
  }
  return { ...base, ...override }
}

export function floatingRootStyle(style?: StyleDesc): StyleDesc {
  return { alignItems: "start", display: "flex", position: "relative", ...style }
}

/** `style` on a part is always a `StyleDesc`; a CSS string is left to the element. */
export function styleObject(style: unknown): StyleDesc | undefined {
  return style !== null && typeof style === "object"
    ? (style as StyleDesc)
    : undefined
}

/** The ilha event type of a prop name: `onClick` and `onclick` are both `click`. */
const eventTypeOf = (name: string): string | undefined =>
  name.length > 2 && name.startsWith("on") ? name.slice(2).toLowerCase() : undefined

/**
 * Take the handlers for `types` out of a prop bag, whichever case they were
 * written in. ilha lowercases event props, so a part that adds its own handler
 * has to remove the caller's spelling, or one of the two would be dropped.
 */
export function splitHandlers<Type extends string>(
  props: object,
  types: readonly Type[]
): { handlers: Partial<Record<Type, Handler>>; rest: Record<string, unknown> } {
  const handlers: Partial<Record<Type, Handler>> = {}
  const rest: Record<string, unknown> = {}
  for (const [name, value] of Object.entries(props)) {
    const type = eventTypeOf(name) as Type | undefined
    if (type && types.includes(type) && typeof value === "function") {
      handlers[type] ??= value as Handler
    } else {
      rest[name] = value
    }
  }
  return { handlers, rest }
}

export function composeHandlers(
  first?: Handler,
  second?: Handler
): Handler | undefined {
  if (!first) {
    return second
  }
  if (!second) {
    return first
  }
  return (event) => {
    first(event)
    second(event)
  }
}

const DismissLayerContext = createContext<DismissLayer | undefined>(undefined)

export interface DismissableLayerProps {
  children?: View
  /** Escape reached the window, this layer is on top, and nothing prevented it. */
  onEscapeKeyDown: (event: KeyEvent) => void
  /** Element to focus when the layer opens on top. Null leaves focus. */
  initialFocus?: () => number | null
  /** Element to focus on close. `previous` is the focus from before it opened. */
  finalFocus?: (previous: number | null) => number | null
}

/**
 * Put an open overlay on the window's layer stack while mounted. Escape closes
 * only the top layer. Layers inside this one, such as a Select in a Dialog,
 * are always above it. The stack also moves focus in and out, in stack order.
 */
export function DismissableLayer(props: DismissableLayerProps): View {
  const renderer = useGpuixRequired()
  const parent = context(DismissLayerContext)()
  // Read on the first render: the layer's elements do not exist yet, so an
  // `autoFocus` inside it has not taken focus.
  const box = atom.lazy(() => ({
    previousFocus: renderer.getFocusedElementId?.() ?? null,
    props,
  }))()
  box.props = props
  const layer = atom.lazy<DismissLayer>(() => ({
    finalFocus: (previous) => box.props.finalFocus?.(previous) ?? null,
    initialFocus: () => box.props.initialFocus?.() ?? null,
    onEscapeKeyDown: (event) => box.props.onEscapeKeyDown(event),
    parent,
  }))()
  watch.once(() => {
    let pop: (() => void) | undefined
    // After the children are painted and mounted, so `initialFocus` can name
    // one of them. Nested components paint on later microtasks, hence a task.
    const timer = setTimeout(() => {
      commitAll()
      pop = pushDismissLayer(renderer, layer, {
        previousFocus: box.previousFocus,
      })
    }, 0)
    return () => {
      clearTimeout(timer)
      pop?.()
    }
  })
  return el(DismissLayerContext.Provider, { value: layer }, props.children)
}

/** Flatten a children value, dropping the views ilha skips. */
function childList(children: View, into: View[] = []): View[] {
  if (Array.isArray(children)) {
    for (const child of children) {
      childList(child, into)
    }
  } else if (
    children !== null &&
    children !== undefined &&
    typeof children !== "boolean"
  ) {
    into.push(children)
  }
  return into
}

const isElementVNode = (view: View): view is VNode =>
  typeof view === "object" &&
  view !== null &&
  (view as { $$ilha?: unknown }).$$ilha === 1 &&
  typeof (view as VNode).type === "string"

/** The single element an `asChild` part renders onto. */
function onlyElement(children: View): VNode {
  const list = childList(children)
  const [child] = list
  if (list.length !== 1 || child === undefined || !isElementVNode(child)) {
    throw new Error("asChild requires one GPUIX intrinsic element")
  }
  return child
}

/**
 * Render `props` on a `div`, or merge them into the single `asChild` element.
 *
 * `defaultTabIndex` makes the part a tab stop, like the `<button>` Base UI
 * renders. An explicit `tabIndex` on the part, then on the child, wins. The
 * child's own handlers run before the part's, like React `asChild`.
 */
export function renderSlot(args: {
  asChild?: boolean
  children?: View
  props: Record<string, unknown>
  defaultTabIndex?: number
}): View {
  if (!args.asChild) {
    return el(
      "div",
      { ...args.props, tabIndex: args.props.tabIndex ?? args.defaultTabIndex },
      args.children
    )
  }
  const child = onlyElement(args.children)
  const own: Record<string, unknown> = { ...child.props }
  const merged: Record<string, unknown> = {}
  for (const [name, value] of Object.entries(args.props)) {
    if (value === undefined) {
      continue
    }
    const type = typeof value === "function" ? eventTypeOf(name) : undefined
    if (!type) {
      merged[name] = value
      continue
    }
    // The child may spell the same event in the other case.
    const { handlers, rest } = splitHandlers(own, [type])
    for (const key of Object.keys(own)) {
      if (!(key in rest)) {
        delete own[key]
      }
    }
    merged[name] = composeHandlers(handlers[type], value as Handler)
  }
  const partRef = args.props.ref as ((node: unknown) => void) | undefined
  const childRef = child.props.ref as ((node: unknown) => void) | undefined
  return {
    ...child,
    props: {
      ...own,
      ...merged,
      ref:
        partRef && childRef
          ? (node: unknown) => {
              childRef(node)
              partRef(node)
            }
          : (partRef ?? childRef),
      tabIndex:
        args.props.tabIndex ?? child.props.tabIndex ?? args.defaultTabIndex,
    } as never,
  }
}

/** A popup anchored to the nearest positioned ancestor. */
export function FloatingLayer(props: FloatingContentProps): View {
  const {
    align = "start",
    alignOffset = 0,
    children,
    collisionPadding,
    side = "bottom",
    sideOffset,
    style,
    ...rest
  } = props
  const offset =
    side === "top" || side === "bottom"
      ? { x: alignOffset, y: 0 }
      : { x: 0, y: alignOffset }
  return el(
    "anchored",
    {
      align,
      deferred: true,
      fit: "snap",
      gap: sideOffset ?? 0,
      occlude: style?.pointerEvents !== "none",
      offset,
      priority: 1,
      side,
      snapMargin: collisionPadding ?? 8,
    },
    el(
      "div",
      { ...rest, style: mergeStyles({ backgroundColor: "#1A1A1A" }, style) },
      children
    )
  )
}
