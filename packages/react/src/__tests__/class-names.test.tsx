/// `className` painting the same pixels as the style it stands for.
///
/// The merge rules are covered without a GPU in `host-config-style.test.tsx`.
/// What these add is that the style a class declares reaches the renderer at
/// mount and on an update, through the same path a real application uses.

import fs from "fs"
import path from "path"
import React from "react"
import { beforeAll, describe, expect, it } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"
import { expectScreenshotsEqual, SHOTS_DIR } from "./test-utils.js"
import type { ClassNameResolver, StyleDesc } from "../types/host.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

beforeAll(() => {
  fs.mkdirSync(SHOTS_DIR, { recursive: true })
})

const shot = (name: string) => path.join(SHOTS_DIR, `class-${name}.png`)

const BOX = { width: 200, height: 120 } as const

const TABLE: Record<string, Record<string, unknown>> = {
  box: BOX,
  "bg-red": { backgroundColor: "#ff0000" },
  "bg-blue": { backgroundColor: "#0000ff" },
  "p-5": { padding: 20 },
  "child-green": { width: 40, height: 40, backgroundColor: "#00ff00" },
}

const resolveClassName: ClassNameResolver = (token) => TABLE[token] ?? null

function paint(name: string, tree: React.ReactElement, withResolver = true) {
  const root = createTestRoot(withResolver ? { resolveClassName } : {})
  root.render(tree)
  root.renderer.captureScreenshot(shot(name))
  root.unmount()
}

describeNative("className", () => {
  it("paints a class the same as the style it stands for", () => {
    paint("through", <div className="box bg-red" />)
    paint("direct", <div style={{ ...BOX, backgroundColor: "#ff0000" }} />, false)
    expectScreenshotsEqual(shot("through"), shot("direct"))
  })

  it("paints a class and a style prop together", () => {
    paint("mixed", <div className="box p-5 bg-red">
      <div className="child-green" />
    </div>)
    paint(
      "mixed-direct",
      <div style={{ ...BOX, padding: 20, backgroundColor: "#ff0000" }}>
        <div style={{ width: 40, height: 40, backgroundColor: "#00ff00" }} />
      </div>,
      false
    )
    expectScreenshotsEqual(shot("mixed"), shot("mixed-direct"))
  })

  it("lets the style prop beat the class", () => {
    paint("override", <div className="box bg-red" style={{ backgroundColor: "#0000ff" }} />)
    paint("override-direct", <div style={{ ...BOX, backgroundColor: "#0000ff" }} />, false)
    expectScreenshotsEqual(shot("override"), shot("override-direct"))
  })

  it("repaints when the class string changes", () => {
    const root = createTestRoot({ resolveClassName })
    root.render(<div className="box bg-red" />)
    root.render(<div className="box bg-blue" />)
    root.renderer.captureScreenshot(shot("changed"))
    root.unmount()

    paint("changed-expected", <div style={{ ...BOX, backgroundColor: "#0000ff" }} />, false)
    expectScreenshotsEqual(shot("changed"), shot("changed-expected"))
  })

  it("paints nothing from a class when the root has no resolver", () => {
    const warn = console.warn
    console.warn = () => {}
    try {
      paint("no-resolver", <div className="box bg-red" style={BOX} />, false)
    } finally {
      console.warn = warn
    }
    paint("no-resolver-direct", <div style={BOX} />, false)
    expectScreenshotsEqual(shot("no-resolver"), shot("no-resolver-direct"))
  })

  it("resolves nothing again when the same class string comes back", () => {
    const { renderer, render } = createTestRoot({ resolveClassName })
    render(<div className="box bg-red" />)
    renderer.resetStyleResolutions()
    render(<div className="box bg-red" />)
    expect(renderer.styleResolutions()).toBe(0)
  })
})

/// The index and child conditions, painted. Each test renders the class form
/// and the same picture written as inline styles, and compares the pixels.
const CONDITIONS: Record<string, StyleDesc> = {
  stack: { width: 200, height: 160, display: "flex", flexDirection: "column" },
  cell: { height: 30, backgroundColor: "#222222" },
  "first-red": {
    selectors: [{ on: ":first-child", style: { backgroundColor: "#ff0000" } }],
  },
  "last-blue": {
    selectors: [{ on: ":last-child", style: { backgroundColor: "#0000ff" } }],
  },
  "odd-red": {
    selectors: [{ on: ":nth-child(odd)", style: { backgroundColor: "#ff0000" } }],
  },
  "even-blue": {
    selectors: [{ on: ":nth-child(even)", style: { backgroundColor: "#0000ff" } }],
  },
  spaced: {
    selectors: [{ on: "& > :not(:last-child)", style: { marginBottom: 10 } }],
  },
  "kids-green": {
    selectors: [{ on: "& > *", style: { backgroundColor: "#00ff00" } }],
  },
  "deep-green": {
    selectors: [{ on: "& *", style: { backgroundColor: "#00ff00" } }],
  },
}

const resolveCondition: ClassNameResolver = (token) => CONDITIONS[token] ?? null

function paintConditions(name: string, tree: React.ReactElement, withResolver = true) {
  const root = createTestRoot(withResolver ? { resolveClassName: resolveCondition } : {})
  root.render(tree)
  root.renderer.captureScreenshot(shot(name))
  root.unmount()
}

