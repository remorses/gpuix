import type { EventPayload } from "@gpuix/native"

// Event handler registry — keyed by numeric element ID.
const eventHandlers = new Map<number, Map<string, (event: EventPayload) => void>>()

export function handleGpuixEvent(payload: EventPayload): void {
  const elementHandlers = eventHandlers.get(payload.elementId)
  if (elementHandlers) {
    const handler = elementHandlers.get(payload.eventType)
    if (handler) {
      handler(payload)
    }
  }
}

export function registerEventHandler(
  elementId: number,
  eventType: string,
  handler: (event: EventPayload) => void
): void {
  let elementHandlers = eventHandlers.get(elementId)
  if (!elementHandlers) {
    elementHandlers = new Map()
    eventHandlers.set(elementId, elementHandlers)
  }
  elementHandlers.set(eventType, handler)
}

export function unregisterEventHandlers(elementId: number): void {
  eventHandlers.delete(elementId)
}

export function clearEventHandlers(): void {
  eventHandlers.clear()
}
