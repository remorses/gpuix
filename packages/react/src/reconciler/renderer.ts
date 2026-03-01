import React from "react"
import type { ReactNode } from "react"
import type { OpaqueRoot } from "react-reconciler"
import { ConcurrentRoot } from "react-reconciler/constants"
import { GpuixRenderer } from "@gpuix/native"
import { reconciler } from "./reconciler"
import type { Container, NativeRenderer } from "../types/host"
import { clearEventHandlers, handleGpuixEvent } from "./event-registry"
import { setNativeRenderer } from "./host-config"

export function createRenderer(
  onEvent?: (event: import("@gpuix/native").EventPayload) => void
): GpuixRenderer {
  return new GpuixRenderer((err, event) => {
    if (err) {
      console.error("[GPUIX] Native event error:", err)
      return
    }
    if (event) {
      handleGpuixEvent(event)
      if (onEvent) {
        onEvent(event)
      }
    }
  })
}

export interface Root {
  render: (node: ReactNode) => void
  unmount: () => void
}

/**
 * Create a root for rendering React to GPUI (or a TestRenderer for tests).
 * Mutations go directly to the renderer — no JSON tree serialization.
 */
export function createRoot(renderer: NativeRenderer): Root {
  let container: OpaqueRoot | null = null

  // Wire up the renderer for host-config to use
  setNativeRenderer(renderer)

  const gpuixContainer: Container = {
    renderer,
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

  // Create container once — reuse on subsequent render() calls
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  container = (reconciler.createContainer as any)(
    gpuixContainer,
    ConcurrentRoot,
    null,
    false,
    null,
    "",
    console.error,
    console.error,
    console.error,
    null
  )

  return {
    render: (node: ReactNode): void => {
      clearEventHandlers()

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

export { reconciler }

const _r = reconciler as typeof reconciler & {
  flushSyncFromReconciler?: typeof reconciler.flushSync
}
export const flushSync = _r.flushSyncFromReconciler ?? _r.flushSync
