// Guards launch order: a Solid window must never show before its first commit.
import { describe, expect, it } from "bun:test"
import { resolve } from "node:path"

const src = resolve(import.meta.dir, "..")

function launchOrder(options: string): string[] {
  const order = `
import { GpuixRenderer } from "@gpuix/native"
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
const { render } = await import(${JSON.stringify(`${src}/renderer.ts`)})
const { createElement } = await import(${JSON.stringify(`${src}/universal.ts`)})
render(() => createElement("text"), ${options})
`
  const result = Bun.spawnSync({
    cmd: [process.execPath, "-e", order],
    stdin: "ignore",
    stdout: "ignore",
    stderr: "pipe",
    timeout: 10_000,
  })
  const stderr = result.stderr.toString()
  expect(result.exitCode, stderr).toBe(0)
  return JSON.parse(stderr)
}

describe.skipIf(process.platform !== "darwin")("launch", () => {
  it("commits before the first frame is shown", () => {
    expect(launchOrder("{}")).toEqual(["init show=false focus=false", "commit", "present", "tick"])
  })

  it("leaves a background launch's window options alone", () => {
    expect(launchOrder("{ focus: false }")).toEqual(["init show=undefined focus=false", "commit", "tick"])
  })
})
