/** Headless Base UI-shaped Select: a trigger and an anchored list of items. */

import type { KeyEvent } from "@gpuix/native/host"
import { atom, context, createContext, watch } from "ilha"
import type { View } from "ilha"

import type { HostElement } from "../host.js"
import { useGpuixRequired } from "../root.js"
import type { DivProps, Handler } from "../types.js"
import {
  DismissableLayer,
  FloatingLayer,
  el,
  floatingRootStyle,
  renderSlot,
  resolveStyle,
  splitHandlers,
  styleObject,
} from "./floating.js"
import type { FloatingContentProps, StateStyle } from "./floating.js"

export interface SelectItemData {
  value: string
  label?: View
  textValue?: string
}

interface ItemRecord {
  value: string
  disabled: boolean
}

interface SelectState {
  open(): boolean
  value(): string | undefined
  active(): string | null
  disabled(): boolean
  label(value: string): View
  trigger: { current: HostElement | null }
  /** `restoreFocus: false` when the user pressed elsewhere, so the press keeps
   *  the focus it just gave. */
  setOpen(value: boolean, restoreFocus?: boolean): void
  setActive(value: string | null): void
  move(delta: number): void
  select(value: string): void
  register(item: ItemRecord): () => void
}

const SelectContext = createContext<SelectState | undefined>(undefined)

function useSelect(name: string): SelectState {
  const value = context(SelectContext)()
  if (!value) {
    throw new Error(`${name} must be used inside Select`)
  }
  return value
}

export interface SelectProps extends DivProps {
  /** Optional label lookup for `SelectValue` while the popup is closed. */
  items?: readonly SelectItemData[]
  value?: string
  defaultValue?: string
  onValueChange?: (value: string) => void
  open?: boolean
  defaultOpen?: boolean
  onOpenChange?: (open: boolean) => void
  disabled?: boolean
}

export function Select(props: SelectProps): View {
  const renderer = useGpuixRequired()
  const internalValue = atom(props.defaultValue)
  const internalOpen = atom(props.defaultOpen ?? false)
  const active = atom<string | null>(null)
  // The parts read the latest props through this box; a controlled `value`
  // reaches them because a parent repaint reruns its nested components.
  const box = atom.lazy(() => ({ props }))()
  box.props = props
  const state = atom.lazy<SelectState>(() => {
    const items: ItemRecord[] = []
    const trigger: SelectState["trigger"] = { current: null }
    const value = () =>
      box.props.value === undefined ? internalValue() : box.props.value
    const open = () =>
      box.props.open === undefined ? internalOpen() : box.props.open
    const setOpen = (next: boolean, restoreFocus = true) => {
      const previous = open()
      if (box.props.open === undefined) {
        internalOpen.set(next)
      }
      if (previous !== next) {
        box.props.onOpenChange?.(next)
      }
      if (!next && restoreFocus && trigger.current?.id) {
        renderer.focusElement?.(trigger.current.id)
      }
      active.set(null)
    }
    return {
      active,
      disabled: () => box.props.disabled ?? false,
      label(itemValue) {
        const item = box.props.items?.find((entry) => entry.value === itemValue)
        return item?.label ?? item?.textValue ?? itemValue
      },
      move(delta) {
        const enabled = items.filter((item) => !item.disabled)
        if (enabled.length === 0) {
          return
        }
        const current = enabled.findIndex((item) => item.value === active())
        const start = current < 0 ? (delta > 0 ? -1 : 0) : current
        const next = enabled[(start + delta + enabled.length) % enabled.length]
        active.set(next?.value ?? null)
      },
      open,
      register(item) {
        items.push(item)
        // The items mount after the popup opens, so the selected one takes
        // the highlight here rather than in `setOpen`.
        if (active() === null && !item.disabled && item.value === value()) {
          active.set(item.value)
        }
        return () => {
          const index = items.indexOf(item)
          if (index >= 0) {
            items.splice(index, 1)
          }
        }
      },
      select(next) {
        if (
          box.props.disabled ||
          items.find((item) => item.value === next)?.disabled
        ) {
          return
        }
        const previous = value()
        if (box.props.value === undefined) {
          internalValue.set(next)
        }
        if (previous !== next) {
          box.props.onValueChange?.(next)
        }
        setOpen(false)
      },
      setActive: (next) => active.set(next),
      setOpen,
      trigger,
      value,
    }
  })()
  const {
    children,
    defaultOpen: _defaultOpen,
    defaultValue: _defaultValue,
    disabled: _disabled,
    items: _items,
    onOpenChange: _onOpenChange,
    onValueChange: _onValueChange,
    open: _open,
    style,
    value: _value,
    ...rest
  } = props
  return el(
    SelectContext.Provider,
    { value: state },
    el("div", { ...rest, style: floatingRootStyle(styleObject(style)) }, children)
  )
}

export interface SelectTriggerState {
  open: boolean
  disabled: boolean
  placeholder: boolean
}

export interface SelectTriggerProps extends Omit<DivProps, "style"> {
  asChild?: boolean
  disabled?: boolean
  style?: StateStyle<SelectTriggerState>
}

