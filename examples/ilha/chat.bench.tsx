/** @jsxImportSource @gpuix/ilha */
/**
 * The chat example with a metrics tap, for `chat.bench.ts`.
 *
 * Draw times come from GPUIX's own frame overlay counters, the numbers the
 * on-screen overlay shows. They are written to `BENCH_OUT` a few times a
 * second so the driver, a separate process, can read them between steps.
 */

import { existsSync, rmSync, writeFileSync } from "node:fs"
import { render } from "@gpuix/ilha"
import { ChatApp } from "./chat"

const out = process.env.BENCH_OUT
if (!out) throw new Error("chat.bench.tsx is started by chat.bench.ts")
const turnCount = Number(process.env.TURNS ?? "1000")

const root = render(() => <ChatApp turnCount={turnCount} />, {
  title: "GPUIX Chat · ilha · bench",
  width: 1180,
  height: 820,
  titlebarTransparent: true,
  windowBackground: "blurred",
  // Counters only; the overlay itself is not painted.
  debugFrameOverlay: "minimal",
  focus: false,
})
const mountedAt = performance.now()

const { renderer } = root
// The first frame GPUI drew, to the millisecond, from process start.
let firstFrameMs: number | undefined
const firstFrame = setInterval(() => {
  if ((renderer.getDebugFrameOverlayStats?.()?.frames ?? 0) > 0) {
    firstFrameMs = performance.now()
    clearInterval(firstFrame)
  }
}, 1)

setInterval(() => {
  // The driver drops this file to start a clean measurement window.
  if (existsSync(`${out}.reset`)) {
    renderer.resetDebugFrameOverlayStats?.()
    rmSync(`${out}.reset`)
  }
  writeFileSync(
    out,
    JSON.stringify({
      pid: process.pid,
      // Milliseconds from process start to the end of the first render call.
      mountMs: mountedAt,
      firstFrameMs,
      at: performance.now(),
      heapUsedMB: process.memoryUsage().heapUsed / 1048576,
      frame: renderer.getDebugFrameOverlayStats?.(),
    }),
  )
}, 100)
