// Guards launch speed: zod costs over 30 ms to import, and only automation uses it.
import { spawnSync } from "node:child_process"
import { describe, expect, it } from "vitest"

const script = `
import { registerHooks } from "node:module"
const loaded = []
registerHooks({
  resolve(specifier, context, next) {
    loaded.push(specifier)
    return next(specifier, context)
  },
})
await import("@gpuix/react")
console.log(JSON.stringify(loaded.filter((specifier) => specifier === "zod").length))
process.exit(0)
`

function launchOrder(options: string): string[] {
  const order = `
import { GpuixRenderer } from "@gpuix/native"
import React from "react"
const calls = []
const proto = GpuixRenderer.prototype
const init = proto.init
proto.init = function (options) {
  calls.push("init show=" + options.show + " focus=" + options.focus)
  return init.call(this, { ...options, show: false, focus: false })
}
const applyBatch = proto.applyBatch
proto.applyBatch = function (json) {
  if (!calls.includes("commit")) calls.push("commit")
  return applyBatch.call(this, json)
}
proto.presentWindow = () => calls.push("present")
proto.tick = () => {
  calls.push("tick")
  process.stderr.write(JSON.stringify(calls))
  process.exit(0)
}
const { render } = await import("@gpuix/react")
render(React.createElement("text", null, "x"), ${options})
`
  const result = spawnSync("node", ["--input-type=module", "-e", order], { encoding: "utf8", timeout: 10_000 })
  expect(result.status, result.stderr).toBe(0)
  return JSON.parse(result.stderr)
}

describe("launch", () => {
  it.skipIf(process.platform !== "darwin")("commits before the first frame is shown", () => {
    expect(launchOrder("{}")).toEqual(["init show=false focus=false", "commit", "present", "tick"])
  })

  it.skipIf(process.platform !== "darwin")("leaves a background launch's window options alone", () => {
    expect(launchOrder("{ focus: false }")).toEqual(["init show=undefined focus=false", "commit", "tick"])
  })

  it("does not load zod until automation is used", () => {
    const result = spawnSync("node", ["--input-type=module", "-e", script], { encoding: "utf8" })
    expect(result.status, result.stderr).toBe(0)
    expect(JSON.parse(result.stdout.trim().split("\n").pop() ?? "")).toBe(0)
  })
})
