/**
 * The demo, driven through the native GPUI test renderer.
 *
 * Every panel mounts and paints on real Metal, and the cases that carry a
 * number are asserted rather than looked at. Screenshots land in
 * gpuix-demo/*.png under the OS temp directory for the ones that only
 * a person can judge.
 */

import fs from "fs"
import os from "os"
import path from "path"
import React from "react"
import { describe, expect, it } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "@gpuix/react/testing"
import type { TestRoot } from "@gpuix/react/testing"
import { App, BASE, PALETTES } from "./demo/app"
import { ClassNames } from "./demo/class-names"
import { Colors } from "./demo/colors"
import { Effects } from "./demo/effects"
import { Gradients } from "./demo/gradients"
import { Inheritance } from "./demo/inheritance"
import { Lengths } from "./demo/lengths"
import { motion } from "@gpuix/react"
import { Motion } from "./demo/motion-panel"
import { Navigation } from "./demo/navigation"
import { IntoView, Scrollbars } from "./demo/scrollbars"
import { Selectors } from "./demo/selectors"
import { Variables } from "./demo/variables"
import { resolveClassName } from "./demo/classes"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

/// `/tmp` does not exist on Windows, and native `save()` never creates the
/// parent directory itself.
const SHOTS_DIR = path.join(os.tmpdir(), "gpuix-demo")
fs.mkdirSync(SHOTS_DIR, { recursive: true })
const shot = (name: string) => path.join(SHOTS_DIR, `${name}.png`)

function root(): TestRoot {
  return createTestRoot({ resolveClassName })
}

const PANELS = [
  ["colors", <Colors />],
  ["gradients", <Gradients />],
  ["effects", <Effects />],
  ["lengths", <Lengths />],
  ["variables", <Variables />],
  ["inheritance", <Inheritance />],
  ["classes", <ClassNames />],
  ["selectors", <Selectors />],
  ["motion", <Motion />],
  ["scrollbars", <Scrollbars />],
  ["navigation", <Navigation />],
] as const

describeNative("demo panels", () => {
  for (const [name, panel] of PANELS) {
    it(`${name} mounts and paints`, () => {
      const test = root()
      test.render(
        <div
          style={{
            ...BASE,
            ...PALETTES.midnight,
            width: "100%",
            height: "100%",
            padding: 16,
            overflowY: "scroll",
            backgroundColor: "var(--color-bg)",
          }}
        >
          {panel}
        </div>
      )
      test.renderer.captureScreenshot(shot(name))
      expect(fs.statSync(shot(name)).size).toBeGreaterThan(0)
      expect(test.renderer.getPaintedText().length).toBeGreaterThan(0)
      test.unmount()
    })
  }
})

describeNative("a class and the style it stands for", () => {
  it("paints the same pixels", () => {
    const viaClass = root()
    viaClass.render(<div className="w-full h-full p-4 bg-[#7c6cff] rounded-lg" />)
    viaClass.renderer.captureScreenshot(shot("class"))
    viaClass.unmount()

    const viaStyle = root()
    viaStyle.render(
      <div style={{ width: "100%", height: "100%", padding: 16, backgroundColor: "#7c6cff", borderRadius: 12 }} />
    )
    viaStyle.renderer.captureScreenshot(shot("style"))
    viaStyle.unmount()

    expect(fs.readFileSync(shot("class")).equals(fs.readFileSync(shot("style")))).toBe(true)
  })

  it("lets the style prop beat the class in every state", () => {
    const test = root()
    test.render(
      <div
        className="w-full h-full bg-[#7c6cff] hover:bg-[#22c55e]"
        style={{ backgroundColor: "#e11d48" }}
      />
    )
    test.renderer.captureScreenshot(shot("inline-wins"))
    test.unmount()

    const expected = root()
    expected.render(<div style={{ width: "100%", height: "100%", backgroundColor: "#e11d48" }} />)
    expected.renderer.captureScreenshot(shot("inline-wins-expected"))
    expected.unmount()

    expect(
      fs.readFileSync(shot("inline-wins")).equals(fs.readFileSync(shot("inline-wins-expected")))
    ).toBe(true)
  })
})

