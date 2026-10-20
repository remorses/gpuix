/** Headless Base UI-shaped Combobox: a text field that filters a list. */

import type { InputProps } from "@gpuix/native/host"
import { atom, context, createContext, watch } from "ilha"
import type { View } from "ilha"

import type { HostElement } from "../host.js"
import { useGpuixRequired } from "../root.js"
import type { DivProps, FormProps, Handler } from "../types.js"
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

export type ComboboxValue = string | string[] | null

interface ComboboxState {
  open(): boolean
  value(): ComboboxValue
  input(): string
  active(): number | null
  filtered(): string[]
  disabled(): boolean
  inputRef: { current: HostElement | null }
  setOpen(value: boolean): void
  setInput(value: string): void
  setActive(value: number | null): void
  move(delta: number): void
  select(value: string): void
  registerDisabled(value: string, disabled: boolean): () => void
}

const ComboboxContext = createContext<ComboboxState | undefined>(undefined)

function useCombobox(name: string): ComboboxState {
  const value = context(ComboboxContext)()
  if (!value) {
    throw new Error(`${name} must be used inside Combobox`)
  }
  return value
}

export interface ComboboxProps extends DivProps {
  items?: readonly string[]
  value?: ComboboxValue
  defaultValue?: ComboboxValue
  onValueChange?: (value: ComboboxValue) => void
  inputValue?: string
  defaultInputValue?: string
  onInputValueChange?: (value: string) => void
  open?: boolean
  defaultOpen?: boolean
  onOpenChange?: (open: boolean) => void
  multiple?: boolean
  disabled?: boolean
  /** Highlight the first match as the user types. */
  autoHighlight?: boolean | "always"
  /** `null` turns filtering off; a function replaces the substring match. */
  filter?:
    | null
    | ((
        item: string,
        query: string,
        itemToString: (item: string) => string
      ) => boolean)
  itemToStringValue?: (item: string) => string
}

export function Combobox(props: ComboboxProps): View {
  const renderer = useGpuixRequired()
  const internalValue = atom<ComboboxValue>(props.defaultValue ?? null)
  const internalInput = atom(props.defaultInputValue ?? "")
  const internalOpen = atom(props.defaultOpen ?? false)
  const active = atom<number | null>(null)
  // The parts read the latest props through this box; a controlled value
  // reaches them because a parent repaint reruns its nested components.
  const box = atom.lazy(() => ({ props }))()
  box.props = props
  const state = atom.lazy<ComboboxState>(() => {
    const inputRef: ComboboxState["inputRef"] = { current: null }
    const disabledItems = new Set<string>()
    const value = () =>
      box.props.value === undefined ? internalValue() : box.props.value
    const input = () =>
      box.props.inputValue === undefined
        ? internalInput()
        : box.props.inputValue
    const open = () =>
      box.props.open === undefined ? internalOpen() : box.props.open
    const itemToString = (item: string) =>
      box.props.itemToStringValue?.(item) ?? item
    const matching = (text: string) => {
      const items = [...(box.props.items ?? [])]
      const { filter } = box.props
      const query = text.trim().toLowerCase()
      if (filter === null) {
        return items
      }
      if (filter) {
        return items.filter((item) => filter(item, query, itemToString))
      }
      return items.filter((item) =>
        itemToString(item).toLowerCase().includes(query)
      )
    }
    const setOpen = (next: boolean) => {
      const previous = open()
      if (box.props.open === undefined) {
        internalOpen.set(next)
      }
      if (previous !== next) {
        box.props.onOpenChange?.(next)
      }
      if (next) {
        queueMicrotask(() => {
          const id = inputRef.current?.id
          if (id) {
            renderer.focusElement?.(id)
          }
        })
      }
    }
    const setInput = (next: string) => {
      const previous = input()
      if (box.props.inputValue === undefined) {
        internalInput.set(next)
      }
      if (previous !== next) {
        box.props.onInputValueChange?.(next)
      }
      const firstEnabled = matching(next).findIndex(
        (item) => !disabledItems.has(item)
      )
      active.set(
        box.props.autoHighlight && firstEnabled >= 0 ? firstEnabled : null
      )
    }
    const setValue = (next: ComboboxValue) => {
      const previous = value()
      if (box.props.value === undefined) {
        internalValue.set(next)
      }
      if (!Object.is(previous, next)) {
        box.props.onValueChange?.(next)
      }
    }
    return {
      active,
      disabled: () => box.props.disabled ?? false,
      filtered: () => matching(input()),
      input,
      inputRef,
      move(delta) {
        const items = matching(input())
        const current = active()
        let next = current === null ? (delta > 0 ? -1 : 0) : current
        for (let checked = 0; checked < items.length; checked += 1) {
          next = (next + delta + items.length) % items.length
          const item = items[next]
          if (item !== undefined && !disabledItems.has(item)) {
            active.set(next)
            return
          }
        }
      },
      open,
      registerDisabled(item, disabled) {
        if (disabled) {
          disabledItems.add(item)
        }
        return () => {
          disabledItems.delete(item)
        }
      },
      select(item) {
        if (box.props.disabled || disabledItems.has(item)) {
          return
        }
        if (box.props.multiple) {
          const current = value()
          const selected = Array.isArray(current) ? current : []
          setValue(
            selected.includes(item)
              ? selected.filter((entry) => entry !== item)
              : [...selected, item]
          )
          setInput("")
          return
        }
        setValue(item)
        setInput(itemToString(item))
        setOpen(false)
      },
      setActive: (next) => active.set(next),
      setInput,
      setOpen,
      value,
    }
  })()
  const {
    autoHighlight: _autoHighlight,
    children,
    defaultInputValue: _defaultInputValue,
    defaultOpen: _defaultOpen,
    defaultValue: _defaultValue,
    disabled: _disabled,
    filter: _filter,
    inputValue: _inputValue,
    itemToStringValue: _itemToStringValue,
    items: _items,
    multiple: _multiple,
    onInputValueChange: _onInputValueChange,
    onOpenChange: _onOpenChange,
    onValueChange: _onValueChange,
    open: _open,
    style,
    value: _value,
    ...rest
  } = props
  return el(
    ComboboxContext.Provider,
    { value: state },
    el("div", { ...rest, style: floatingRootStyle(styleObject(style)) }, children)
  )
}

