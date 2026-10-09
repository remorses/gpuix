import { afterEach, describe, expect, it } from "vitest"
import {
  allSpringChannelsRest,
  GELATIN,
  isSpringRest,
  pumpFrames,
  quantizeSpringValue,
  resetSpringClockForTests,
  SETTLE_HARD_MS,
  shouldSnapSpring,
  snapSpring,
  springClockBusy,
  springShouldPublish,
  stepSpring,
  stepSpringLease,
  subscribeSpringTick,
  type SpringTrack,
} from "../motion-spring.js"

afterEach(() => {
  resetSpringClockForTests()
})

describe("stepSpring", () => {
  it("snaps to rest when inside the rest window", () => {
    const next = stepSpring({ pos: 100.02, vel: 0.01 }, 100, 1 / 60, 28, 8, 1.25)
    expect(next).toEqual({ pos: 100, vel: 0 })
  })

  it("overshoots with GELATIN then settles on the target", () => {
    let track: SpringTrack = { pos: 0, vel: 0 }
    let max = 0
    for (let i = 0; i < 240; i++) {
      track = stepSpring(
        track,
        100,
        1 / 60,
        GELATIN.stiffness,
        GELATIN.damping,
        GELATIN.mass
      )
      max = Math.max(max, track.pos)
    }
    expect(max).toBeGreaterThan(100)
    expect(track.pos).toBe(100)
    expect(track.vel).toBe(0)
  })
})

describe("spring lease parking", () => {
  it("skips no-op and sub-pixel publishes", () => {
    expect(springShouldPublish(12, 12)).toBe(false)
    expect(springShouldPublish(12, 12.4)).toBe(false)
    expect(springShouldPublish(12, 12.6)).toBe(true)
    expect(quantizeSpringValue(12.4)).toBe(12)
    expect(quantizeSpringValue(12.6)).toBe(13)
    expect(springShouldPublish(0.5, 0.5005, "opacity")).toBe(false)
    expect(springShouldPublish(0.5, 0.51, "opacity")).toBe(true)
  })

  it("snaps a crawl, not an in-flight travel", () => {
    expect(shouldSnapSpring({ pos: 10.3, vel: 0.02 }, 10)).toBe(true)
    expect(shouldSnapSpring({ pos: 18, vel: 2.4 }, 10)).toBe(false)
    expect(shouldSnapSpring({ pos: 68, vel: 40 }, 10, SETTLE_HARD_MS)).toBe(false)
    expect(shouldSnapSpring({ pos: 12.5, vel: 1.2 }, 10, SETTLE_HARD_MS)).toBe(true)
    expect(snapSpring(10)).toEqual({ pos: 10, vel: 0 })
    expect(isSpringRest(snapSpring(10), 10)).toBe(true)
  })

  it("parks onFrame when every channel is at rest", () => {
    const tracks = { width: { pos: 10.2, vel: 0.08 } }
    let painted = { width: 10 }
    expect(springClockBusy()).toBe(false)
    subscribeSpringTick((dt) => {
      const result = stepSpringLease({
        tracks,
        target: { width: 10 },
        painted,
        dt,
        elapsedMs: SETTLE_HARD_MS,
        stiffness: GELATIN.stiffness,
        damping: GELATIN.damping,
        mass: GELATIN.mass,
      })
      painted = result.painted
      return result.moving
    })
    expect(springClockBusy()).toBe(true)
    pumpFrames(1 / 60, 0)
    expect(isSpringRest(tracks.width, 10)).toBe(true)
    expect(allSpringChannelsRest(tracks, { width: 10 })).toBe(true)
    expect(springClockBusy()).toBe(false)
  })

  it("wakes on animate retarget", () => {
    const tracks = { width: snapSpring(10) }
    let painted = { width: 10 }
    let target = { width: 10 }
    const arm = () => {
      if (allSpringChannelsRest(tracks, target)) return
      return subscribeSpringTick((dt) => {
        const result = stepSpringLease({
          tracks,
          target,
          painted,
          dt,
          elapsedMs: 0,
          stiffness: GELATIN.stiffness,
          damping: GELATIN.damping,
          mass: GELATIN.mass,
        })
        painted = result.painted
        return result.moving
      })
    }

    arm()
    expect(springClockBusy()).toBe(false)

    target = { width: 80 }
    arm()
    expect(springClockBusy()).toBe(true)
    pumpFrames(1 / 60, 16)
    expect(tracks.width.pos).not.toBe(80)
    expect(tracks.width.vel).not.toBe(0)
  })

  it("does not publish when the visual is already settled", () => {
    const tracks = { width: { pos: 12.2, vel: 0.01 }, opacity: { pos: 1.0004, vel: 0 } }
    const result = stepSpringLease({
      tracks,
      target: { width: 12, opacity: 1 },
      painted: { width: 12, opacity: 1 },
      dt: 1 / 60,
      elapsedMs: 0,
      stiffness: GELATIN.stiffness,
      damping: GELATIN.damping,
      mass: GELATIN.mass,
    })
    expect(result.publish).toBe(false)
    expect(result.moving).toBe(false)
    expect(result.painted).toEqual({ width: 12, opacity: 1 })

    let publishes = 0
    subscribeSpringTick((dt) => {
      const next = stepSpringLease({
        tracks,
        target: { width: 12, opacity: 1 },
        painted: result.painted,
        dt,
        elapsedMs: SETTLE_HARD_MS,
        stiffness: GELATIN.stiffness,
        damping: GELATIN.damping,
        mass: GELATIN.mass,
      })
      if (next.publish) publishes += 1
      return next.moving
    })
    pumpFrames(1 / 60, 32)
    pumpFrames(1 / 60, 48)
    pumpFrames(1 / 60, 64)
    expect(publishes).toBe(0)
    expect(springClockBusy()).toBe(false)
  })

  it("hard-settles a leftover GELATIN crawl inside the 280–420ms budget", () => {
    let track = { pos: 0, vel: 0 }
    for (let i = 0; i < 55; i += 1) {
      const elapsed = (i + 1) * (1000 / 125)
      track = stepSpring(track, 8, 8 / 1000, GELATIN.stiffness, GELATIN.damping, GELATIN.mass)
      if (shouldSnapSpring(track, 8, elapsed)) track = snapSpring(8)
    }
    expect(isSpringRest(track, 8)).toBe(true)
  })
})
