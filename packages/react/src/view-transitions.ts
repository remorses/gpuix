// The View Transitions API: capture the named elements, apply the React
// update synchronously, then animate each name from its old place to its
// new one. The native renderer owns the animation, so React renders once.

import { flushSync } from "./reconciler/reconciler.js"
import type { MotionEase, NativeRenderer } from "./types/host.js"

/** A translation distance: pixels as a number or "Npx", or a share of the
 *  element's size as "N%". */
export type ViewTransitionLength = number | string

/** What one side of a pair does over the transition. Every field is a
 *  `[from, to]` pair. A missing field holds still. */
export interface ViewTransitionSide {
  translateX?: [ViewTransitionLength, ViewTransitionLength]
  translateY?: [ViewTransitionLength, ViewTransitionLength]
  opacity?: [number, number]
  /** A `filter: blur()` sigma in pixels. */
  blur?: [number, number]
  /** Paint this side over the other one. Only read on `old`. */
  onTop?: boolean
}

export interface ViewTransitionGroupOptions {
  /** Seconds, like the `motion` prop. The default is 0.3. */
  duration?: number
  /** Seconds before this group starts. */
  delay?: number
  ease?: MotionEase
  /** The element that leaves. When a group gives neither `old` nor `new`,
   *  the pair crossfades. */
  old?: ViewTransitionSide
  /** The element that arrives. */
  new?: ViewTransitionSide
}

export interface ViewTransitionOptions {
  /** Seconds, for every group that does not set its own. */
  duration?: number
  delay?: number
  ease?: MotionEase
  /** Options per `viewTransitionName`. A name with no entry crossfades. */
  groups?: Record<string, ViewTransitionGroupOptions>
}

/**
 * Run `update` and animate every element that carries a
 * `viewTransitionName` from its place before the update to its place after
 * it. Give the leaving screen and the arriving screen the same name to
 * animate a navigation as a pair. A name that only leaves paints a frozen
 * copy over the tree while its group's `old` side runs, without the clip
 * of its former ancestors.
 *
 * On a renderer without the native methods, this runs `update` alone.
 */
export function startViewTransition(
  renderer: NativeRenderer,
  update: () => void,
  options?: ViewTransitionOptions,
): void {
  if (!renderer.viewTransitionCapture || !renderer.viewTransitionStart) {
    update()
    return
  }
  renderer.viewTransitionCapture()
  flushSync(update)
  renderer.viewTransitionStart(JSON.stringify(options ?? {}))
}
