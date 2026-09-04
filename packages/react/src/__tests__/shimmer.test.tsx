import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { createTestRoot, hasNativeTestRenderer, type TestRoot } from "../testing.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

describeNative("custom element: shimmer", () => {
  let root: TestRoot

  beforeEach(() => {
    root = createTestRoot({ width: 240, height: 40 })
  })

  afterEach(() => {
    root.unmount()
  })

  it("paints and updates one native animated text host", () => {
    root.render(
      <shimmer
        testId="working-shimmer"
        text="Working"
        baseColor="#777777"
        highlightColor="#eeeeee"
        duration={2}
        style={{ display: "flex", fontSize: 13, fontWeight: 650 }}
      >
        Working
      </shimmer>
    )
    root.renderer.flush()

    expect(root.renderer.findByType("shimmer")).toHaveLength(1)
    expect(root.renderer.getPaintedText()).toContain("Working")

    root.render(
      <shimmer
        testId="working-shimmer"
        text="Thinking"
        baseColor="#777777"
        highlightColor="#eeeeee"
        duration={1.5}
        style={{ display: "flex", fontSize: 13, fontWeight: 700 }}
      >
        Thinking
      </shimmer>
    )
    root.renderer.flush()

    expect(root.renderer.getPaintedText()).toContain("Thinking")
    expect(root.renderer.getPaintedText()).not.toContain("Working")
  })
})
