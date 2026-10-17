import type { EventPayload } from "@gpuix/native"
import { EVENT_PROPS } from "@gpuix/native/host"
import type { KeyEvent } from "@gpuix/native/host"

import type { HostElement } from "./host.js"

/**
 * Event object handed to an ilha handler.
 *
 * It is the GPUIX `EventPayload` plus the DOM-shaped fields an ilha handler
 * reads: `type`, `target`, `currentTarget`, `preventDefault`. A handler typed
 * against `EventPayload` or `KeyEvent` receives the same object.
 */
export interface HostEvent extends EventPayload {
  /** The ilha event type the handler was bound with, e.g. `"input"`. */
  readonly type: string
  readonly target: HostElement
  readonly currentTarget: HostElement
  readonly defaultPrevented: boolean
  /** True once a handler called `stopPropagation`. */
  readonly isPropagationStopped: boolean
  /** On `keydown` this cancels the default action, such as Tab moving focus. */
  preventDefault: () => void
  /** On `keydown` / `keyup` this skips ancestor and window key handlers. */
  stopPropagation: () => void
}

/**
 * ilha event type (the lowercased name after `on`) → GPUIX native event type.
 *
 * ilha lowercases every event prop, so `onLinkClick` and `onlinkclick` both
 * arrive as `"linkclick"`. Every GPUIX event is reachable that way; the DOM
 * names GPUIX has an equivalent for are added as aliases. Names GPUIX does not
 * emit (`dblclick`, `pointerdown`, `toggle`, …) are absent: the listener is
 * never registered and a dev warning is logged.
 */
const GPUIX_EVENT_TYPE_BY_ILHA_TYPE: ReadonlyMap<string, string> = new Map([
  ...EVENT_PROPS.map(([, type]): [string, string] => [type.toLowerCase(), type]),
  ["contextmenu", "auxClick"],
  ["input", "change"],
  ["drop", "fileDrop"],
])

export function gpuixEventTypeFor(ilhaType: string): string | undefined {
  return GPUIX_EVENT_TYPE_BY_ILHA_TYPE.get(ilhaType)
}

/** Build the handler argument for one dispatch. */
export function hostEventFor(
  type: string,
  element: HostElement,
  payload: EventPayload
): HostEvent {
  // Key events arrive as a `KeyEvent` whose flags are shared by every handler
  // of the keystroke, so they are forwarded rather than shadowed.
  const native = payload as Partial<KeyEvent>
  const state = { defaultPrevented: false, propagationStopped: false }
  return {
    ...payload,
    currentTarget: element,
    get defaultPrevented() {
      return state.defaultPrevented || native.defaultPrevented === true
    },
    get isPropagationStopped() {
      return state.propagationStopped
    },
    preventDefault: () => {
      state.defaultPrevented = true
      native.preventDefault?.()
    },
    stopPropagation: () => {
      state.propagationStopped = true
      native.stopPropagation?.()
    },
    target: element,
    type,
  }
}
