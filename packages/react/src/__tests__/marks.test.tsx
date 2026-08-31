import { afterEach, describe, expect, test } from "vitest"
import { createTestRoot } from "../testing.js"
import { OrganicBody, VectorEyes, DecorativeStroke, VectorPath, Canvas } from "../components/marks.js"
import { CanvasPathCommand } from "../components/marks.js"

describe("Living Vector Marks", () => {
  const root = createTestRoot()
  afterEach(() => root.unmount())

  test("renders VectorPath and OrganicBody to the native tree", () => {
    const path: CanvasPathCommand[] = [
      { type: "move", x: 0, y: 0 },
      { type: "line", x: 10, y: 10 }
    ]

    root.render(
      <div id="container">
        <VectorPath path={path} fill="#ff0000" />
        <OrganicBody path={path} fill="#00ff00" squash={0.5} breatheLoop={1.0} />
      </div>
    )

    const canvasElements = root.findType("canvas")
    expect(canvasElements.length).toBe(2)
    
    // First canvas
    const canvas1 = canvasElements[0]
    expect(canvas1.props.shapes).toBeDefined()
    expect(canvas1.props.shapes[0].type).toBe("path")
    expect(canvas1.props.shapes[0].fill).toBe("#ff0000")

    // Second canvas
    const canvas2 = canvasElements[1]
    expect(canvas2.props.shapes[0].type).toBe("body")
    expect(canvas2.props.shapes[0].squash).toBe(0.5)
    expect(canvas2.props.shapes[0].breatheLoop).toBe(1.0)
  })

  test("renders Eyes and Stroke marks", () => {
    const path: CanvasPathCommand[] = [{ type: "move", x: 0, y: 0 }]
    
    root.render(
      <div>
        <VectorEyes path={path} stroke="#ffffff" strokeWidth={2} glanceX={5} blink={0.5} blinkLoop={2.0} />
        <DecorativeStroke path={path} stroke="#000000" wiggle={3} wiggleLoop={1.5} />
      </div>
    )

    const canvasElements = root.findType("canvas")
    expect(canvasElements.length).toBe(2)

    expect(canvasElements[0].props.shapes[0].type).toBe("eyes")
    expect(canvasElements[0].props.shapes[0].glanceX).toBe(5)
    expect(canvasElements[0].props.shapes[0].blinkLoop).toBe(2.0)

    expect(canvasElements[1].props.shapes[0].type).toBe("stroke")
    expect(canvasElements[1].props.shapes[0].wiggle).toBe(3)
    expect(canvasElements[1].props.shapes[0].wiggleLoop).toBe(1.5)
  })
})
