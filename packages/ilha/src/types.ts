import type { HostProps, StyleDesc } from "@gpuix/native/host"
import type { AtomHandle, View } from "ilha"

import type { HostEvent } from "./events.js"
import type { HostElement } from "./host.js"

/** Handler for an ilha lowercase event prop. */
export type Handler = (event: HostEvent) => unknown

/**
 * Values that bind through `value`, like ilha's DOM `Bindable<T>`. An
 * `AtomHandle` is invariant in `T`, so the constituents are listed explicitly.
 */
export type Bindable<T> =
  | T
  | AtomHandle<T>
  | (string extends T ? AtomHandle<string> : never)
  | (number extends T ? AtomHandle<number> : never)
  | (boolean extends T ? AtomHandle<boolean> : never)

/** Lowercase native event props ilha binds. */
export interface IlhaEvents {
  onclick?: Handler
  onauxclick?: Handler
  oncontextmenu?: Handler
  onmousedown?: Handler
  onmouseup?: Handler
  onmousemove?: Handler
  onmouseenter?: Handler
  onmouseleave?: Handler
  oninput?: Handler
  onchange?: Handler
  onsubmit?: Handler
  onkeydown?: Handler
  onkeyup?: Handler
  onfocus?: Handler
  onblur?: Handler
  onscroll?: Handler
  ondrop?: Handler
}

export interface IlhaExtras extends IlhaEvents {
  /**
   * GPUIX styles. A CSS declaration string works too; either form is converted
   * to `StyleDesc`. GPUIX has no CSS classes, so `class` is not accepted.
   */
  style?: StyleDesc | string | null
  key?: string | number | null
  /** Receives the host element on mount and `null` on unmount. */
  ref?: (node: HostElement | null) => void
  children?: View
}

/**
 * Every handler receives the ilha `HostEvent`, whichever case the prop is
 * written in: `onLinkClick` and `onclick` are bound by the same code.
 */
type IlhaHandlers<P> = {
  [K in keyof P]: K extends `on${string}` ? Handler : P[K]
}

/** Props of a GPUIX element as an ilha component writes them. */
export type ElementProps<P> = P extends unknown
  ? IlhaHandlers<Omit<P, "style">> & IlhaExtras
  : never

export type FormProps<P> = P extends unknown
  ? IlhaHandlers<Omit<P, "style" | "value" | "checked">> &
      IlhaExtras & {
        value?: Bindable<string | number> | null
      }
  : never

/** Props of a `<div>`: what most headless parts accept and pass through. */
export type DivProps = ElementProps<HostProps>