describeNative("selector classes", () => {
  const FRAME = {
    ...BASE,
    ...PALETTES.midnight,
    width: "100%",
    height: "100%",
    backgroundColor: "var(--color-bg)",
  } as const

  it("paints divide-y the same as borders written by hand", () => {
    const viaClass = root()
    viaClass.render(
      <div style={FRAME}>
        <div className="col divide-y">
          <div className="p-3" />
          <div className="p-3" />
          <div className="p-3" />
        </div>
      </div>
    )
    viaClass.renderer.captureScreenshot(shot("divide"))
    viaClass.unmount()

    const line = { borderBottomWidth: 1, borderColor: "var(--color-line)" } as const
    const viaStyle = root()
    viaStyle.render(
      <div style={FRAME}>
        <div className="col">
          <div className="p-3" style={line} />
          <div className="p-3" style={line} />
          <div className="p-3" />
        </div>
      </div>
    )
    viaStyle.renderer.captureScreenshot(shot("divide-expected"))
    viaStyle.unmount()

    expect(fs.readFileSync(shot("divide")).equals(fs.readFileSync(shot("divide-expected")))).toBe(
      true
    )
  })

  it("paints last: on the row that is last right now", () => {
    const viaClass = root()
    viaClass.render(
      <div style={FRAME}>
        <div className="col">
          <div className="p-3 last:bg-brand" />
          <div className="p-3 last:bg-brand" />
        </div>
      </div>
    )
    viaClass.renderer.captureScreenshot(shot("last"))
    viaClass.unmount()

    const viaStyle = root()
    viaStyle.render(
      <div style={FRAME}>
        <div className="col">
          <div className="p-3" />
          <div className="p-3" style={{ backgroundColor: "var(--color-brand)" }} />
        </div>
      </div>
    )
    viaStyle.renderer.captureScreenshot(shot("last-expected"))
    viaStyle.unmount()

    expect(fs.readFileSync(shot("last")).equals(fs.readFileSync(shot("last-expected")))).toBe(true)
  })
})

describeNative("height: auto", () => {
  const WORDS =
    "The measurement runs at the width the element really gets, so the same " +
    "words wrap into a different number of lines in a different column."

  const column = (width: number) => (
    <div style={{ width, display: "flex", flexDirection: "column" }}>
      <motion.div
        initial={{ height: 0 }}
        animate={{ height: "auto" }}
        transition={{ duration: 0.5, ease: "linear" }}
        style={{ display: "flex", flexDirection: "column" }}
      >
        <text>{WORDS}</text>
      </motion.div>
    </div>
  )

  /// The same words in two widths. The narrow column wraps into more lines, so
  /// it has to settle taller. Neither number is written anywhere, and measuring
  /// at max-content instead would give both of them one line.
  it("settles at the height the content takes at each width", () => {
    const settled = [220, 440].map((width) => {
      const test = root()
      test.renderer.clockPause()
      test.render(column(width))
      const id = test.renderer.findByType("div")[1]!.id
      test.renderer.clockFastForward(2000)
      const height = test.renderer.getElementBounds(id)?.[3] ?? -1
      test.renderer.clockResume()
      test.unmount()
      return height
    })
    expect(settled[0]).toBeGreaterThan(0)
    expect(settled[1]).toBeGreaterThan(0)
    expect(settled[0]).toBeGreaterThan(settled[1]!)
  })

  it("animates open and reaches the measured height", () => {
    const test = root()
    test.renderer.clockPause()
    test.render(column(300))
    const id = test.renderer.findByType("div")[1]!.id
    const at = () => test.renderer.getElementBounds(id)?.[3] ?? -1
    const start = at()
    test.renderer.clockFastForward(250)
    const middle = at()
    test.renderer.clockFastForward(500)
    const end = at()
    test.renderer.clockResume()
    test.unmount()
    expect(start).toBe(0)
    expect(middle).toBeGreaterThan(start)
    expect(end).toBeGreaterThan(middle)
  })

  it("collapses from the height it reached and turns back without a jump", () => {
    const test = root()
    test.renderer.clockPause()
    const tree = (open: boolean) => (
      <div style={{ width: 300, display: "flex", flexDirection: "column" }}>
        <motion.div
          initial={{ height: 0 }}
          animate={{ height: open ? "auto" : 0 }}
          transition={{ duration: 1, ease: "linear" }}
          style={{ display: "flex", flexDirection: "column" }}
        >
          <div style={{ width: 120, height: 100 }} />
        </motion.div>
      </div>
    )
    test.render(tree(true))
    const id = test.renderer.findByType("div")[1]!.id
    const at = () => test.renderer.getElementBounds(id)?.[3] ?? -1
    test.renderer.clockFastForward(1000)
    expect(at()).toBe(100)
    test.render(tree(false))
    expect(at()).toBe(100)
    test.renderer.clockFastForward(500)
    expect(at()).toBe(50)
    test.renderer.clockFastForward(500)
    expect(at()).toBe(0)

    // Turn back part way through, which starts from a frame that is part
    // pixels and part content.
    test.render(tree(true))
    test.renderer.clockFastForward(500)
    expect(at()).toBe(50)
    test.render(tree(false))
    expect(at()).toBe(50)
    test.renderer.clockFastForward(500)
    expect(at()).toBe(25)
    test.renderer.clockResume()
    test.unmount()
  })
})

