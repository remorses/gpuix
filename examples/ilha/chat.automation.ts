/**
 * Drives the ilha chat example through the automation client.
 *
 * Run from this directory so `@gpuix/ilha` resolves:
 *   cd examples/ilha && bun chat.automation.ts
 */

import path from "path"
import { fileURLToPath } from "url"
import { launch } from "@gpuix/ilha/automation"

const here = path.dirname(fileURLToPath(import.meta.url))
const shots = path.resolve(here, "..", "screenshots")

const watchdog = setTimeout(() => {
  console.error("[automation] global watchdog fired, exiting")
  process.exit(2)
}, 90_000)
watchdog.unref?.()

function step(name: string) {
  console.log(`[automation] ${name}`)
}

const app = await launch({
  command: "bun",
  args: ["chat.tsx"],
  cwd: here,
  env: { GPUIX_BACKGROUND: "1" },
})

let failures = 0
async function check(name: string, fn: () => Promise<void>) {
  try {
    await fn()
    console.log(`  ok   ${name}`)
  } catch (error) {
    failures += 1
    console.error(`  FAIL ${name}: ${error instanceof Error ? error.message : String(error)}`)
  }
}

// Capture needs a test-support build, which Linux does not ship. The checks
// below read the retained tree and do not depend on it.
async function shot(name: string) {
  try {
    await app.screenshot({ path: path.join(shots, name) })
  } catch (error) {
    step(`no screenshot (${error instanceof Error ? error.message : String(error)})`)
  }
}

try {
  step("wait for first paint")
  await app.getByTestId("sidebar-collapse").waitFor({ timeoutMs: 30_000 })
  await shot("ilha-chat-initial.png")

  await check("collapse then expand the sidebar", async () => {
    await app.getByTestId("sidebar-collapse").click()
    await app.getByTestId("sidebar-expand").waitFor({ timeoutMs: 5_000 })
    await app.getByTestId("sidebar-expand").click()
    await app.getByTestId("sidebar-collapse").waitFor({ timeoutMs: 5_000 })
  })

  await check("switch conversation from the sidebar", async () => {
    // The sidebar title is truncated ("...optimizatio..."), but the transcript
    // user turn spells it out. So this phrase is unique to the switched thread.
    await app.getByTestId("thread-c4").click()
    await app.getByText("memory optimizations are left").waitFor({ timeoutMs: 5_000 })
    await app.getByTestId("thread-c1").click()
  })

  await check("send a message adds a user turn", async () => {
    // The demo reply is a native <markdown> node, invisible to the retained
    // tree. Assert on the user turn instead: a plain <text>, unique and retained.
    const composer = app.getByTestId("composer")
    await composer.fill("hello from ilha")
    await composer.press("enter")
    await app.getByText("hello from ilha").waitFor({ timeoutMs: 5_000 })
  })

  await check("open the model picker and choose a model", async () => {
    await app.getByTestId("model-picker").click()
    await app.getByTestId("model-opus-4.6").waitFor({ timeoutMs: 5_000 })
    await app.getByTestId("model-opus-4.6").click()
    await app.getByText("Claude Opus 4.6").waitFor({ timeoutMs: 5_000 })
  })

  await check("toggle the inspector", async () => {
    await app.getByTestId("inspector-toggle").click()
    await app.getByText("Inspector").waitFor({ timeoutMs: 5_000 })
  })

  await check("open the search overlay and filter", async () => {
    await app.getByTestId("search").click()
    await app.getByTestId("search-input").waitFor({ timeoutMs: 5_000 })
    await app.getByTestId("search-input").fill("memory")
    await app.getByTestId("search-c4").waitFor({ timeoutMs: 5_000 })
    await app.getByTestId("search-c4").click()
  })

  await shot("ilha-chat-final.png")
  step(`done, ${failures} failure(s)`)
} finally {
  await app.close()
}

process.exit(failures === 0 ? 0 : 1)
