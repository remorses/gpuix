// GPUIX component definitions and native motion wrappers.

import { createElement, forwardRef, useContext, useEffect, useMemo, useRef, useState } from "react"
import type { ReactElement, ReactNode } from "react"
import type {
  MotionProps,
  MotionSpringTransition,
  MotionStyle,
  Props,
  PublicInstance,
  StyleDesc,
} from "../types/host.js"
import {
  allSpringChannelsRest,
  animateSignature,
  GELATIN,
  seedSpringTrack,
  SPRING_KEYS,
  stepSpringLease,
  subscribeSpringTick,
  type SpringKey,
  type SpringTrack,
} from "../motion-spring.js"
import { PresenceContext, usePresence } from "./animate-presence.js"

let nextMotionGeneration = 0

function motionStyleKey(style: MotionProps["animate"] | false | undefined) {
  if (!style) return style
  return [
    style.width,
    style.height,
    style.opacity,
    style.top,
    style.right,
    style.bottom,
    style.left,
    style.borderRadius,
  ]
}

export {
  AnimatePresence,
  PresenceContext,
  useIsPresent,
  usePresence,
} from "./animate-presence.js"
export type { AnimatePresenceProps } from "./animate-presence.js"

export const gpuixComponents = {
  div: "div",
  text: "text",
  img: "img",
  svg: "svg",
  canvas: "canvas",
  input: "input",
  textarea: "textarea",
  anchored: "anchored",
  "virtual-list": "virtual-list",
} as const

export type GpuixComponentType = keyof typeof gpuixComponents

export interface MotionDivProps extends MotionProps {
  children?: ReactNode
  style?: StyleDesc
  onClick?: Props["onClick"]
  onMouseDown?: Props["onMouseDown"]
  onMouseUp?: Props["onMouseUp"]
  onMouseEnter?: Props["onMouseEnter"]
  onMouseLeave?: Props["onMouseLeave"]
  onMouseMove?: Props["onMouseMove"]
  onMouseDownOutside?: Props["onMouseDownOutside"]
  onKeyDown?: Props["onKeyDown"]
  onKeyUp?: Props["onKeyUp"]
  onFocus?: Props["onFocus"]
  onBlur?: Props["onBlur"]
  onScroll?: Props["onScroll"]
  onFileDrop?: Props["onFileDrop"]
  onMotionComplete?: Props["onMotionComplete"]
  autoFocus?: boolean
}

function isSpringTransition(
  transition: MotionProps["transition"]
): transition is MotionSpringTransition {
  return transition != null && transition.type === "spring"
}

function readStyle(style: MotionStyle | false | undefined, key: SpringKey): number | undefined {
  if (style == null || style === false) return undefined
  return style[key]
}

const MotionDiv = forwardRef<PublicInstance, MotionDivProps>(function MotionDiv(
  { initial, animate, exit, transition, onMotionComplete, style, ...props },
  ref
): ReactElement {
  const presence = useContext(PresenceContext)
  const [isPresent, safeToRemove] = usePresence()
  const resolvedInitial = presence?.initial === false ? false : initial
  const resolvedAnimate = !isPresent && exit ? exit : animate
  const spring = isSpringTransition(transition)

  useEffect(() => {
    if (!isPresent && !exit) safeToRemove?.()
  }, [exit, isPresent, safeToRemove])

  const animateRef = useRef(resolvedAnimate)
  animateRef.current = resolvedAnimate
  const [current, setCurrent] = useState<MotionStyle>(() => {
    const seed: MotionStyle = {}
    for (const key of SPRING_KEYS) {
      const value = readStyle(resolvedInitial, key) ?? resolvedAnimate[key]
      if (value != null) seed[key] = value
    }
    return seed
  })
  const tracks = useRef<Partial<Record<SpringKey, SpringTrack>>>({})
  const paintedRef = useRef(current)
  const transitionRef = useRef(transition)
  transitionRef.current = transition
  const exitingRef = useRef<(() => void) | null>(null)
  // Springs are JS-driven, so an exit completes when the lease parks at rest
  // rather than on a native motionComplete event.
  exitingRef.current = spring && !isPresent && exit ? (safeToRemove ?? null) : null
  const targetSignature = spring ? `${isPresent}|${animateSignature(resolvedAnimate)}` : ""

  useEffect(() => {
    if (!spring) return
    const spec = transitionRef.current
    const kick = isSpringTransition(spec) ? (spec.velocity ?? 0) : 0
    const target = animateRef.current
    for (const key of SPRING_KEYS) {
      const to = target[key]
      if (to == null) continue
      seedSpringTrack(tracks.current, key, paintedRef.current[key], to, kick)
    }
    if (allSpringChannelsRest(tracks.current, target)) {
      exitingRef.current?.()
      return
    }

    let elapsedMs = 0
    return subscribeSpringTick((dt) => {
      const live = transitionRef.current
      if (!isSpringTransition(live)) return false
      elapsedMs += dt * 1000
      const result = stepSpringLease({
        tracks: tracks.current,
        target: animateRef.current,
        painted: paintedRef.current,
        dt,
        elapsedMs,
        stiffness: live.stiffness ?? GELATIN.stiffness,
        damping: live.damping ?? GELATIN.damping,
        mass: live.mass ?? GELATIN.mass,
        kick: live.velocity ?? 0,
      })
      if (result.publish) {
        paintedRef.current = result.painted
        setCurrent(result.painted)
      }
      if (!result.moving) exitingRef.current?.()
      return result.moving
    })
  }, [spring, targetSignature])

  const motionKey = JSON.stringify([
    isPresent,
    motionStyleKey(resolvedInitial),
    motionStyleKey(resolvedAnimate),
    transition ?? null,
  ])
  const generation = useMemo(() => ++nextMotionGeneration, [motionKey])

  if (spring) {
    const hostProps: Props = {
      ...props,
      ref,
      style: {
        ...(style ?? {}),
        ...(current.width != null ? { width: current.width } : {}),
        ...(current.height != null ? { height: current.height } : {}),
        ...(current.opacity != null ? { opacity: current.opacity } : {}),
        ...(current.top != null ? { top: current.top } : {}),
        ...(current.right != null ? { right: current.right } : {}),
        ...(current.bottom != null ? { bottom: current.bottom } : {}),
        ...(current.left != null ? { left: current.left } : {}),
        ...(current.borderRadius != null ? { borderRadius: current.borderRadius } : {}),
      },
    }
    return createElement("div", hostProps)
  }

  const motionDescription = {
    generation,
    isExit: !isPresent && exit !== undefined,
    initial: resolvedInitial,
    animate: resolvedAnimate,
    transition,
  }
  const hostProps: Props = {
    ...props,
    ref,
    style,
    motion: motionDescription,
  }
  if (!isPresent || onMotionComplete) {
    hostProps.onMotionComplete = (event) => {
      if (event.motionGeneration !== generation) return
      onMotionComplete?.(event)
      if (!isPresent) safeToRemove?.()
    }
  }
  return createElement("div", hostProps)
})

/** Native animations with a Motion-like declarative React API. */
export const motion = {
  div: MotionDiv,
} as const

// There is no `VirtualList` React wrapper. Windowing on the React side is the
// app's job: pass `itemCount`, `estimatedItemHeight` and `windowStart` to the
// host `<virtual-list>` and render only that slice. A generic wrapper cannot
// know when to widen its own window, so it silently dropped rows whenever
// `itemCount` grew without a scroll.