describeNative("the scrollbars panel", () => {
  it("scrollIntoView honours scroll-padding and scroll-margin", () => {
    const test = root()
    test.render(
      <div
        style={{
          ...BASE,
          ...PALETTES.midnight,
          width: "100%",
          height: "100%",
          padding: 16,
          backgroundColor: "var(--color-bg)",
        }}
      >
        <IntoView />
      </div>
    )
    const box = test.renderer.findByTestId("into-view-box")!
    expect(test.renderer.getScrollOffset(box.id)![1]).toBe(0)

    const start = test.renderer.findByText("start")!
    const [x, y] = test.renderer.getElementBounds(start.id)!
    test.renderer.nativeSimulateClick(x + 4, y + 4)

    expect(test.renderer.getScrollOffset(box.id)![1]).toBeLessThan(0)
    const [, boxY] = test.renderer.getElementBounds(box.id)!
    const row = test.renderer.findByTestId("into-view-target")!
    const [, rowY] = test.renderer.getElementBounds(row.id)!
    // 12px of scroll-padding plus 16px of scroll-margin, inside the border.
    expect(rowY - boxY).toBeGreaterThanOrEqual(28)
    expect(rowY - boxY).toBeLessThanOrEqual(30)
    test.unmount()
  })

  it("the two-axes box scrolls both ways at the real page width", () => {
    const test = root()
    test.render(
      <div
        style={{
          ...BASE,
          ...PALETTES.midnight,
          width: "100%",
          height: "100%",
          padding: 16,
          overflowY: "scroll",
          backgroundColor: "var(--color-bg)",
        }}
      >
        <Scrollbars />
      </div>
    )
    const box = test.renderer.findByTestId("two-axes-box")!
    // The box is the third panel of the stack, near the bottom of the
    // window, and a wheel outside the window moves nothing. Put the box
    // in the middle first.
    test.renderer.scrollIntoView(box.id, "center")
    test.renderer.flush()
    const [x, y, width] = test.renderer.getElementBounds(box.id)!
    // The content must be wider than the box even on a wide window.
    const inner = test.renderer.findByTestId("two-axes-inner")!
    expect(test.renderer.getElementBounds(inner.id)![2]).toBeGreaterThan(width)
    test.renderer.nativeSimulateScrollWheel(x + 40, y + 40, -60, -60)
    test.renderer.flush()
    const offset = test.renderer.getScrollOffset(box.id)!
    expect(offset[0]).toBeLessThan(0)
    expect(offset[1]).toBeLessThan(0)
    test.unmount()
  })
})

