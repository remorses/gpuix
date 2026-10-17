/**
 * Measures the ilha chat example on a real window: startup, memory, idle CPU,
 * and draw times while scrolling and typing.
 *
 *   cd examples/ilha && bun chat.bench.ts
 *
 * Linux only: memory and CPU are read from /proc.
 */

import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs"
import os from "node:os"
import path from "path"
import { fileURLToPath } from "url"
import { launch } from "@gpuix/ilha/automation"

const here = path.dirname(fileURLToPath(import.meta.url))
const out = path.join(os.tmpdir(), `gpuix-ilha-bench-${process.pid}.json`)
const turns = process.env.TURNS ?? "1000"
const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms))

interface Tap {
  pid: number
  mountMs: number
  firstFrameMs?: number
  at: number
  heapUsedMB: number
  frame?: { p90Ms?: number; p99Ms?: number; maxMs?: number; frames: number; samples: number }
}
const tap = (): Tap => JSON.parse(readFileSync(out, "utf8")) as Tap
const fresh = async (): Promise<Tap> => {
  const before = tap().at
  for (;;) {
    await sleep(40)
    const next = tap()
    if (next.at > before) return next
  }
}
const resetFrames = async () => {
  writeFileSync(`${out}.reset`, "")
  while (existsSync(`${out}.reset`)) await sleep(20)
  await fresh()
}

/** The process and every descendant, so helper processes are counted too. */
function tree(pid: number): number[] {
  const children = new Map<number, number[]>()
  for (const entry of readdirSync("/proc")) {
    if (!/^\d+$/.test(entry)) continue
    try {
      const stat = readFileSync(`/proc/${entry}/stat`, "utf8")
      const parent = Number(stat.slice(stat.lastIndexOf(")") + 2).split(" ")[1])
      children.set(parent, [...(children.get(parent) ?? []), Number(entry)])
    } catch {}
  }
  const all: number[] = []
  const walk = (id: number) => {
    all.push(id)
    for (const child of children.get(id) ?? []) walk(child)
  }
  walk(pid)
  return all
}

/** Proportional and resident set size of a process tree, in MB. */
function memory(pid: number) {
  let pss = 0
  let rss = 0
  const pids = tree(pid)
  for (const id of pids) {
    try {
      const rollup = readFileSync(`/proc/${id}/smaps_rollup`, "utf8")
      pss += Number(/^Pss:\s+(\d+)/m.exec(rollup)?.[1] ?? 0)
      rss += Number(/^Rss:\s+(\d+)/m.exec(rollup)?.[1] ?? 0)
    } catch {}
  }
  return { processes: pids.length, pssMB: pss / 1024, rssMB: rss / 1024 }
}

/** CPU seconds the tree has used so far (user + system). */
function cpuSeconds(pid: number): number {
  let ticks = 0
  for (const id of tree(pid)) {
    try {
      const stat = readFileSync(`/proc/${id}/stat`, "utf8")
      const fields = stat.slice(stat.lastIndexOf(")") + 2).split(" ")
      ticks += Number(fields[11]) + Number(fields[12])
    } catch {}
  }
  return ticks / 100
}

/** Run `work`, and report the frames drawn and CPU used while it ran. */
async function measure(pid: number, work: () => Promise<void>) {
  await resetFrames()
  const cpu = cpuSeconds(pid)
  const start = performance.now()
  const first = tap().frame?.frames ?? 0
  await work()
  const seconds = (performance.now() - start) / 1000
  const last = await fresh()
  return {
    seconds: +seconds.toFixed(2),
    framesPerSecond: +(((last.frame?.frames ?? 0) - first) / seconds).toFixed(1),
    drawP90Ms: last.frame?.p90Ms,
    drawP99Ms: last.frame?.p99Ms,
    drawMaxMs: last.frame?.maxMs,
    cpuPercentOfOneCore: +(((cpuSeconds(pid) - cpu) / seconds) * 100).toFixed(1),
  }
}

const spawned = performance.now()
const app = await launch({
  command: "bun",
  args: ["chat.bench.tsx"],
  cwd: here,
  env: { BENCH_OUT: out, TURNS: turns },
})

try {
  await app.getByTestId("composer").waitFor({ timeoutMs: 30_000 })
  const firstPaintMs = performance.now() - spawned
  while (!existsSync(out)) await sleep(20)
  const { pid, mountMs } = tap()
  const firstFrameMs = (await fresh()).firstFrameMs

  await sleep(3000)
  const idleMemory = memory(pid)
  const heapUsedMB = tap().heapUsedMB

  const idle = await measure(pid, () => sleep(5000))

  // One wheel event per 120 Hz frame, down then back up, over the transcript.
  const scroll = await measure(pid, async () => {
    const end = performance.now() + 6000
    let tick = 0
    while (performance.now() < end) {
      const down = performance.now() < end - 3000
      await app.mouse.wheel({ x: 700, y: 400 }, 0, down ? -60 : 60)
      tick += 1
      const wait = tick * (1000 / 120) - (performance.now() - (end - 6000))
      if (wait > 0) await sleep(wait)
    }
  })

  const afterFirstScroll = memory(pid)
  // The same pass again: memory that only grows the first time is a cache.
  const scrollAgain = await measure(pid, async () => {
    const end = performance.now() + 6000
    let tick = 0
    while (performance.now() < end) {
      const down = performance.now() < end - 3000
      await app.mouse.wheel({ x: 700, y: 400 }, 0, down ? -60 : 60)
      tick += 1
      const wait = tick * (1000 / 120) - (performance.now() - (end - 6000))
      if (wait > 0) await sleep(wait)
    }
  })
  const afterSecondScroll = memory(pid)

  const typing = await measure(pid, async () => {
    const composer = app.getByTestId("composer")
    for (const key of "the quick brown fox jumps over the lazy dog".replaceAll(" ", "")) {
      await composer.press(key)
      await sleep(30)
    }
  })

  const sidebar = await measure(pid, async () => {
    for (let round = 0; round < 5; round += 1) {
      await app.getByTestId("sidebar-collapse").click()
      await sleep(300)
      await app.getByTestId("sidebar-expand").click()
      await sleep(300)
    }
  })

  console.log(
    JSON.stringify(
      {
        turns: Number(turns),
        startup: {
          // In the app, from process start: render() returned, first frame drawn.
          mountMs: Math.round(mountMs),
          firstFrameMs: firstFrameMs === undefined ? undefined : Math.round(firstFrameMs),
          // In this driver, from spawn until the automation tree shows the composer.
          automationReadyMs: Math.round(firstPaintMs),
        },
        memory: { ...idleMemory, jsHeapUsedMB: +heapUsedMB.toFixed(1) },
        idle,
        scroll,
        afterFirstScroll,
        scrollAgain,
        afterSecondScroll,
        typing,
        sidebar,
      },
      null,
      2,
    ),
  )
} finally {
  await app.close()
}
process.exit(0)
