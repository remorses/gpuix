/** Semi-implicit Euler spring. Runs on the GPUIX frame loop, not CSS tweens. */

export type SpringTrack = { pos: number; vel: number }

export type SpringChannelKind = "px" | "opacity"

export const SPRING_KEYS = [
  "width",
  "height",
  "opacity",
  "top",
  "right",
  "bottom",
  "left",
  "borderRadius",
] as const

export type SpringKey = (typeof SPRING_KEYS)[number]

/** Soft GELATIN crawls; the budget forces rest so `onFrame` can park. */
export const SETTLE_BUDGET_MS = 280
export const SETTLE_HARD_MS = 420

const SNAP_POS_PX = 1.05
const SNAP_VEL_PX = 0.35
const CRAWL_POS_PX = 2.25
/** Hard settle only kills leftover crawl, not an in-flight travel. */
const HARD_CRAWL_POS_PX = 8
const SNAP_POS_OPACITY = 0.002
const SNAP_VEL_OPACITY = 0.02
const CRAWL_POS_OPACITY = 0.02
const HARD_CRAWL_POS_OPACITY = 0.08
const OPACITY_PUBLISH_EPS = 0.002

export function stepSpring(
  track: SpringTrack,
  target: number,
  dt: number,
  stiffness: number,
  damping: number,
  mass: number,
  rest = 0.05
): SpringTrack {
  const clamped = Math.min(Math.max(dt, 0), 0.032)
  const m = Math.max(mass, 0.001)
  const x = track.pos - target
  const accel = (-stiffness * x - damping * track.vel) / m
  const vel = track.vel + accel * clamped
  const pos = track.pos + vel * clamped
  if (Math.abs(pos - target) < rest && Math.abs(vel) < rest) {
    return { pos: target, vel: 0 }
  }
  return { pos, vel }
}

export function snapSpring(target: number): SpringTrack {
  return { pos: target, vel: 0 }
}

export function isSpringRest(track: SpringTrack, target: number): boolean {
  return track.vel === 0 && track.pos === target
}

export function springChannelKind(key: SpringKey): SpringChannelKind {
  return key === "opacity" ? "opacity" : "px"
}

export function shouldSnapSpring(
  track: SpringTrack,
  target: number,
  elapsedMs = 0,
  kind: SpringChannelKind = "px"
): boolean {
  const dist = Math.abs(track.pos - target)
  const speed = Math.abs(track.vel)
  if (kind === "opacity") {
    if (speed < SNAP_VEL_OPACITY && dist < SNAP_POS_OPACITY) return true
    if (elapsedMs >= SETTLE_BUDGET_MS && dist < CRAWL_POS_OPACITY) return true
    return elapsedMs >= SETTLE_HARD_MS && dist < HARD_CRAWL_POS_OPACITY
  }
  if (speed < SNAP_VEL_PX && dist < SNAP_POS_PX) return true
  if (elapsedMs >= SETTLE_BUDGET_MS && dist < CRAWL_POS_PX) return true
  return elapsedMs >= SETTLE_HARD_MS && dist < HARD_CRAWL_POS_PX
}

export function springShouldPublish(
  previous: number,
  next: number,
  kind: SpringChannelKind = "px"
): boolean {
  if (kind === "opacity") return Math.abs(next - previous) >= OPACITY_PUBLISH_EPS
  return Math.round(next) !== Math.round(previous)
}

export function quantizeSpringValue(value: number, kind: SpringChannelKind = "px"): number {
  if (kind === "opacity") return value
  return Math.round(value)
}

export function animateSignature(style: Partial<Record<SpringKey, number>>): string {
  return SPRING_KEYS.map((key) => `${key}:${style[key] ?? ""}`).join("|")
}

export function seedSpringTrack(
  tracks: Partial<Record<SpringKey, SpringTrack>>,
  key: SpringKey,
  from: number | undefined,
  to: number,
  kick = 0
): SpringTrack {
  const existing = tracks[key]
  if (existing) return existing
  const track = { pos: from ?? to, vel: kick }
  tracks[key] = track
  return track
}

export function allSpringChannelsRest(
  tracks: Partial<Record<SpringKey, SpringTrack>>,
  target: Partial<Record<SpringKey, number>>
): boolean {
  for (const key of SPRING_KEYS) {
    const to = target[key]
    if (to == null) continue
    const track = tracks[key]
    if (!track || !isSpringRest(track, to)) return false
  }
  return true
}

export function stepSpringLease(opts: {
  tracks: Partial<Record<SpringKey, SpringTrack>>
  target: Partial<Record<SpringKey, number>>
  painted: Partial<Record<SpringKey, number>>
  dt: number
  elapsedMs: number
  stiffness: number
  damping: number
  mass: number
  kick?: number
}): {
  painted: Partial<Record<SpringKey, number>>
  moving: boolean
  publish: boolean
} {
  const { tracks, target, dt, elapsedMs, stiffness, damping, mass, kick = 0 } = opts
  let moving = false
  let publish = false
  const painted = { ...opts.painted }
  for (const key of SPRING_KEYS) {
    const to = target[key]
    if (to == null) continue
    const kind = springChannelKind(key)
    const rest = kind === "opacity" ? SNAP_POS_OPACITY : 0.05
    let track = seedSpringTrack(tracks, key, painted[key], to, kick)
    track = stepSpring(track, to, dt, stiffness, damping, mass, rest)
    if (shouldSnapSpring(track, to, elapsedMs, kind)) track = snapSpring(to)
    tracks[key] = track
    if (!isSpringRest(track, to)) moving = true
    const visual = isSpringRest(track, to) ? to : quantizeSpringValue(track.pos, kind)
    const previous = painted[key]
    if (previous == null || springShouldPublish(previous, visual, kind)) {
      if (previous !== visual) {
        painted[key] = visual
        publish = true
      }
    }
  }
  return { painted, moving, publish }
}

export type FrameListener = (dt: number, now: number) => void

const listeners = new Set<FrameListener>()

export function onFrame(listener: FrameListener): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

/**
 * Subscribe a spring tick. Unsubscribes itself when the tick returns false
 * (every channel at rest), matching Automaton `subscribeSpringTick`.
 */
export function subscribeSpringTick(tick: (dt: number, now: number) => boolean): () => void {
  const listener: FrameListener = (dt, now) => {
    if (!tick(dt, now)) listeners.delete(listener)
  }
  return onFrame(listener)
}

export function springClockBusy(): boolean {
  return listeners.size > 0
}

export function resetSpringClockForTests(): void {
  listeners.clear()
}

export function pumpFrames(dt: number, now: number): void {
  for (const listener of [...listeners]) listener(dt, now)
}

export const GELATIN = { stiffness: 28, damping: 8, mass: 1.25, velocity: 0 }
