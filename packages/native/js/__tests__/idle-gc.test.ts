import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { collectGarbageWhenIdle } from "../idle-gc.js"
import { appliedBatchCount, createMutationQueue } from "../host.js"

describe("collectGarbageWhenIdle", () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  const setup = () => {
    let activity = 0
    const collect = vi.fn()
    const stop = collectGarbageWhenIdle({ activity: () => activity, collect, checkMs: 1000, quietChecks: 2 })
    return {
      collect,
      stop,
      work: () => void activity++,
      wait: (checks: number) => vi.advanceTimersByTime(checks * 1000),
    }
  }

  it("collects once after the app has been quiet, then waits for the next burst", () => {
    const { collect, work, wait } = setup()
    wait(1)
    expect(collect, "one quiet look is not enough").not.toHaveBeenCalled()
    wait(1)
    expect(collect).toHaveBeenCalledTimes(1)
    wait(10)
    expect(collect, "nothing new to collect").toHaveBeenCalledTimes(1)

    work()
    wait(1)
    expect(collect, "the burst itself is not quiet").toHaveBeenCalledTimes(1)
    wait(2)
    expect(collect).toHaveBeenCalledTimes(2)
  })

  it("starts counting again whenever work shows up", () => {
    const { collect, work, wait } = setup()
    wait(1)
    work()
    wait(1)
    wait(1)
    expect(collect).not.toHaveBeenCalled()
    wait(1)
    expect(collect).toHaveBeenCalledTimes(1)
  })

  it("stops when asked", () => {
    const { collect, stop, wait } = setup()
    stop()
    wait(10)
    expect(collect).not.toHaveBeenCalled()
  })

  it("collects under Bun with a full collection and then a shrink, which releases the freed pages", () => {
    const calls: string[] = []
    vi.stubGlobal("Bun", { gc: (force: boolean) => calls.push(`gc:${force}`), shrink: () => calls.push("shrink") })
    try {
      collectGarbageWhenIdle({ activity: () => 0, checkMs: 1000, quietChecks: 1 })
      vi.advanceTimersByTime(1000)
      expect(calls).toEqual(["gc:true", "shrink"])
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it("still collects under a Bun that has no shrink", () => {
    const calls: string[] = []
    vi.stubGlobal("Bun", { gc: (force: boolean) => calls.push(`gc:${force}`) })
    try {
      collectGarbageWhenIdle({ activity: () => 0, checkMs: 1000, quietChecks: 1 })
      vi.advanceTimersByTime(1000)
      expect(calls).toEqual(["gc:true"])
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it("counts applied mutation batches by default", () => {
    const renderer = { applyBatch: () => [] as number[] }
    const queue = createMutationQueue(renderer)
    const before = appliedBatchCount()
    queue.flushMutations()
    expect(appliedBatchCount(), "an empty queue applies nothing").toBe(before)
    queue.createElement(1, "div")
    queue.flushMutations()
    expect(appliedBatchCount()).toBe(before + 1)
  })
})
