/**
 * Drives the ilha counter example through the automation client.
 *
 * Run from this directory so `@gpuix/ilha` resolves:
 *   cd examples/ilha && bun counter.automation.ts
 */

import path from "path"
import { fileURLToPath } from "url"
import { launch } from "@gpuix/ilha/automation"

const here = path.dirname(fileURLToPath(import.meta.url))

const watchdog = setTimeout(() => {
  console.error("[automation] global watchdog fired, exiting")
  process.exit(2)
}, 90_000)
watchdog.unref?.()

const app = await launch({
  command: "bun",
  args: ["counter.tsx"],
  cwd: here,
  env: { GPUIX_BACKGROUND: "1" },
})

let failures = 0
const check = async (name: string, fn: () => Promise<void>): Promise<void> => {
  try {
    await fn()
    console.log(`  ok   ${name}`)
  } catch (error) {
    failures += 1
    console.error(
      `  FAIL ${name}: ${error instanceof Error ? error.message : String(error)}`
    )
  }
}

const expectText = (actual: string, expected: string, what: string): void => {
  if (actual.trim() !== expected) {
    throw new Error(`${what}: expected "${expected}", got "${actual.trim()}"`)
  }
}

try {
  console.log("[automation] wait for first paint")
  await app.getByTestId("count").waitFor({ timeoutMs: 30_000 })

  await check("starts at zero", async () => {
    expectText(await app.getByTestId("count").textContent(), "0", "count")
  })

  await check("increment and decrement", async () => {
    await app.getByTestId("increment").click()
    await app.getByTestId("increment").click()
    await app.getByTestId("increment").click()
    expectText(await app.getByTestId("count").textContent(), "3", "count")

    await app.getByTestId("decrement").click()
    expectText(await app.getByTestId("count").textContent(), "2", "count")
  })

  await check("records a keyed history row per click", async () => {
    await app.getByTestId("history-3").waitFor({ timeoutMs: 5_000 })
    // A row is two adjacent text nodes: "#3" and "= 3".
    expectText(await app.getByTestId("history-3").textContent(), "#3= 3", "row 3")
    expectText(await app.getByTestId("history-4").textContent(), "#4= 2", "row 4")
  })

  await check("clicking the number increments", async () => {
    await app.getByTestId("count").click()
    expectText(await app.getByTestId("count").textContent(), "3", "count")
  })

  await check("reset clears the count and the history", async () => {
    await app.getByTestId("reset").click()
    expectText(await app.getByTestId("count").textContent(), "0", "count")
    const rows = await app.getByTestId("history").all()
    if (rows.length > 0) {
      throw new Error("history rows survived reset")
    }
  })
} finally {
  await app.close()
  clearTimeout(watchdog)
}

if (failures > 0) {
  console.error(`[automation] ${failures} check(s) failed`)
  process.exit(1)
}
console.log("[automation] all checks passed")
