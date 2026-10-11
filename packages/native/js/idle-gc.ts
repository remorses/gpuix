import { appliedBatchCount } from "./mutations.js"

export interface IdleGcOptions {
  /**
   * A number that changes whenever the app is doing work worth waiting out.
   * Defaults to the count of mutation batches applied to any renderer, which
   * moves on every commit, scroll-driven render and typed character.
   */
  activity?: () => number
  /**
   * Runs the collection. Defaults to `Bun.gc(true)` followed by `Bun.shrink()`, or else
   * `globalThis.gc` (Node's `--expose-gc`).
   */
  collect?: () => void
  /** How often to look, in ms. Default 5000. */
  checkMs?: number
  /** Quiet looks in a row before collecting. Default 2. */
  quietChecks?: number
  /** Fraction of one core above which work that commits nothing (parsing, decoding) is running. Default 0.5. */
  busyCpu?: number
}

type Collector = () => void

/** CPU time this process has used, in ms; 0 where the runtime cannot say (a browser). */
function cpuMs(): number {
  if (typeof process === "undefined" || typeof process.cpuUsage !== "function") return 0
  const { user, system } = process.cpuUsage()
  return (user + system) / 1000
}

function defaultCollector(): Collector | undefined {
  const runtime = globalThis as unknown as {
    Bun?: { gc(force: boolean): void; shrink?(): void }
    gc?: () => void
  }
  if (runtime.Bun) {
    const bun = runtime.Bun
    // A collection alone leaves the freed pages and the caches the engine rebuilds on demand
    // in place; shrinking releases them (about 45 MB on a long chat thread).
    return () => {
      bun.gc(true)
      bun.shrink?.()
    }
  }
  if (runtime.gc) return () => runtime.gc!()
  return undefined
}

/**
 * Run one full garbage collection once the app has been quiet for a while.
 *
 * A JavaScript engine collects on allocation pressure, so after a burst of work
 * (the first mount, opening a large screen) the heap and the pages behind it stay
 * at their peak until the next burst. This waits for `activity` to hold still
 * for `quietChecks` looks and the process to be under `busyCpu`, collects once,
 * then waits for the next busy period.
 *
 * CPU alone cannot stand for "quiet": a focused text field blinks its caret in
 * native code and redraws the window twice a second, which on a large screen
 * exceeds any threshold that would still tell a busy period apart. Commits do
 * not move for that, so they decide, and CPU only covers work that commits
 * nothing.
 *
 * Returns a function that stops it. Does nothing, and returns a no-op, where
 * the runtime has no way to force a collection.
 */
export function collectGarbageWhenIdle(options: IdleGcOptions = {}): () => void {
  const collect = options.collect ?? defaultCollector()
  if (!collect) return () => {}
  const activity = options.activity ?? appliedBatchCount
  const checkMs = options.checkMs ?? 5_000
  const quietChecks = options.quietChecks ?? 2
  const busyCpu = options.busyCpu ?? 0.5

  let lastCpu = cpuMs()
  let lastWall = performance.now()
  let lastActivity = activity()
  let quiet = 0
  let collected = false
  const timer = setInterval(() => {
    const cpu = cpuMs()
    const wall = performance.now()
    const busy = (cpu - lastCpu) / (wall - lastWall)
    const current = activity()
    const active = current !== lastActivity || busy >= busyCpu
    lastCpu = cpu
    lastWall = wall
    lastActivity = current
    if (active) {
      quiet = 0
      collected = false
      return
    }
    if (collected || ++quiet < quietChecks) return
    collected = true
    collect()
  }, checkMs)
  // A background check must not keep a process alive that has nothing else to do.
  ;(timer as { unref?: () => void }).unref?.()
  return () => clearInterval(timer)
}