describeNative("selector conditions", () => {
  const STACK = CONDITIONS.stack as Record<string, unknown>
  const CELL = { height: 30 } as const

  it("paints first and last from the child position", () => {
    paintConditions(
      "index",
      <div className="stack">
        <div className="cell first-red last-blue" />
        <div className="cell first-red last-blue" />
        <div className="cell first-red last-blue" />
      </div>
    )
    paintConditions(
      "index-direct",
      <div style={STACK}>
        <div style={{ ...CELL, backgroundColor: "#ff0000" }} />
        <div style={{ ...CELL, backgroundColor: "#222222" }} />
        <div style={{ ...CELL, backgroundColor: "#0000ff" }} />
      </div>,
      false
    )
    expectScreenshotsEqual(shot("index"), shot("index-direct"))
  })

  it("stripes odd and even, counting from one", () => {
    paintConditions(
      "stripes",
      <div className="stack">
        <div className="cell odd-red even-blue" />
        <div className="cell odd-red even-blue" />
        <div className="cell odd-red even-blue" />
      </div>
    )
    paintConditions(
      "stripes-direct",
      <div style={STACK}>
        <div style={{ ...CELL, backgroundColor: "#ff0000" }} />
        <div style={{ ...CELL, backgroundColor: "#0000ff" }} />
        <div style={{ ...CELL, backgroundColor: "#ff0000" }} />
      </div>,
      false
    )
    expectScreenshotsEqual(shot("stripes"), shot("stripes-direct"))
  })

  it("re-evaluates the position when the list changes", () => {
    const root = createTestRoot({ resolveClassName: resolveCondition })
    const rows = (count: number) => (
      <div className="stack">
        {Array.from({ length: count }, (_, at) => (
          <div key={at} className="cell last-blue" />
        ))}
      </div>
    )
    root.render(rows(2))
    root.render(rows(3))
    root.renderer.captureScreenshot(shot("grown"))
    root.unmount()

    paintConditions(
      "grown-direct",
      <div style={STACK}>
        <div style={{ ...CELL, backgroundColor: "#222222" }} />
        <div style={{ ...CELL, backgroundColor: "#222222" }} />
        <div style={{ ...CELL, backgroundColor: "#0000ff" }} />
      </div>,
      false
    )
    expectScreenshotsEqual(shot("grown"), shot("grown-direct"))
  })

  it("spaces every child except the last from a rule on the parent", () => {
    paintConditions(
      "spaced",
      <div className="stack spaced">
        <div className="cell" />
        <div className="cell" />
        <div className="cell" />
      </div>
    )
    paintConditions(
      "spaced-direct",
      <div style={STACK}>
        <div style={{ ...CELL, backgroundColor: "#222222", marginBottom: 10 }} />
        <div style={{ ...CELL, backgroundColor: "#222222", marginBottom: 10 }} />
        <div style={{ ...CELL, backgroundColor: "#222222" }} />
      </div>,
      false
    )
    expectScreenshotsEqual(shot("spaced"), shot("spaced-direct"))
  })

  it("lets a child's own declaration beat a rule from the parent", () => {
    // `& > *` compiles from `:where()`, which has specificity zero. The first
    // child declares no background, so the rule paints it. The second and the
    // third declare their own, through a class and through the style prop, and
    // each keeps it.
    paintConditions(
      "kids",
      <div className="stack kids-green">
        <div style={{ height: 30 }} />
        <div className="cell" />
        <div className="cell" style={{ backgroundColor: "#0000ff" }} />
      </div>
    )
    paintConditions(
      "kids-direct",
      <div style={STACK}>
        <div style={{ ...CELL, backgroundColor: "#00ff00" }} />
        <div style={{ ...CELL, backgroundColor: "#222222" }} />
        <div style={{ ...CELL, backgroundColor: "#0000ff" }} />
      </div>,
      false
    )
    expectScreenshotsEqual(shot("kids"), shot("kids-direct"))
  })

  it("reaches a grandchild through a descendant rule", () => {
    // The wrapper declares no background, so the rule paints it. The cell
    // declares its own, which wins.
    paintConditions(
      "deep",
      <div className="stack deep-green">
        <div style={{ padding: 10 }}>
          <div className="cell" />
          <div style={{ height: 30 }} />
        </div>
      </div>
    )
    paintConditions(
      "deep-direct",
      <div style={STACK}>
        <div style={{ padding: 10, backgroundColor: "#00ff00" }}>
          <div style={{ ...CELL, backgroundColor: "#222222" }} />
          <div style={{ height: 30, backgroundColor: "#00ff00" }} />
        </div>
      </div>,
      false
    )
    expectScreenshotsEqual(shot("deep"), shot("deep-direct"))
  })

  it("keeps a descendant rule off the raw text inside a text element", () => {
    // The reconciler turns a raw string child into an anonymous text node.
    // The web gives a text node no box, so `& *` must not paint it.
    paintConditions(
      "raw-deep",
      <div className="stack deep-green">
        <text style={{ backgroundColor: "#0000ff", color: "#ffffff", padding: 10 }}>HELLO</text>
      </div>
    )
    paintConditions(
      "raw-deep-direct",
      <div style={STACK}>
        <text style={{ backgroundColor: "#0000ff", color: "#ffffff", padding: 10 }}>HELLO</text>
      </div>,
      false
    )
    expectScreenshotsEqual(shot("raw-deep"), shot("raw-deep-direct"))
  })

  it("does not count a raw string sibling in the child positions", () => {
    paintConditions(
      "raw-index",
      <div className="stack">
        {"loose"}
        <div className="cell first-red last-blue" />
        <div className="cell first-red last-blue" />
      </div>
    )
    paintConditions(
      "raw-index-direct",
      <div style={STACK}>
        {"loose"}
        <div style={{ ...CELL, backgroundColor: "#ff0000" }} />
        <div style={{ ...CELL, backgroundColor: "#0000ff" }} />
      </div>,
      false
    )
    expectScreenshotsEqual(shot("raw-index"), shot("raw-index-direct"))
  })
})
