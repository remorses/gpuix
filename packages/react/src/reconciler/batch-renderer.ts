/// BatchingRenderer — buffers individual napi mutation calls into a single
/// applyBatch() FFI call, reducing N FFI boundary crossings to 1 per commit.
///
/// Implemented as a JS Proxy: any method call on the NativeRenderer is
/// automatically captured as ["methodName", ...args] in a queue. On
/// commitMutations(), the entire queue is flushed via applyBatch(json).
///
/// Adding a new method to NativeRenderer requires ZERO changes here —
/// the Proxy captures it automatically.

import type { NativeRenderer } from "../types/host"
import { unregisterEventHandlers } from "./event-registry"

export type MutationTuple = (number | string | boolean)[]

/**
 * Wrap a NativeRenderer with batching support.
 * If the inner renderer has applyBatch(), returns a Proxy that buffers
 * all mutation calls and flushes them in one applyBatch() per commit.
 * Otherwise returns the inner renderer unchanged.
 */
export function wrapWithBatching(inner: NativeRenderer): NativeRenderer {
  if (typeof inner.applyBatch !== "function") return inner

  const batchable = inner as NativeRenderer & { applyBatch(json: string): number[] }
  let queue: MutationTuple[] = []

  return new Proxy(inner, {
    get(_target, prop: string) {
      // commitMutations: flush the queue via applyBatch.
      if (prop === "commitMutations") {
        return () => {
          if (queue.length === 0) {
            batchable.commitMutations()
            return
          }

          const json = JSON.stringify(queue)

          // applyBatch may throw on malformed ops — queue is preserved
          // on failure so state doesn't desync between JS and Rust.
          const destroyedIds = batchable.applyBatch(json)

          // Clean up JS-side event handlers immediately after successful batch.
          for (const id of destroyedIds) {
            unregisterEventHandlers(id)
          }

          // Clear queue only AFTER successful batch + cleanup.
          queue = []

          // Trigger commit side effects (e.g. needs_redraw flag).
          batchable.commitMutations()
        }
      }

      // destroyElement: queue the op, return [] (destroyed IDs come from applyBatch).
      if (prop === "destroyElement") {
        return (id: number): Array<number> => {
          queue.push(["destroyElement", id])
          return []
        }
      }

      // applyBatch: pass through for direct access.
      if (prop === "applyBatch") {
        return batchable.applyBatch.bind(batchable)
      }

      // All other methods: auto-capture as [methodName, ...args].
      return (...args: any[]) => {
        queue.push([prop, ...args])
      }
    },
  }) as NativeRenderer
}
