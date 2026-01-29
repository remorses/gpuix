import React from "react"
import type { ReactNode } from "react"
import type { OpaqueRoot } from "react-reconciler"
import { ConcurrentRoot } from "react-reconciler/constants"
import { GpuixRenderer, type EventPayload } from "@gpuix/native"
import { reconciler } from "./reconciler"
import type { Container, ElementDesc } from "../types/host"

// Event handler registry
const eventHandlers = new Map<string, Map<string, (event: EventPayload) => void>>()

function handleGpuixEvent(payload: EventPayload): void {
  const elementHandlers = eventHandlers.get(payload.elementId)
  if (elementHandlers) {
    const handler = elementHandlers.get(payload.eventType)
    if (handler) {
      handler(payload)
    }
  }
}

// Register event handlers when building the tree
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

// Clear all event handlers (called before each render)
export function clearEventHandlers(): void {
  eventHandlers.clear()
}

export interface Root {
  render: (node: ReactNode) => void
  unmount: () => void
}

/**
 * Create a root for rendering React to GPUI
 */
export function createRoot(renderer: GpuixRenderer): Root {
  let container: OpaqueRoot | null = null
  let currentTree: ElementDesc | null = null

  // Create a container that bridges React to GPUI
  const gpuixContainer: Container = {
    render(tree: ElementDesc): void {
      console.log("[GPUIX] Container.render called with tree:", JSON.stringify(tree, null, 2))
      currentTree = tree
      // Register event handlers from the tree
      registerTreeEventHandlers(tree)
      // Send to native renderer
      const jsonTree = JSON.stringify(tree)
      console.log("[GPUIX] Sending to native renderer, JSON length:", jsonTree.length)
      renderer.render(jsonTree)
      console.log("[GPUIX] Native render() returned")
    },
    requestRender(): void {
      console.log("[GPUIX] Container.requestRender called, has tree:", !!currentTree)
      if (currentTree) {
        this.render(currentTree)
      }
    },
  }

  const cleanup = (): void => {
    if (container) {
      reconciler.updateContainer(null, container, null, () => {})
      // @ts-expect-error types not up to date
      reconciler.flushSyncWork?.()
      container = null
    }
    clearEventHandlers()
  }

  return {
    render: (node: ReactNode): void => {
      // Clear previous event handlers
      clearEventHandlers()

      // Types are out of date with react-reconciler 0.31.0
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      container = (reconciler.createContainer as any)(
        gpuixContainer,
        ConcurrentRoot,
        null, // hydrationCallbacks
        false, // isStrictMode
        null, // concurrentUpdatesByDefaultOverride
        "", // identifierPrefix
        console.error, // onUncaughtError
        console.error, // onCaughtError
        console.error, // onRecoverableError
        null // transitionCallbacks
      )

      reconciler.updateContainer(
        React.createElement(React.Fragment, null, node),
        container,
        null,
        () => {}
      )
    },

    unmount: cleanup,
  }
}

// Helper to register all event handlers from a tree
function registerTreeEventHandlers(tree: ElementDesc): void {
  // This will be populated from the actual props during reconciliation
  // For now, we just traverse and prepare the structure
  if (tree.children) {
    for (const child of tree.children) {
      registerTreeEventHandlers(child)
    }
  }
}

// Re-export for convenience
export { reconciler }

// flushSync for synchronous updates
const _r = reconciler as typeof reconciler & {
  flushSyncFromReconciler?: typeof reconciler.flushSync
}
export const flushSync = _r.flushSyncFromReconciler ?? _r.flushSync
