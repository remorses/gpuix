import { atom, context, createContext, watch } from "ilha"
import type { View } from "ilha"

import { el } from "./floating.js"

export interface PresenceContextValue {
  /** False once the child left `AnimatePresence` and is animating out. */
  isPresent(): boolean
  /** `false` on the first render under `<AnimatePresence initial={false}>`. */
  initial: false | undefined
  /** Tell `AnimatePresence` the child finished leaving and can be removed. */
  safeToRemove(): void
  /** A component that will call `safeToRemove` announces itself here. */
  register(): () => void
}

export const PresenceContext = createContext<PresenceContextValue | undefined>(
  undefined
)

const ALWAYS_PRESENT: [boolean, () => void] = [true, () => undefined]

/**
 * Whether this component is still present, and the function to call once its
 * exit is done. While a component uses this, `AnimatePresence` keeps it
 * mounted after it leaves, until `safeToRemove` runs.
 */
export function usePresence(): [isPresent: boolean, safeToRemove: () => void] {
  const presence = context(PresenceContext)()
  watch.once(() => presence?.register())
  return presence
    ? [presence.isPresent(), presence.safeToRemove]
    : ALWAYS_PRESENT
}

export function useIsPresent(): boolean {
  return context(PresenceContext)()?.isPresent() ?? true
}

export interface AnimatePresenceProps {
  children?: View
  /** `false` skips the `initial` animation of the children present at mount. */
  initial?: boolean
  /** Runs once every leaving child has been removed. */
  onExitComplete?: () => void
}

/** One child, present or leaving, by key. */
interface Entry {
  key: string
  view: View
  present: boolean
  consumers: number
  context: PresenceContextValue
}

/** Flatten a children value, dropping the views ilha skips. */
function childList(children: View, into: View[] = []): View[] {
  if (Array.isArray(children)) {
    for (const child of children) {
      childList(child, into)
    }
  } else if (
    children !== null &&
    children !== undefined &&
    typeof children !== "boolean"
  ) {
    into.push(children)
  }
  return into
}

/**
 * The child without its key. The wrapper carries the key. Inside it the child
 * is positional, so it reruns whenever the wrapper repaints: that is how it
 * learns it is leaving, which changes its presence and none of its props.
 */
const unkeyed = (view: View): View =>
  typeof view === "object" &&
  view !== null &&
  (view as { $$ilha?: unknown }).$$ilha === 1 &&
  (view as { key?: unknown }).key !== undefined
    ? ({ ...(view as object), key: undefined } as View)
    : view

const keyOf = (view: View, index: number): string => {
  const key =
    typeof view === "object" && view !== null
      ? (view as { key?: string | number | null }).key
      : undefined
  return key === null || key === undefined ? `$${index}` : String(key)
}

/**
 * Holds one child and gives it its presence. `view` and `present` are props so
 * that ilha reruns the wrapper, and with it the child, when either changes.
 */
function PresenceChild(props: {
  entry: Entry
  view: View
  present: boolean
}): View {
  return el(
    PresenceContext.Provider,
    { value: props.entry.context },
    props.view
  )
}

/**
 * Keeps a child mounted after it leaves the tree, until its exit animation is
 * done. Children are matched by `key`; give each one a key when there is more
 * than one. A `motion.div` with an `exit` target animates out on its own.
 */
export function AnimatePresence(props: AnimatePresenceProps): View {
  const revision = atom(0)
  revision()
  const state = atom.lazy(() => ({
    entries: new Map<string, Entry>(),
    first: true,
    props,
  }))()
  state.props = props
  const repaint = () => revision.update((value: number) => value + 1)

  const remove = (entry: Entry) => {
    if (entry.present || state.entries.get(entry.key) !== entry) {
      return
    }
    state.entries.delete(entry.key)
    repaint()
    if (![...state.entries.values()].some((other) => !other.present)) {
      state.props.onExitComplete?.()
    }
  }

  const current = childList(props.children)
  const seen = new Set<string>()
  const order: Entry[] = []
  current.forEach((view, index) => {
    const key = keyOf(view, index)
    seen.add(key)
    let entry = state.entries.get(key)
    if (!entry) {
      const created: Entry = {
        consumers: 0,
        context: {
          initial: state.first && props.initial === false ? false : undefined,
          isPresent: () => created.present,
          register() {
            created.consumers += 1
            return () => {
              created.consumers -= 1
            }
          },
          safeToRemove: () => remove(created),
        },
        key,
        present: true,
        view: unkeyed(view),
      }
      entry = created
      state.entries.set(key, entry)
    }
    entry.view = unkeyed(view)
    entry.present = true
    order.push(entry)
  })
  for (const entry of state.entries.values()) {
    if (seen.has(entry.key)) {
      continue
    }
    if (entry.present) {
      entry.present = false
      if (entry.consumers === 0) {
        // Nothing will animate it out: it goes with this render.
        state.entries.delete(entry.key)
        if (![...state.entries.values()].some((other) => !other.present)) {
          queueMicrotask(() => state.props.onExitComplete?.())
        }
        continue
      }
    }
    order.push(entry)
  }
  state.first = false
  return order.map((entry) =>
    el(PresenceChild, {
      entry,
      key: entry.key,
      present: entry.present,
      view: entry.view,
    })
  )
}
