import type { MotionProps } from "@gpuix/native/host"
import { atom, context, watch } from "ilha"
import type { View } from "ilha"

import type { DivProps, Handler } from "../types.js"
import { PresenceContext } from "./animate-presence.js"
import { el, splitHandlers } from "./floating.js"

export interface MotionDivProps extends DivProps, MotionProps {}

/**
 * A `<div>` whose `animate` target is interpolated natively. A change of
 * target starts a new generation, and `onMotionComplete` only reports the
 * current one, so an interrupted animation never fires late.
 *
 * Inside `AnimatePresence` it animates to `exit` once it has left, then tells
 * `AnimatePresence` it can be removed.
 */
function MotionDiv(props: MotionDivProps): View {
  const { animate, children, exit, initial, transition, ...host } = props
  const presence = context(PresenceContext)()
  watch.once(() => presence?.register())
  const leaving = presence !== undefined && !presence.isPresent()
  const isExit = leaving && exit !== undefined
  if (leaving && !exit) {
    // No exit animation to wait for. Not during this render: it repaints
    // `AnimatePresence`.
    queueMicrotask(presence.safeToRemove)
  }
  const start = presence?.initial === false ? false : initial
  const target = isExit ? exit : animate
  const track = atom.lazy(() => ({
    generation: 0,
    target: undefined as string | undefined,
  }))()
  const targetKey = JSON.stringify([isExit, start, target, transition])
  if (targetKey !== track.target) {
    track.target = targetKey
    track.generation += 1
  }
  const { handlers, rest } = splitHandlers(host, ["motioncomplete"])
  return el(
    "div",
    {
      ...rest,
      motion: {
        animate: target,
        exit,
        generation: track.generation,
        initial: start,
        isExit,
        transition,
      },
      onMotionComplete:
        handlers.motioncomplete || presence
          ? (((event) => {
              if (event.motionGeneration !== track.generation) {
                return
              }
              handlers.motioncomplete?.(event)
              if (presence && !presence.isPresent()) {
                presence.safeToRemove()
              }
            }) satisfies Handler)
          : undefined,
    },
    children
  )
}

export const motion = { div: MotionDiv } as const
