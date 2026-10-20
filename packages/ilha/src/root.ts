import type { EventPayload } from "@gpuix/native"
import {
  createMutationQueue,
  createRendererState,
  unregisterEventHandlers,
} from "@gpuix/native/host"
import type {
  EventHandlerMap,
  NativeRenderer,
  RendererRootBinding,
  RootEventHandlers,
} from "@gpuix/native/host"
import { createRenderer as createIlhaRenderer, getFiber } from "ilha/renderer"
import type {
  Component,
  FiberLocal,
  HostRenderer,
  IlhaRuntime,
  RendererHandle,
} from "ilha/renderer"

import { HostRoot, gpuixOps } from "./host.js"
import type { HostElement, HostGpuixContext, HostNode } from "./host.js"

/** Value a component reads with `useGpuix()`. */
export type GpuixContextValue = HostGpuixContext

/** The host each ilha runtime paints into. */
const ROOT_BY_RUNTIME = new WeakMap<IlhaRuntime, HostRoot>()

/**
 * Read the GPUIX host inside a component, or `null` outside one.
 *
 * The host is found through the active ilha fiber's runtime, so it is the same
 * in the root component, a nested component, a keyed row and an async or
 * generator component, and no provider wrapper sits between the app and the
 * painter.
 */
export function useGpuix(): GpuixContextValue | null {
  let fiber: FiberLocal
  try {
    fiber = getFiber()
  } catch {
    // `atom()` and friends also require a fiber; reading the host outside a
    // component is a no-op rather than an error.
    return null
  }
  return ROOT_BY_RUNTIME.get(fiber.runtime)?.gpuix ?? null
}

/** The native renderer of the GPUIX root this component is mounted in. */
export function useGpuixRequired(): NativeRenderer {
  const value = useGpuix()
  if (!value) {
    throw new Error("useGpuixRequired must be used inside a GPUIX root")
  }
  return value.renderer
}

export interface Root {
  render(fn: Component): void
  flush(): void
  flushSync<Value>(fn: () => Value): Value
  dispatch(event: EventPayload): boolean
  unmount(): void
  readonly host: HostRoot
  readonly renderer: NativeRenderer
  readonly painter: HostRenderer<HostNode, HostElement>
}

const asError = (error: unknown): Error =>
  error instanceof Error ? error : new Error(String(error))

/**
 * Own one GPUIX window's tree: the ilha painter, the mutation queue and the
 * single root binding every event routes through.
 */
export function createRoot(
  renderer: NativeRenderer,
  rootHandlers: RootEventHandlers = {}
): Root {
  const state = createRendererState(renderer)
  const eventHandlers: EventHandlerMap = new Map()
  const reportError = (error: unknown) => {
    if (rootHandlers.onUncaughtError) {
      rootHandlers.onUncaughtError(asError(error), {})
    } else {
      console.error("[gpuix-ilha] uncaught error", error)
    }
  }
  const mutations = createMutationQueue(
    renderer,
    (ids) => {
      for (const id of ids) {
        unregisterEventHandlers(eventHandlers, id)
      }
    },
    // A mutation queued outside a paint, such as an atom-bound `value`.
    (flush) =>
      queueMicrotask(() => {
        try {
          flush()
        } catch (error) {
          reportError(error)
        }
      })
  )
  const host = new HostRoot({
    allocateId: () => ++state.ids.nextElementId,
    eventHandlers,
    mutations,
    nativeRenderer: renderer,
    onError: reportError,
  })
  const painter = createIlhaRenderer(gpuixOps)

  let binding: RendererRootBinding | undefined
  let handle: RendererHandle | undefined
  const selectionListeners = new Set<(text: string | null) => void>()

  const onSelectionChange = (
    event: EventPayload,
    currentRenderer: NativeRenderer
  ) => {
    rootHandlers.onSelectionChange?.(event, currentRenderer)
    for (const listener of selectionListeners) {
      listener(event.value ?? null)
    }
  }

  const syncSelection = () => {
    if (!binding) {
      return
    }
    renderer.setWindowSelectionChange?.(
      Boolean(rootHandlers.onSelectionChange) || selectionListeners.size > 0,
      binding.windowSelectionEventId
    )
  }

  const attach = () => {
    binding = state.attach({
      eventHandlers,
      onEvent: rootHandlers.onEvent,
      onSelectionChange,
      onWindowKeyDown: rootHandlers.onKeyDown,
      onWindowKeyUp: rootHandlers.onKeyUp,
      tabNavigation: rootHandlers.tabNavigation,
    })
    // Always on: the Tab and Escape defaults run on the window event.
    renderer.setWindowKeyEvents?.(
      true,
      Boolean(rootHandlers.onKeyUp),
      binding.windowKeyEventId
    )
    // Sent on every attach, so a remount without the option restores the default.
    mutations.setKeyboardFocusDim(rootHandlers.keyboardFocusDim !== false)
    syncSelection()
  }

  const detach = () => {
    if (!binding?.detach()) {
      return
    }
    renderer.setWindowKeyEvents?.(false, false, binding.windowKeyEventId)
    renderer.setWindowSelectionChange?.(false, binding.windowSelectionEventId)
    binding = undefined
  }

  host.gpuix = {
    renderer,
    subscribeSelection(callback) {
      selectionListeners.add(callback)
      callback(renderer.getSelectedText?.() ?? null)
      syncSelection()
      return () => {
        selectionListeners.delete(callback)
        syncSelection()
      }
    },
  }
  attach()

  const stop = () => {
    if (!handle) {
      return
    }
    ROOT_BY_RUNTIME.delete(handle.runtime)
    handle.unmount()
    handle = undefined
  }

  return {
    dispatch(event) {
      try {
        return state.dispatch(event)
      } finally {
        host.commit()
      }
    },
    flush: () => host.commit(),
    flushSync(fn) {
      try {
        return fn()
      } finally {
        host.commit()
      }
    },
    host,
    painter,
    render(fn) {
      stop()
      handle = painter.mount(host as never, fn, {
        onError: reportError,
        // Before the root component runs, so `useGpuix()` works in it.
        onRuntime: (runtime) => ROOT_BY_RUNTIME.set(runtime, host),
      })
      host.commit()
    },
    unmount() {
      stop()
      host.teardown()
      eventHandlers.clear()
      selectionListeners.clear()
      detach()
      host.commit()
    },
    renderer,
  }
}
