/// Persist-and-remount tests for render(). bun --hot re-evaluates the entry
/// and calls render() again; the native host must stay the same instance.

import { spawn } from "node:child_process"
import { unlinkSync, writeFileSync } from "node:fs"
import { createRequire } from "node:module"
import { join } from "node:path"
import { fileURLToPath } from "node:url"
import React, { useState } from "react"
import { beforeEach, describe, expect, it } from "vitest"
import { hasNativeTestRenderer, TestRenderer } from "../testing.js"
import {
  installBrowserAutomation,
  render,
  resetRender,
} from "../reconciler/renderer.js"
import { handleGpuixEvent } from "../reconciler/event-registry.js"

const srcDir = fileURLToPath(new URL("..", import.meta.url))

function hotAppSource(label: string): string {
  return `
import React from "react"
import { TestRenderer } from ${JSON.stringify(join(srcDir, "testing.ts"))}
import { render } from ${JSON.stringify(join(srcDir, "reconciler/renderer.ts"))}

const slot = globalThis
slot.__hotEvals = (slot.__hotEvals ?? 0) + 1
if (!slot.__hotRenderer) {
  slot.__hotRenderer = new TestRenderer()
}
const renderer = slot.__hotRenderer
render(
  React.createElement(
    "div",
    {
      onClick: () => console.log("HOT_CLICK", ${JSON.stringify(label)}),
      style: { width: 100, height: 100 },
    },
    ${JSON.stringify(label)}
  ),
  { renderer }
)
renderer.flush()
renderer.nativeSimulateClick(10, 10)
const rootId = renderer.getRoot()?.id
if (slot.__hotRootId !== undefined) {
  console.log("HOT_NEW_ROOT_ID", rootId !== slot.__hotRootId)
}
slot.__hotRootId = rootId
console.log("HOT_EVAL", slot.__hotEvals)
console.log("HOT_LABEL", ${JSON.stringify(label)})
console.log("HOT_TEXT", JSON.stringify(renderer.getAllText()))
console.log("HOT_SAME_RENDERER", renderer === slot.__hotRenderer)
setInterval(() => {}, 1 << 30)
`
}

function collectOutput(child: ReturnType<typeof spawn>) {
  let buf = ""
  child.stdout?.on("data", (chunk) => {
    buf += String(chunk)
  })
  child.stderr?.on("data", (chunk) => {
    buf += String(chunk)
  })
  return {
    wait: async (match: string, timeoutMs: number) => {
      const start = Date.now()
      while (!buf.includes(match)) {
        if (Date.now() - start > timeoutMs) {
          throw new Error(`timed out waiting for ${JSON.stringify(match)}\n${buf}`)
        }
        await new Promise((resolve) => setTimeout(resolve, 50))
      }
      return buf
    },
  }
}

describe("TestGpuixRenderer availability", () => {
  it("exports a constructor, and a flag that is true only when construction works", () => {
    const native = createRequire(import.meta.url)("@gpuix/native") as {
      TestGpuixRenderer?: new (width?: number, height?: number) => unknown
      hasTestGpuixRenderer?: () => boolean
    }
    expect(typeof native.TestGpuixRenderer).toBe("function")
    expect(native.hasTestGpuixRenderer?.()).toBe(hasNativeTestRenderer)
    if (hasNativeTestRenderer) {
      const renderer = new native.TestGpuixRenderer!(1, 1)
      expect(renderer).toBeTruthy()
    } else {
      expect(() => new native.TestGpuixRenderer!()).toThrow(
        /macOS and Windows only.*wgpu cannot read a rendered image back yet.*GpuixRenderer still works/s
      )
    }
  })
})

const describeNative = hasNativeTestRenderer ? describe : describe.skip