export interface ComboboxInputProps
  extends Omit<FormProps<InputProps>, "value"> {
  disabled?: boolean
}

export function ComboboxInput(props: ComboboxInputProps): View {
  const state = useCombobox("ComboboxInput")
  const { handlers, rest } = splitHandlers(props, [
    "click",
    "focus",
    "blur",
    "change",
    "input",
    "keydown",
    "submit",
  ])
  const { disabled: ownDisabled, readOnly, ref, ...host } = rest
  const disabled = (ownDisabled as boolean | undefined) ?? state.disabled()
  const openIfEnabled = () => {
    if (!disabled) {
      state.setOpen(true)
    }
  }
  return el("input", {
    ...host,
    autoFocus: state.open(),
    onBlur: ((event) => {
      handlers.blur?.(event)
      // Focus leaving the input closes the popup, like Base UI. Tab moves
      // focus only when no handler prevented it, so a kept Tab keeps it open.
      state.setOpen(false)
    }) satisfies Handler,
    onChange: ((event) => {
      handlers.input?.(event)
      handlers.change?.(event)
      state.setInput(event.value ?? "")
      openIfEnabled()
    }) satisfies Handler,
    onClick: ((event) => {
      handlers.click?.(event)
      openIfEnabled()
    }) satisfies Handler,
    onFocus: ((event) => {
      handlers.focus?.(event)
      openIfEnabled()
    }) satisfies Handler,
    onKeyDown: ((event) => {
      handlers.keydown?.(event)
      if (event.key === "down") {
        state.move(1)
      } else if (event.key === "up") {
        state.move(-1)
      }
    }) satisfies Handler,
    onSubmit: ((event) => {
      handlers.submit?.(event)
      const index = state.active()
      const item = index === null ? undefined : state.filtered().at(index)
      if (item !== undefined) {
        state.select(item)
      }
    }) satisfies Handler,
    readOnly: disabled || readOnly,
    ref(element: HostElement | null) {
      state.inputRef.current = element
      ;(ref as DivProps["ref"])?.(element)
    },
    value: state.input(),
  })
}

export interface ComboboxTriggerProps extends DivProps {
  asChild?: boolean
  disabled?: boolean
}

export function ComboboxTrigger(props: ComboboxTriggerProps): View {
  const state = useCombobox("ComboboxTrigger")
  const { handlers, rest } = splitHandlers(props, ["click"])
  const { asChild, children, disabled: ownDisabled, tabIndex, ...host } = rest
  const disabled = (ownDisabled as boolean | undefined) ?? state.disabled()
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
      tabIndex: disabled ? -1 : tabIndex,
    },
  })
}

