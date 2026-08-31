import { describe, expect, it } from "vitest"
import { GELATIN, stepSpring, type SpringTrack } from "../motion-spring.js"

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