describeNative("render()", () => {
  let renderer: TestRenderer

  beforeEach(() => {
    resetRender()
    renderer = new TestRenderer()
  })

  it("reuses the injected renderer on the second call", () => {
    const ignored = new TestRenderer()
    render(<text>one</text>, { renderer })
    render(<text>two</text>, { renderer: ignored })

    renderer.flush()
    expect(renderer.getAllText()).toEqual(["two"])
    expect(ignored.getAllText()).toEqual([])
  })

  it("replaces painted text when the entry is evaluated again", () => {
    render(<text>hello</text>, { renderer })
    renderer.flush()
    expect(renderer.getAllText()).toEqual(["hello"])

    render(<text>world</text>, { renderer })
    renderer.flush()
    expect(renderer.getAllText()).toEqual(["world"])
  })

  it("does not deliver a queued window event to a remounted root", () => {
    const received: string[] = []
    const observed: string[] = []
    render(<text>first</text>, {
      renderer,
      onKeyDown: () => received.push("first"),
      onEvent: () => observed.push("first"),
    })
    render(<text>second</text>, {
      renderer,
      onKeyDown: () => received.push("second"),
      onEvent: () => observed.push("second"),
    })

    handleGpuixEvent(
      { elementId: 1, eventType: "windowKeyDown", key: "tab" },
      renderer
    )
    handleGpuixEvent(
      { elementId: 2, eventType: "windowKeyDown", key: "tab" },
      renderer
    )

    expect(received).toEqual(["second"])
    expect(observed).toEqual(["second"])
  })

  it("delivers Tab to elements and the renderer without moving focus", () => {
    const windowKeys: string[] = []
    const windowKeyUps: string[] = []
    const elementKeys: string[] = []

    render(
      <div style={{ width: 200, height: 100 }}>
        <div
          autoFocus
          tabIndex={0}
          onKeyDown={(event) => {
            elementKeys.push(
              `first:${event.modifiers?.shift ? "shift-" : ""}${event.key}`
            )
          }}
        />
        <div
          tabIndex={0}
          onKeyDown={(event) => {
            elementKeys.push(
              `second:${event.modifiers?.shift ? "shift-" : ""}${event.key}`
            )
          }}
        />
      </div>,
      {
        renderer,
        onKeyDown: (event) => {
          windowKeys.push(`${event.modifiers?.shift ? "shift-" : ""}${event.key}`)
        },
        onKeyUp: (event) => {
          windowKeyUps.push(`${event.modifiers?.shift ? "shift-" : ""}${event.key}`)
        },
      }
    )
    renderer.flush()

    renderer.simulateKeystrokes("tab")
    renderer.simulateKeystrokes("a")

    const second = renderer
      .findByType("div")
      .filter((element) => element.events.has("keyDown"))[1]
    renderer.focusElement(second.id)
    renderer.simulateKeystrokes("shift-tab")
    renderer.simulateKeystrokes("b")
    renderer.nativeSimulateKeyUp(second.id, "shift-tab")

    expect({ windowKeys, windowKeyUps, elementKeys }).toMatchInlineSnapshot(`
      {
        "elementKeys": [
          "first:tab",
          "first:a",
          "second:shift-tab",
          "second:b",
        ],
        "windowKeyUps": [
          "shift-tab",
        ],
        "windowKeys": [
          "tab",
          "a",
          "shift-tab",
          "b",
        ],
      }
    `)
  })

  it("remounts when the app component identity changes", () => {

    function makeApp(label: string) {
      return function App() {
        const [value] = useState(label)
        return <text>{value}</text>
      }
    }

    render(React.createElement(makeApp("first")), { renderer })
    renderer.flush()
    expect(renderer.getAllText()).toEqual(["first"])

    render(React.createElement(makeApp("second")), { renderer })
    renderer.flush()
    expect(renderer.getAllText()).toEqual(["second"])
  })

  it("keeps the remounted tree after deferred React work", async () => {
    render(
      <div>
        <text>before</text>
      </div>,
      { renderer }
    )
    renderer.flush()
    expect(renderer.getAllText()).toEqual(["before"])
    expect(renderer.getRoot()).toBeDefined()

    render(
      <div>
        <text>after</text>
      </div>,
      { renderer }
    )
    renderer.flush()
    expect(renderer.getAllText()).toEqual(["after"])

    await new Promise((resolve) => setTimeout(resolve, 50))
    renderer.flush()
    expect(renderer.getRoot()).toBeDefined()
    expect(renderer.getAllText()).toEqual(["after"])
  })

  it("takes a class resolver", () => {
    // Without this, a class channel meant building the root by hand, because
    // render() had no way to pass one on.
    render(<text className="brand">named</text>, {
      renderer,
      resolveClassName: (token) => (token === "brand" ? { color: "#ff0000" } : null),
    })
    renderer.flush()

    const node = renderer.findByType("text")[0]
    expect(node?.style).toMatchObject({ color: "#ff0000" })
  })

  it("always exposes browser automation on globalThis", async () => {
    Reflect.set(globalThis, "window", {})
    try {
      installBrowserAutomation(renderer)
      render(<text>automated</text>, { renderer })
      renderer.flush()

      const automation = Reflect.get(globalThis, "gpuix")
      expect(automation).toBeDefined()
      expect(await automation.getByText("automated").textContent()).toBe("automated")
    } finally {
      resetRender()
      Reflect.deleteProperty(globalThis, "window")
    }

    expect(Reflect.get(globalThis, "gpuix")).toBeUndefined()
  })

  it("remounts under bun --hot without creating a new root", async () => {
    const file = join(srcDir, "__tests__", "hot-app.tmp.tsx")
    writeFileSync(file, hotAppSource("hello"))

    const child = spawn("bun", ["--hot", file], {
      cwd: srcDir,
      stdio: ["ignore", "pipe", "pipe"],
    })
    const output = collectOutput(child)

    try {
      await output.wait("HOT_LABEL hello", 15_000)
      await output.wait('HOT_TEXT ["hello"]', 1000)
      await output.wait("HOT_CLICK hello", 1000)
      await output.wait("HOT_SAME_RENDERER true", 1000)
      await new Promise((resolve) => setTimeout(resolve, 300))

      writeFileSync(file, hotAppSource("world"))

      await output.wait("HOT_LABEL world", 15_000)
      await output.wait('HOT_TEXT ["world"]', 1000)
      await output.wait("HOT_CLICK world", 1000)
      await output.wait("HOT_NEW_ROOT_ID true", 1000)
      await output.wait("HOT_SAME_RENDERER true", 1000)
    } finally {
      child.kill("SIGTERM")
      try {
        unlinkSync(file)
      } catch {}
    }
  }, 40_000)
})
