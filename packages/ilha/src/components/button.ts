/** Headless Base UI-shaped Button: a focusable `div` that acts like `<button>`. */

import type { View } from "ilha"

import type { DivProps, Handler } from "../types.js"
import { renderSlot, resolveStyle, splitHandlers } from "./floating.js"
import type { StateStyle } from "./floating.js"

export interface ButtonState {
  disabled: boolean
}

export interface ButtonBehavior {
  disabled?: boolean
  /** Keep a disabled button in the Tab order, for example while it saves. */
  focusableWhenDisabled?: boolean
  onClick?: Handler
  onKeyDown?: Handler
  onKeyUp?: Handler
  tabIndex?: number
}

/**
 * Props that make any element act like a `<button>`: a tab stop, `role`, and
 * `onClick` from a press, Enter (on key down) or Space (on key up), like a
 * browser. Primitives such as `DialogTrigger` build on it.
 */
export function buttonProps(behavior: ButtonBehavior): Record<string, unknown> {
  const disabled = behavior.disabled ?? false
  return {
    onClick: ((event) => {
      if (!disabled) {
        behavior.onClick?.(event)
      }
    }) satisfies Handler,
    onKeyDown: ((event) => {
      behavior.onKeyDown?.(event)
      if (disabled || event.defaultPrevented) {
        return
      }
      if (event.key === "enter" && !event.isHeld) {
        behavior.onClick?.(event)
      }
      // Space would otherwise scroll or type; it fires on release.
      if (event.key === "space") {
        event.preventDefault()
      }
    }) satisfies Handler,
    onKeyUp: ((event) => {
      behavior.onKeyUp?.(event)
      if (!disabled && !event.defaultPrevented && event.key === "space") {
        behavior.onClick?.(event)
      }
    }) satisfies Handler,
    role: "button",
    // Unset keeps an `asChild` child's own tabIndex; renderSlot defaults to 0.
    tabIndex:
      disabled && !behavior.focusableWhenDisabled ? -1 : behavior.tabIndex,
  }
}

export interface ButtonProps extends Omit<DivProps, "style"> {
  disabled?: boolean
  focusableWhenDisabled?: boolean
  asChild?: boolean
  style?: StateStyle<ButtonState>
}

/** Take the button behaviour out of a part's props, in either event spelling. */
export function splitButtonProps(props: object & ButtonBehavior): {
  behavior: ButtonBehavior
  rest: Record<string, unknown>
} {
  const { handlers, rest } = splitHandlers(props, ["click", "keydown", "keyup"])
  const { disabled, focusableWhenDisabled, tabIndex, ...host } = rest
  return {
    behavior: {
      disabled: disabled as boolean | undefined,
      focusableWhenDisabled: focusableWhenDisabled as boolean | undefined,
      onClick: handlers.click,
      onKeyDown: handlers.keydown,
      onKeyUp: handlers.keyup,
      tabIndex: tabIndex as number | undefined,
    },
    rest: host,
  }
}

export function Button(props: ButtonProps): View {
  const { behavior, rest } = splitButtonProps(props)
  const { asChild, children, style, ...host } = rest
  return renderSlot({
    asChild: asChild as boolean | undefined,
    children: children as View,
    defaultTabIndex: 0,
    props: {
      ...host,
      ...buttonProps(behavior),
      style: resolveStyle(style as ButtonProps["style"], {
        disabled: behavior.disabled ?? false,
      }),
    },
  })
}
