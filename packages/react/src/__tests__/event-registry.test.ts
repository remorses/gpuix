import { describe, expect, it, vi } from "vitest"
import {
  handleGpuixEvent,
  isEventProp,
  mountEventListeners,
  unregisterEventHandlers,
  updateEventListeners,
} from "../reconciler/event-registry.js"

describe("event registry", () => {
  it("owns listener registration, replacement, dispatch, and removal", () => {
    const nativeChanges: Array<[number, string, boolean]> = []
    const renderer = {
      setEventListener(elementId: number, eventType: string, enabled: boolean): void {
        nativeChanges.push([elementId, eventType, enabled])
      },
    }
    const firstClick = vi.fn()
    const secondClick = vi.fn()
    const change = vi.fn()
    const elementId = 7_001
    const initialProps = { onClick: firstClick, onChange: change }

    mountEventListeners(renderer, elementId, initialProps)
    expect(nativeChanges).toEqual([
      [elementId, "change", true],
      [elementId, "click", true],
    ])

    handleGpuixEvent({ elementId, eventType: "click" })
    handleGpuixEvent({ elementId, eventType: "change" })
    expect(firstClick).toHaveBeenCalledOnce()
    expect(change).toHaveBeenCalledOnce()

    nativeChanges.length = 0
    updateEventListeners(renderer, elementId, initialProps, { onClick: secondClick })
    expect(nativeChanges).toEqual([[elementId, "change", false]])

    handleGpuixEvent({ elementId, eventType: "click" })
    handleGpuixEvent({ elementId, eventType: "change" })
    expect(firstClick).toHaveBeenCalledOnce()
    expect(secondClick).toHaveBeenCalledOnce()
    expect(change).toHaveBeenCalledOnce()

    unregisterEventHandlers(elementId)
    handleGpuixEvent({ elementId, eventType: "click" })
    expect(secondClick).toHaveBeenCalledOnce()
  })

  it("recognizes every reserved event prop without reserving normal props", () => {
    expect(isEventProp("onClick")).toBe(true)
    expect(isEventProp("onToggleFile")).toBe(true)
    expect(isEventProp("style")).toBe(false)
    expect(isEventProp("testId")).toBe(false)
  })
})
