/** @jsxImportSource @gpuix/ilha */
// Bun takes the JSX runtime from the tsconfig of the directory it is started
// in, not the one next to this file. The pragma makes the file run from
// anywhere; the tsconfig here still gives the editor the GPUIX element types.
import { createTestRoot } from "@gpuix/ilha/testing"
import { ChatApp } from "./chat"

const turnCount = Number(process.env.TURNS ?? "1000")
const root = createTestRoot()
const t = performance.now()
root.render(() => <ChatApp turnCount={turnCount} />)
// ilha mounts nested components on microtasks; a task later the tree is whole.
await new Promise((resolve) => setTimeout(resolve, 0))
root.root.flush()
root.renderer.flush()
const mountMs = +(performance.now() - t).toFixed(1)
Bun.gc(true)
const m = process.memoryUsage()
console.log(
  JSON.stringify({
    framework: "ilha",
    turnCount,
    mountMs,
    rssMB: +(m.rss / 1048576).toFixed(1),
    heapUsedMB: +(m.heapUsed / 1048576).toFixed(1),
  }),
)
root.unmount()
process.exit(0)
