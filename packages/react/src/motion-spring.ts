/** Semi-implicit Euler spring. Runs on the GPUIX frame loop, not CSS tweens. */

export type SpringTrack = { pos: number; vel: number }

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

export type FrameListener = (dt: number, now: number) => void

const listeners = new Set<FrameListener>()

export function onFrame(listener: FrameListener): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function pumpFrames(dt: number, now: number): void {
  for (const listener of listeners) listener(dt, now)
}

export const GELATIN = { stiffness: 28, damping: 8, mass: 1.25, velocity: 0 }