describeNative("the navigation panel", () => {
  it("pushes the General screen from the right and pops it back", () => {
    const test = root()
    test.render(
      <div
        style={{
          ...BASE,
          ...PALETTES.midnight,
          width: "100%",
          height: "100%",
          padding: 16,
          backgroundColor: "var(--color-bg)",
        }}
      >
        <Navigation />
      </div>
    )
    test.renderer.clockPause()

    const general = test.renderer.findByTestId("nav-row-General")!
    const [gx, gy] = test.renderer.getElementBounds(general.id)!
    test.renderer.nativeSimulateClick(gx + 4, gy + 4)

    // At the start of the push, the General screen sits one screen width to
    // the right of where it will rest. The phone is 320 wide with a 1px
    // border on each side, so the screen is 318.
    const about = test.renderer.findByText("About")!
    const startX = test.renderer.getElementBounds(about.id)![0]
    test.renderer.clockFastForward(600)
    const endX = test.renderer.getElementBounds(about.id)![0]
    expect(startX - endX).toBeCloseTo(318, 0)

    const back = test.renderer.findByTestId("nav-back")!
    const [bx, by] = test.renderer.getElementBounds(back.id)!
    test.renderer.nativeSimulateClick(bx + 4, by + 4)
    test.renderer.clockFastForward(600)
    expect(test.renderer.findByTestId("nav-row-General")).toBeDefined()
    expect(test.renderer.findByText("About")).toBeUndefined()

    test.renderer.clockResume()
    test.unmount()
  })
})

describeNative("the whole application", () => {
  /// Walk the sidebar and paint each section, so the whole application is
  /// covered rather than the one it opens on. The test renderer has the frame
  /// overlay, so the performance panel is in the walk too.
  it("paints every section the sidebar reaches", () => {
    const test = root()
    test.render(<App />)
    expect(test.renderer.getPaintedText()).toContain("GPUIX")

    for (const title of ["Lengths", "Variables", "Inheritance", "className", "Selectors", "Motion", "Scrollbars", "Navigation", "Performance", "Colours"]) {
      const item = test.renderer.findByText(title)
      expect(item, `no sidebar item named ${title}`).toBeDefined()
      const bounds = test.renderer.getElementBounds(item!.id)
      expect(bounds).not.toBeNull()
      test.renderer.nativeSimulateClick(bounds![0]! + 4, bounds![1]! + 4)
      test.renderer.flush()
      test.renderer.captureScreenshot(shot(`app-${title.toLowerCase()}`))
      expect(test.renderer.getPaintedText().length).toBeGreaterThan(4)
    }
    test.unmount()
  })

  /// A frame that changes nothing must resolve nothing. GPUI rebuilds its
  /// element tree every frame, so this is what stops the rebuild from
  /// repeating the style work.
  it("resolves nothing on a frame that changed nothing", () => {
    const test = root()
    test.render(<App />)
    test.renderer.resetStyleResolutions()
    for (let frame = 0; frame < 5; frame += 1) test.renderer.flush()
    expect(test.renderer.styleResolutions()).toBe(0)
    test.unmount()
  })

  /// The palette is one declaration at the root, and every class reads it
  /// through `var()`. Changing it has to reach the whole tree.
  it("repaints the tree when the palette changes", () => {
    const test = root()
    test.render(<App />)
    test.renderer.captureScreenshot(shot("palette-before"))
    const paper = test.renderer.findByText("paper")
    expect(paper).toBeDefined()
    const bounds = test.renderer.getElementBounds(paper!.id)
    expect(bounds).not.toBeNull()
    test.renderer.nativeSimulateClick(bounds![0] + 4, bounds![1] + 4)
    test.renderer.flush()
    test.renderer.captureScreenshot(shot("palette-after"))
    // Metal on the macOS CI VM returns stale captures, so the two files
    // come out byte-identical there no matter what painted.
    if (!process.env.CI) {
      const before = fs.readFileSync(shot("palette-before"))
      const after = fs.readFileSync(shot("palette-after"))
      expect(before.equals(after)).toBe(false)
    }
    test.unmount()
  })
})
