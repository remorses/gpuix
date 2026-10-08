/** Headless Base UI-shaped Tooltip: opens on hover and on focus. */

import { atom, context, createContext, watch } from "ilha"
import type { View } from "ilha"

import type { DivProps, Handler } from "../types.js"
import {
  DismissableLayer,
  FloatingLayer,
  el,
  floatingRootStyle,
  renderSlot,
  splitHandlers,
  styleObject,
} from "./floating.js"
import type { FloatingContentProps } from "./floating.js"

interface ProviderState {
  delayDuration: number
  skipDelayDuration: number
  disableHoverableContent: boolean
  lastClosedAt: { current: number }
}

const defaultProvider: ProviderState = {
  delayDuration: 0,
  disableHoverableContent: false,
  lastClosedAt: { current: Number.NEGATIVE_INFINITY },
  skipDelayDuration: 300,
}

const ProviderContext = createContext(defaultProvider)

export interface TooltipProviderProps {
  children?: View
  /** Milliseconds a pointer rests on a trigger before its tooltip opens. */
  delayDuration?: number
  /** Within this long of one tooltip closing, the next opens with no delay. */
  skipDelayDuration?: number
  /** Close as soon as the pointer leaves the trigger, even onto the tooltip. */
  disableHoverableContent?: boolean
}

export function TooltipProvider(props: TooltipProviderProps): View {
  const value = atom.lazy<ProviderState>(() => ({
    ...defaultProvider,
    lastClosedAt: { current: Number.NEGATIVE_INFINITY },
  }))()
  value.delayDuration = props.delayDuration ?? 0
  value.skipDelayDuration = props.skipDelayDuration ?? 300
  value.disableHoverableContent = props.disableHoverableContent ?? false
  return el(ProviderContext.Provider, { value }, props.children)
}

interface TooltipState {
  open(): boolean
  hoverable(): boolean
  openNow(): void
  scheduleOpen(): void
  scheduleClose(): void
  cancelClose(): void
  close(): void
}

const TooltipContext = createContext<TooltipState | undefined>(undefined)

function useTooltip(name: string): TooltipState {
  const value = context(TooltipContext)()
  if (!value) {
    throw new Error(`${name} must be used inside Tooltip`)
  }
  return value
}

export interface TooltipProps extends DivProps {
  open?: boolean
  defaultOpen?: boolean
  onOpenChange?: (open: boolean) => void
  delayDuration?: number
  disableHoverableContent?: boolean
}

export function Tooltip(props: TooltipProps): View {
  const provider = context(ProviderContext)()
  const internal = atom(props.defaultOpen ?? false)
  const box = atom.lazy(() => ({ props, provider }))()
  box.props = props
  box.provider = provider
  const controls = atom.lazy(() => {
    let openTimer: ReturnType<typeof setTimeout> | undefined
    let closeTimer: ReturnType<typeof setTimeout> | undefined
    const open = () =>
      box.props.open === undefined ? internal() : box.props.open
    const cancelOpen = () => {
      clearTimeout(openTimer)
      openTimer = undefined
    }
    const cancelClose = () => {
      clearTimeout(closeTimer)
      closeTimer = undefined
    }
    const hoverable = () =>
      !(
        box.props.disableHoverableContent ??
        box.provider.disableHoverableContent
      )
    const setOpen = (next: boolean) => {
      const previous = open()
      cancelOpen()
      cancelClose()
      if (box.props.open === undefined) {
        internal.set(next)
      }
      if (!next) {
        box.provider.lastClosedAt.current = Date.now()
      }
      if (previous !== next) {
        box.props.onOpenChange?.(next)
      }
    }
    const state: TooltipState = {
      cancelClose,
      close: () => setOpen(false),
      hoverable,
      open,
      openNow: () => setOpen(true),
      scheduleClose() {
        cancelOpen()
        if (hoverable()) {
          // Long enough for the pointer to cross onto the tooltip.
          closeTimer = setTimeout(() => setOpen(false), 80)
        } else {
          setOpen(false)
        }
      },
      scheduleOpen() {
        cancelClose()
        const recent =
          Date.now() - box.provider.lastClosedAt.current <=
          box.provider.skipDelayDuration
        const delay = recent
          ? 0
          : (box.props.delayDuration ?? box.provider.delayDuration)
        if (delay <= 0) {
          setOpen(true)
        } else {
          openTimer = setTimeout(() => setOpen(true), delay)
        }
      },
    }
    return {
      dispose() {
        cancelOpen()
        cancelClose()
      },
      state,
    }
  })()
  watch.once(() => controls.dispose)
  const {
    children,
    defaultOpen: _defaultOpen,
    delayDuration: _delayDuration,
    disableHoverableContent: _disableHoverableContent,
    onOpenChange: _onOpenChange,
    open: _open,
    style,
    ...rest
  } = props
  return el(
    TooltipContext.Provider,
    { value: controls.state },
    el("div", { ...rest, style: floatingRootStyle(styleObject(style)) }, children)
  )
}

export interface TooltipTriggerProps extends DivProps {
  asChild?: boolean
}

export function TooltipTrigger(props: TooltipTriggerProps): View {
  const state = useTooltip("TooltipTrigger")
  const { handlers, rest } = splitHandlers(props, [
    "mouseenter",
    "mouseleave",
    "mousedown",
    "click",
    "focus",
    "blur",
  ])
  const { asChild, children, ...host } = rest
  const after =
    (own: Handler | undefined, then: () => void): Handler =>
    (event) => {
      own?.(event)
      then()
    }
  return renderSlot({
    asChild: asChild as boolean | undefined,
    children: children as View,
    defaultTabIndex: 0,
    props: {
      ...host,
      onBlur: after(handlers.blur, state.close),
      onClick: after(handlers.click, state.close),
      onFocus: after(handlers.focus, state.openNow),
      onMouseDown: after(handlers.mousedown, state.close),
      onMouseEnter: after(handlers.mouseenter, state.scheduleOpen),
      onMouseLeave: after(handlers.mouseleave, state.scheduleClose),
    },
  })
}

export interface TooltipContentProps extends FloatingContentProps {}

export function TooltipContent(props: TooltipContentProps): View {
  const state = useTooltip("TooltipContent")
  if (!state.open()) {
    return null
  }
  const { handlers, rest } = splitHandlers(props, ["mouseenter", "mouseleave"])
  const floating = rest as FloatingContentProps
  return el(
    DismissableLayer,
    { onEscapeKeyDown: state.close },
    FloatingLayer({
      ...floating,
      align: floating.align ?? "center",
      onMouseEnter: ((event) => {
        handlers.mouseenter?.(event)
        if (state.hoverable()) {
          state.cancelClose()
        }
      }) satisfies Handler,
      onMouseLeave: ((event) => {
        handlers.mouseleave?.(event)
        state.scheduleClose()
      }) satisfies Handler,
      side: floating.side ?? "top",
    })
  )
}

export {
  Tooltip as Root,
  TooltipContent as Content,
  TooltipProvider as Provider,
  TooltipTrigger as Trigger,
}