/** h() hands children over as a list; a render function is its only entry. */
const renderFunctionOf = <Arg>(
  children: unknown
): ((arg: Arg) => View) | undefined => {
  const [only] = Array.isArray(children) ? children : [children]
  return typeof only === "function" ? (only as (arg: Arg) => View) : undefined
}

const hasChildren = (children: unknown): boolean =>
  Array.isArray(children) ? children.length > 0 : children !== undefined

export interface ComboboxValueProps extends Omit<DivProps, "children"> {
  children?: View | ((value: ComboboxValue) => View)
  placeholder?: View
}

export function ComboboxValue(props: ComboboxValueProps): View {
  const state = useCombobox("ComboboxValue")
  const { children, placeholder, ...rest } = props
  const value = state.value()
  const render = renderFunctionOf<ComboboxValue>(children)
  if (render) {
    return el("div", rest, render(value))
  }
  if (hasChildren(children)) {
    return el("div", rest, children as View)
  }
  return el(
    "div",
    rest,
    Array.isArray(value) ? value.join(", ") : (value ?? placeholder)
  )
}

export function ComboboxContent(props: FloatingContentProps): View {
  const state = useCombobox("ComboboxContent")
  if (!state.open()) {
    return null
  }
  const { handlers, rest } = splitHandlers(props, ["mousedownoutside"])
  return el(
    DismissableLayer,
    { onEscapeKeyDown: () => state.setOpen(false) },
    FloatingLayer({
      ...(rest as FloatingContentProps),
      onMouseDownOutside: ((event) => {
        handlers.mousedownoutside?.(event)
        state.setOpen(false)
      }) satisfies Handler,
    })
  )
}

export interface ComboboxListProps extends Omit<DivProps, "children"> {
  /** A function is called once per item that matches the input. */
  children?: View | ((item: string) => View)
}

export function ComboboxList(props: ComboboxListProps): View {
  const state = useCombobox("ComboboxList")
  const { children, ...rest } = props
  const render = renderFunctionOf<string>(children)
  return el(
    "div",
    rest,
    render ? state.filtered().map((item) => render(item)) : (children as View)
  )
}

export interface ComboboxItemState {
  selected: boolean
  highlighted: boolean
  disabled: boolean
}

export interface ComboboxItemProps extends Omit<DivProps, "style" | "children"> {
  value: string
  disabled?: boolean
  asChild?: boolean
  children?: View | ((state: ComboboxItemState) => View)
  style?: StateStyle<ComboboxItemState>
}

export function ComboboxItem(props: ComboboxItemProps): View {
  const state = useCombobox("ComboboxItem")
  const disabled = props.disabled ?? false
  watch.once(() => state.registerDisabled(props.value, disabled))
  const value = state.value()
  const index = state.filtered().indexOf(props.value)
  const itemState: ComboboxItemState = {
    disabled,
    highlighted: index >= 0 && index === state.active(),
    selected: Array.isArray(value)
      ? value.includes(props.value)
      : value === props.value,
  }
  const { handlers, rest } = splitHandlers(props, ["click", "mouseenter"])
  const { asChild, children, disabled: _disabled, style, value: _value, ...host } =
    rest
  const render = renderFunctionOf<ComboboxItemState>(children)
  return renderSlot({
    asChild: asChild as boolean | undefined,
    children: render ? render(itemState) : (children as View),
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
          state.setActive(state.filtered().indexOf(props.value))
        }
      }) satisfies Handler,
      style: resolveStyle(style as ComboboxItemProps["style"], itemState),
    },
  })
}

/** Painted only while nothing matches the input. */
export function ComboboxEmpty(props: DivProps): View {
  const state = useCombobox("ComboboxEmpty")
  if (state.filtered().length > 0) {
    return null
  }
  const { children, ...rest } = props
  return el("div", rest, children)
}

function divPart(props: DivProps): View {
  const { children, ...rest } = props
  return el("div", rest, children)
}

export const ComboboxGroup = divPart
export const ComboboxLabel = divPart
export const ComboboxSeparator = divPart

export {
  Combobox as Root,
  ComboboxContent as Content,
  ComboboxEmpty as Empty,
  ComboboxGroup as Group,
  ComboboxInput as Input,
  ComboboxItem as Item,
  ComboboxLabel as Label,
  ComboboxList as List,
  ComboboxSeparator as Separator,
  ComboboxTrigger as Trigger,
  ComboboxValue as Value,
}
