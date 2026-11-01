import type { MutationHost, MutationRenderer } from "./host.js"

export type MutationValue = number | string | boolean | object | null
export type MutationTuple = [string, ...MutationValue[]]

let appliedBatches = 0

/**
 * How many batches of mutations have reached a renderer, across every queue.
 * It moves whenever the tree changes, which makes it the signal that an app is
 * still working: `collectGarbageWhenIdle` waits for it to hold still.
 */
export function appliedBatchCount(): number {
  return appliedBatches
}

/** Atomic mutation queue shared by all framework adapters. */
export interface MutationQueue extends MutationRenderer {
  readonly pending: number
}

export function createMutationQueue(
  renderer: MutationHost,
  onDestroyed: (ids: readonly number[]) => void = () => {},
  scheduleFlush?: (flush: () => void) => void
): MutationQueue {
  let queue: MutationTuple[] = []
  let flushScheduled = false

  const enqueue = (mutation: MutationTuple) => {
    queue.push(mutation)
    if (!scheduleFlush || flushScheduled) return
    flushScheduled = true
    scheduleFlush(() => {
      if (!flushScheduled) return
      flushScheduled = false
      mutations.flushMutations()
    })
  }

  const mutations: MutationQueue = {
    get pending() {
      return queue.length
    },
    createElement(id, elementType) {
      enqueue(["createElement", id, elementType])
    },
    destroyElement(id) {
      enqueue(["destroyElement", id])
      return []
    },
    appendChild(parentId, childId) {
      enqueue(["appendChild", parentId, childId])
    },
    insertBefore(parentId, childId, beforeId) {
      enqueue(["insertBefore", parentId, childId, beforeId])
    },
    setStyle(id, style) {
      enqueue(["setStyle", id, style])
    },
    setText(id, content) {
      enqueue(["setText", id, content])
    },
    setEventListener(id, eventType, hasHandler) {
      enqueue(["setEventListener", id, eventType, hasHandler])
    },
    setRoot(id) {
      enqueue(["setRoot", id])
    },
    setKeyboardFocusDim(enabled) {
      enqueue(["setKeyboardFocusDim", enabled])
    },
    setCustomProp(id, key, value) {
      enqueue(["setCustomProp", id, key, value])
    },
    flushMutations() {
      flushScheduled = false
      if (queue.length === 0) return
      appliedBatches++
      const destroyed = renderer.applyBatch(JSON.stringify(queue))
      onDestroyed(destroyed)
      queue = []
    },
  }
  return mutations
}

const HANDLER_PROP_NAME = /^on[A-Z]/

/**
 * Event type a prop listens for, or undefined when the prop is data.
 *
 * Any function named `on` + uppercase is a handler: `onToggleFile` listens for
 * `toggleFile`. No list of names, so an element added with
 * `register_custom_element` can take `onGreet`. Rust drops events an element
 * does not support. A non-function stays data, so `onTop={true}` still reaches
 * the element's `set_prop`.
 */
export function handlerEventType(name: string, value: unknown): string | undefined {
  if (typeof value !== "function" || !HANDLER_PROP_NAME.test(name)) return undefined
  return name[2].toLowerCase() + name.slice(3)
}

export const RESERVED_PROPS: ReadonlySet<string> = new Set([
  "style",
  "className",
  "children",
  "key",
  "ref",
])

export const BUILT_IN_TYPES: ReadonlySet<string> = new Set(["div", "text"])

/**
 * Props every element forwards to Rust as custom props, beyond events. Any
 * `aria-*` attribute qualifies: the native accessibility mapper is the single
 * source of truth for which ones it understands, and it ignores the rest, so
 * the adapters do not keep a second list to drift.
 */
export const UNIVERSAL_PROPS: ReadonlySet<string> = new Set([
  "autoFocus",
  "tabIndex",
  "motion",
  "testId",
  "highlight",
  "role",
])

export function isUniversalProp(name: string): boolean {
  return name.startsWith("aria-") || UNIVERSAL_PROPS.has(name)
}

export function isReservedProp(name: string, value: unknown): boolean {
  return RESERVED_PROPS.has(name) || handlerEventType(name, value) !== undefined
}

/**
 * Structural equality for the JSON values a mutation carries: styles and
 * custom props. A key whose value is `undefined` counts as absent, because
 * `JSON.stringify` drops it, so `{ color: undefined }` and `{}` reach Rust as
 * the same payload and must compare equal here.
 *
 * Only plain objects, arrays and primitives are compared. A function or class
 * instance is equal to itself and nothing else.
 */
export function sameMutationValue(left: unknown, right: unknown): boolean {
  if (left === right) return true
  // NaN serializes to null on both sides, and -0 to 0, so neither is a change.
  if (typeof left === "number" && typeof right === "number") return left !== left && right !== right
  if (typeof left !== "object" || typeof right !== "object" || left === null || right === null) {
    return false
  }
  if (Array.isArray(left)) {
    if (!Array.isArray(right) || left.length !== right.length) return false
    for (let index = 0; index < left.length; index++) {
      if (!sameMutationValue(left[index], right[index])) return false
    }
    return true
  }
  if (Array.isArray(right)) return false
  const leftProto = Object.getPrototypeOf(left)
  if (leftProto !== Object.prototype && leftProto !== null) return false
  if (Object.getPrototypeOf(right) !== leftProto) return false
  const leftRecord = left as Record<string, unknown>
  const rightRecord = right as Record<string, unknown>
  let defined = 0
  for (const key in leftRecord) {
    const value = leftRecord[key]
    if (value === undefined) continue
    defined++
    if (!sameMutationValue(value, rightRecord[key])) return false
  }
  let rightDefined = 0
  for (const key in rightRecord) {
    if (rightRecord[key] !== undefined) rightDefined++
  }
  return defined === rightDefined
}

export function serializeCustomProp(
  value: object | string | number | boolean | null | undefined
): string | object | number | boolean | null {
  if (value === undefined || typeof value === "function") return null
  return value
}
