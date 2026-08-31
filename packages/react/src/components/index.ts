// GPUIX component definitions and native motion wrappers.

import { createElement, forwardRef, useEffect, useRef, useState } from "react"
import type { ReactElement, ReactNode } from "react"
import type {
  MotionProps,
  MotionSpringTransition,
  MotionStyle,
  Props,
  PublicInstance,
  StyleDesc,
} from "../types/host.js"
import { GELATIN, onFrame, stepSpring, type SpringTrack } from "../motion-spring.js"

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
  autoFocus?: boolean
}

const SPRING_KEYS = [
  "width",
  "height",
  "opacity",
  "top",
  "right",
  "bottom",
  "left",
  "borderRadius",
] as const

type SpringKey = (typeof SPRING_KEYS)[number]

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
  { initial, animate, transition, style, ...props },
  ref
): ReactElement {
  const spring = isSpringTransition(transition)
  const animateRef = useRef(animate)
  animateRef.current = animate
  const [current, setCurrent] = useState<MotionStyle>(() => {
    const seed: MotionStyle = {}
    for (const key of SPRING_KEYS) {
      const value = readStyle(initial, key) ?? animate[key]
      if (value != null) seed[key] = value
    }
    return seed
  })
  const tracks = useRef<Partial<Record<SpringKey, SpringTrack>>>({})
  const transitionRef = useRef(transition)
  transitionRef.current = transition

  useEffect(() => {
    if (!spring) return
    return onFrame((dt) => {
      const spec = transitionRef.current
      if (!isSpringTransition(spec)) return
      const stiffness = spec.stiffness ?? GELATIN.stiffness
      const damping = spec.damping ?? GELATIN.damping
      const mass = spec.mass ?? GELATIN.mass
      const kick = spec.velocity ?? 0
      const target = animateRef.current
      let changed = false
      const next: MotionStyle = {}
      for (const key of SPRING_KEYS) {
        const to = target[key]
        if (to == null) continue
        const rest = key === "opacity" ? 0.002 : 0.05
        let track = tracks.current[key]
        if (!track) track = { pos: to, vel: kick }
        const stepped = stepSpring(track, to, dt, stiffness, damping, mass, rest)
        if (stepped.pos !== track.pos || stepped.vel !== track.vel) changed = true
        tracks.current[key] = stepped
        next[key] = stepped.pos
      }
      if (changed) setCurrent(next)
    })
  }, [spring])

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

  const hostProps: Props = {
    ...props,
    ref,
    style,
    motion: {
      ...(initial === undefined ? {} : { initial }),
      animate,
      ...(transition === undefined ? {} : { transition }),
    },
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
