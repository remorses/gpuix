import type { EventPayload } from "@gpuix/native"
import type { NativeRenderer, Props } from "../types/host.js"

type EventHandler = (event: EventPayload) => void
type EventListenerRenderer = Pick<NativeRenderer, "setEventListener">

const EVENT_PROPS = [
  // Custom element events
  ["onToggleFile", "toggleFile"],
  ["onShowMore", "showMore"],
  ["onLineClick", "lineClick"],
  ["onLinkClick", "linkClick"],
  ["onChange", "change"],
  ["onSubmit", "submit"],
  // Mouse events
  ["onClick", "click"],
  ["onMouseDown", "mouseDown"],
  ["onMouseUp", "mouseUp"],
  ["onMouseEnter", "mouseEnter"],
  ["onMouseLeave", "mouseLeave"],
  ["onMouseMove", "mouseMove"],
  ["onMouseDownOutside", "mouseDownOutside"],
  // Keyboard events
  ["onKeyDown", "keyDown"],
  ["onKeyUp", "keyUp"],
  // Focus events
  ["onFocus", "focus"],
  ["onBlur", "blur"],
  // Scroll events
  ["onScroll", "scroll"],
] as const satisfies ReadonlyArray<readonly [keyof Props, string]>

const eventPropNames = Object.fromEntries(
  EVENT_PROPS.map(([name]) => [name, true])
) as Record<string, true>
const eventHandlers = new Map<number, Map<string, EventHandler>>()

export function isEventProp(name: string): boolean {
  return eventPropNames[name] === true
}

export function handleGpuixEvent(payload: EventPayload): void {
  eventHandlers.get(payload.elementId)?.get(payload.eventType)?.(payload)
}

export function mountEventListeners(
  renderer: EventListenerRenderer,
  elementId: number,
  props: Props
): void {
  for (const [propName, eventType] of EVENT_PROPS) {
    const handler = props[propName] as EventHandler | undefined
    if (!handler) continue

    let elementHandlers = eventHandlers.get(elementId)
    if (!elementHandlers) {
      elementHandlers = new Map()
      eventHandlers.set(elementId, elementHandlers)
    }
    elementHandlers.set(eventType, handler)
    renderer.setEventListener(elementId, eventType, true)
  }
}

export function updateEventListeners(
  renderer: EventListenerRenderer,
  elementId: number,
  oldProps: Props,
  newProps: Props
): void {
  for (const [propName, eventType] of EVENT_PROPS) {
    const oldHandler = oldProps[propName] as EventHandler | undefined
    const newHandler = newProps[propName] as EventHandler | undefined
    if (oldHandler === newHandler) continue

    if (!newHandler) {
      const elementHandlers = eventHandlers.get(elementId)
      elementHandlers?.delete(eventType)
      if (elementHandlers?.size === 0) eventHandlers.delete(elementId)
      renderer.setEventListener(elementId, eventType, false)
      continue
    }

    let elementHandlers = eventHandlers.get(elementId)
    if (!elementHandlers) {
      elementHandlers = new Map()
      eventHandlers.set(elementId, elementHandlers)
    }
    elementHandlers.set(eventType, newHandler)
    if (!oldHandler) renderer.setEventListener(elementId, eventType, true)
  }
}

export function unregisterEventHandlers(elementId: number): void {
  eventHandlers.delete(elementId)
}
