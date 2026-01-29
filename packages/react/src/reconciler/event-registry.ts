import type { EventPayload } from "@gpuix/native"

// Event handler registry
const eventHandlers = new Map<string, Map<string, (event: EventPayload) => void>>()

export function handleGpuixEvent(payload: EventPayload): void {
  console.log("[GPUIX] handleGpuixEvent:", payload.elementId, payload.eventType)
  const elementHandlers = eventHandlers.get(payload.elementId)
  if (elementHandlers) {
    const handler = elementHandlers.get(payload.eventType)
    if (handler) {
      handler(payload)
    } else {
      console.log("[GPUIX] No handler for event type:", payload.eventType)
    }
  } else {
    console.log("[GPUIX] No handlers registered for element:", payload.elementId)
  }
}

export function registerEventHandler(
  elementId: string,
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

export function clearEventHandlers(): void {
  eventHandlers.clear()
}
