/// BatchingRenderer — buffers individual napi mutation calls into a single
/// applyBatch() FFI call, reducing N FFI boundary crossings to 1 per commit.
///
/// Implemented as a JS Proxy: mutation method calls on the NativeRenderer are
/// captured as ["methodName", ...args] in a queue. On commitMutations(), the
/// entire queue is flushed via applyBatch(json).
///
/// Adding a new mutation method to NativeRenderer requires adding it to
/// BATCHED_METHODS below — nothing else.
///
/// ## Batch timing
///
/// The batch boundary is React's commit phase (synchronous):
///
///   setState() → React render → reconciler mutation callbacks → resetAfterCommit()
///                                ↓ each callback queues ops     ↓ flushes queue
///                                queue.push([name, ...args])    applyBatch(json)
///
/// Multiple setState calls batched by React into one render = one batch.
/// Multiple separate commits in the same event loop tick = multiple batches.
///
/// ## Render-phase caveat (concurrent mode)
///
/// React's createInstance / createTextInstance / appendInitialChild are called
/// during the RENDER phase, not the commit phase. In concurrent mode, React
/// can abandon a render and retry. Mutations from abandoned renders stay in
/// the queue and get flushed with the next successful commit. This can create
/// orphaned elements in the Rust retained tree.
///
/// This is a pre-existing issue (before batching, these calls went directly
/// to native and could also leave orphaned elements). Batching doesn't make
/// it worse. The proper fix is moving element creation to commit-phase
/// callbacks, but that requires a larger reconciler refactor.

import type { NativeRenderer } from "../types/host"
import { unregisterEventHandlers } from "./event-registry"

export type MutationTuple = (number | string | boolean)[]

/// Methods that should be batched (queued instead of called immediately).
/// Any method NOT in this set is passed through to the inner renderer directly.
/// This prevents accidental queuing of getters, queries, or future non-mutation
/// methods that would return undefined and enqueue garbage ops.
const BATCHED_METHODS = new Set([
  "createElement",
  "appendChild",
  "removeChild",
  "insertBefore",
  "setStyle",
  "setText",
  "setEventListener",
  "setRoot",
  "setCustomProp",
])

/**
 * Wrap a NativeRenderer with batching support.
 *
 * If the inner renderer has applyBatch(), returns a Proxy that buffers
 * all mutation calls and flushes them in one applyBatch() per React commit.
 * Otherwise returns the inner renderer unchanged.
 */
export function wrapWithBatching(inner: NativeRenderer): NativeRenderer {
  if (typeof inner.applyBatch !== "function") return inner

  const batchable = inner as NativeRenderer & { applyBatch(json: string): number[] }
  let queue: MutationTuple[] = []

  return new Proxy(inner, {
    get(_target, prop: string) {
      // commitMutations: flush the queue via a single applyBatch() FFI call.
      // Called by resetAfterCommit() at the end of React's commit phase.
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

      // Batched mutation methods: queue as [methodName, ...args].
      if (BATCHED_METHODS.has(prop)) {
        return (...args: any[]) => {
          queue.push([prop, ...args])
        }
      }

      // Everything else (getters, queries, applyBatch, future methods):
      // pass through to the inner renderer directly.
      const value = (batchable as any)[prop]
      if (typeof value === "function") {
        return value.bind(batchable)
      }
      return value
    },
  }) as NativeRenderer
}