export function SelectTrigger(props: SelectTriggerProps): View {
  const state = useSelect("SelectTrigger")
  const { handlers, rest } = splitHandlers(props, ["click", "keydown"])
  const { asChild, children, disabled: ownDisabled, ref, style, tabIndex, ...host } =
    rest
  const disabled = (ownDisabled as boolean | undefined) ?? state.disabled()
  const open = state.open()
  return renderSlot({
    asChild: asChild as boolean | undefined,
    children: children as View,
    defaultTabIndex: 0,
    props: {
      ...host,
      onClick: ((event) => {
        handlers.click?.(event)
        if (!disabled) {
          state.setOpen(!state.open())
        }
      }) satisfies Handler,
      onKeyDown: ((event) => {
        handlers.keydown?.(event)
        if (disabled) {
          return
        }
        if (event.key === "down" || event.key === "up") {
          if (state.open()) {
            state.move(event.key === "down" ? 1 : -1)
          } else {
            state.setOpen(true)
          }
        } else if (event.key === "enter" || event.key === "space") {
          state.setOpen(!state.open())
        }
      }) satisfies Handler,
      ref(element: HostElement | null) {
        state.trigger.current = element
        ;(ref as DivProps["ref"])?.(element)
      },
      style: resolveStyle(style as SelectTriggerProps["style"], {
        disabled,
        open,
        placeholder: state.value() === undefined,
      }),
      tabIndex: disabled ? -1 : tabIndex,
    },
  })
}

export interface SelectValueProps extends DivProps {
  placeholder?: View
}

export function SelectValue(props: SelectValueProps): View {
  const state = useSelect("SelectValue")
  const { children, placeholder, ...rest } = props
  const selected = state.value()
  const own = Array.isArray(children) && children.length === 0 ? undefined : children
  return el(
    "div",
    rest,
    own ?? (selected === undefined ? placeholder : state.label(selected))
  )
}

export interface SelectContentProps extends FloatingContentProps {
  onEscapeKeyDown?: (event: KeyEvent) => void
}

export function SelectContent(props: SelectContentProps): View {
  const state = useSelect("SelectContent")
  if (!state.open()) {
    return null
  }
  const { onEscapeKeyDown, ...content } = props
  const { handlers, rest } = splitHandlers(content, [
    "keydown",
    "mousedownoutside",
  ])
  const floating = rest as FloatingContentProps
  return el(
    DismissableLayer,
    {
      onEscapeKeyDown(event: KeyEvent) {
        onEscapeKeyDown?.(event)
        state.setOpen(false)
      },
    },
    FloatingLayer({
      ...floating,
      autoFocus: true,
      onKeyDown: ((event) => {
        handlers.keydown?.(event)
        // The popup is modal, like Base UI: Tab stays inside until it closes.
        if (event.key === "tab") {
          event.preventDefault()
        } else if (event.key === "down") {
          state.move(1)
        } else if (event.key === "up") {
          state.move(-1)
        } else if (event.key === "enter" || event.key === "space") {
          const current = state.active()
          if (current) {
            state.select(current)
          }
        }
      }) satisfies Handler,
      onMouseDownOutside: ((event) => {
        handlers.mousedownoutside?.(event)
        state.setOpen(false, false)
      }) satisfies Handler,
      // Focused only to receive keys; the highlighted item shows where the
      // user is, so the popup gets no focus look of its own.
      style: { keyboardFocusDim: false, ...floating.style },
      tabIndex: floating.tabIndex ?? -1,
    })
  )
}

export interface SelectItemState {
  selected: boolean
  highlighted: boolean
  disabled: boolean
}

export interface SelectItemProps extends Omit<DivProps, "style" | "children"> {
  value: string
  disabled?: boolean
  asChild?: boolean
  children?: View | ((state: SelectItemState) => View)
  style?: StateStyle<SelectItemState>
}

export function SelectItem(props: SelectItemProps): View {
  const state = useSelect("SelectItem")
  const disabled = props.disabled ?? false
  // Keyboard navigation reads the mounted items, in mount order.
  const record = atom.lazy<ItemRecord>(() => ({ disabled, value: props.value }))()
  record.value = props.value
  record.disabled = disabled
  watch.once(() => state.register(record))
  const itemState: SelectItemState = {
    disabled,
    highlighted: state.active() === props.value,
    selected: state.value() === props.value,
  }
  const { handlers, rest } = splitHandlers(props, ["click", "mouseenter"])
  const { asChild, children, disabled: _disabled, style, value: _value, ...host } =
    rest
  // h() hands children over as a list; a render function is its only entry.
  const [only] = Array.isArray(children) ? children : [children]
  return renderSlot({
    asChild: asChild as boolean | undefined,
    children:
      typeof only === "function"
        ? (only as (state: SelectItemState) => View)(itemState)
        : (children as View),
    props: {
      ...host,
      onClick: ((event) => {
        handlers.click?.(event)
        if (!disabled) {
          state.select(props.value)
        }
      }) satisfies Handler,
      onMouseEnter: ((event) => {
        handlers.mouseenter?.(event)
        if (!disabled) {
          state.setActive(props.value)
        }
      }) satisfies Handler,
      style: resolveStyle(style as SelectItemProps["style"], itemState),
    },
  })
}

function divPart(props: DivProps): View {
  const { children, ...rest } = props
  return el("div", rest, children)
}

export const SelectGroup = divPart
export const SelectLabel = divPart
export const SelectSeparator = divPart
export const SelectScrollUpButton = divPart
export const SelectScrollDownButton = divPart

export {
  Select as Root,
  SelectContent as Content,
  SelectGroup as Group,
  SelectItem as Item,
  SelectLabel as Label,
  SelectScrollDownButton as ScrollDownButton,
  SelectScrollUpButton as ScrollUpButton,
  SelectSeparator as Separator,
  SelectTrigger as Trigger,
  SelectValue as Value,
}
