import {
  createTextSearchController,
  observeWindowInsets,
  observeWindowSize,
  readWindowInsets,
  readWindowSize,
} from "@gpuix/native/host"
import type {
  ObserverOptions,
  TextSearchOptions,
  TextSearchSnapshot,
  WindowInsets,
  WindowSize,
} from "@gpuix/native/host"
import { atom, watch } from "ilha"
import type { AtomHandle } from "ilha"

import { useGpuix, useGpuixRequired } from "./root.js"

/**
 * The window size as an atom, sampled every 100ms by default.
 *
 * Like every ilha primitive it is called in the component body; reading the
 * atom there repaints the component when the window resizes.
 */
export function windowSize(options: ObserverOptions = {}): AtomHandle<WindowSize> {
  const renderer = useGpuixRequired()
  const size = atom.lazy(() => readWindowSize(renderer))
  watch.once(() =>
    observeWindowSize(
      renderer,
      (next) => {
        size.set(next)
      },
      options
    )
  )
  return size
}

/** Safe-area and keyboard geometry as an atom, sampled every 100ms by default. */
export function windowInsets(
  options: ObserverOptions = {}
): AtomHandle<WindowInsets> {
  const renderer = useGpuixRequired()
  const insets = atom.lazy(() => readWindowInsets(renderer))
  watch.once(() =>
    observeWindowInsets(
      renderer,
      (next) => {
        insets.set(next)
      },
      options
    )
  )
  return insets
}

/** The window's selected text as an atom; `null` when nothing is selected. */
export function selectedText(): AtomHandle<string | null> {
  const context = useGpuix()
  if (!context) {
    throw new Error("selectedText must be used inside a GPUIX root")
  }
  const text = atom.lazy(() => context.renderer.getSelectedText?.() ?? null)
  watch.once(() =>
    context.subscribeSelection((next) => {
      text.set(next)
    })
  )
  return text
}

export interface TextSearch extends TextSearchSnapshot {
  next(): void
  previous(): void
  goTo(index: number): void
}

/**
 * Find-in-page state for a subtree. Spread `props` onto the element to search;
 * `total` and `active` are the match count and the cursor.
 *
 * ```tsx
 * const query = atom("")
 * const search = textSearch({ query: query() })
 * return <div {...search.props}>…</div>
 * ```
 */
export function textSearch(options: TextSearchOptions): TextSearch {
  const controller = atom.lazy(createTextSearchController)()
  const revision = atom(0)
  watch.once(() =>
    controller.subscribe(() => {
      revision.update((value: number) => value + 1)
    })
  )
  // Read in render: a match count or cursor change repaints the component.
  revision()
  return {
    ...controller.getSnapshot(options),
    goTo: (index) => controller.goTo(index),
    next: () => controller.next(),
    previous: () => controller.previous(),
  }
}
