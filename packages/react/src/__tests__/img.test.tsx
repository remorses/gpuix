/// Tests for GPUIX custom <img> element — validates native image rendering
/// via the custom-element pipeline and visual screenshot behavior.

import fs from "fs"
import { beforeEach, describe, expect, it } from "vitest"
import React, { useState } from "react"
import { createTestRoot, hasNativeTestRenderer, type TestRoot } from "../testing"
import { bufferSimilarity, isCI, SHOTS_DIR } from "./test-utils"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

const IMAGE_FIXTURE_PATH = `${SHOTS_DIR}/gpuix-img-fixture.svg`
const SVG_FIXTURE = [
  '<svg xmlns="http://www.w3.org/2000/svg" width="240" height="140" viewBox="0 0 240 140">',
  '<rect x="0" y="0" width="240" height="140" fill="#1e2d59"/>',
  '<rect x="16" y="16" width="208" height="108" rx="14" fill="#5ca9ff"/>',
  '<circle cx="68" cy="70" r="24" fill="#ffd166"/>',
  '<rect x="112" y="50" width="88" height="14" rx="7" fill="#20304f"/>',
  '<rect x="112" y="74" width="70" height="12" rx="6" fill="#2a3c61"/>',
  "</svg>",
].join("")
const SVG_DATA_URL = `data:image/svg+xml;base64,${Buffer.from(SVG_FIXTURE).toString("base64")}`

function writeSvgFixture(filePath: string): void {
  fs.writeFileSync(filePath, SVG_FIXTURE, "utf8")
}

describeNative("custom element: img", () => {
  let testRoot: TestRoot

  beforeEach(() => {
    writeSvgFixture(IMAGE_FIXTURE_PATH)
    testRoot = createTestRoot()
  })

  describe("rendering", () => {
    it("should create img element and forward src/objectFit props", () => {
      function App() {
        return (
          <div style={{ width: 400, height: 240 }}>
            <img
              src={IMAGE_FIXTURE_PATH}
              objectFit="cover"
              style={{ width: 220, height: 120 }}
            />
          </div>
        )
      }

      testRoot.render(<App />)

      const images = testRoot.renderer.findByType("img")
      expect(images.length).toBe(1)
      const image = images[0] as any
      expect(image.customProps?.src).toBe(IMAGE_FIXTURE_PATH)
      expect(image.customProps?.objectFit).toBe("cover")
    })

  })

  describe("screenshots", () => {
    it("renders base64 data URLs like filesystem images", () => {
      function App({ src }: { src: string }) {
        return <img src={src} style={{ width: 240, height: 140 }} />
      }

      const pathImage = `${SHOTS_DIR}/gpuix-img-path.png`
      const dataImage = `${SHOTS_DIR}/gpuix-img-data-url.png`
      if (fs.existsSync(pathImage)) fs.unlinkSync(pathImage)
      if (fs.existsSync(dataImage)) fs.unlinkSync(dataImage)

      testRoot.render(<App src={IMAGE_FIXTURE_PATH} />)
      testRoot.renderer.flush()
      testRoot.renderer.flush()
      testRoot.renderer.captureScreenshot(pathImage)

      testRoot.render(<App src={SVG_DATA_URL} />)
      testRoot.renderer.flush()
      testRoot.renderer.flush()
      testRoot.renderer.captureScreenshot(dataImage)

      if (!isCI) {
        expect(
          bufferSimilarity(fs.readFileSync(pathImage), fs.readFileSync(dataImage))
        ).toBeGreaterThan(0.99)
      }
    })

    it("should capture screenshot changes after image source is set", () => {
      function ImageScreenshotProbe() {
        const [loaded, setLoaded] = useState(false)

        return (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              width: "100%",
              height: "100%",
              backgroundColor: "#0f111a",
            }}
          >
            <div
              style={{
                width: 420,
                height: 260,
                display: "flex",
                flexDirection: "column",
                gap: 12,
                padding: 18,
                borderRadius: 16,
                backgroundColor: "#1d2135",
              }}
              onClick={() => setLoaded(true)}
            >
              <text style={{ color: "#b3bddf", fontSize: 13 }}>
                click panel to load image
              </text>
              <img
                src={loaded ? IMAGE_FIXTURE_PATH : ""}
                objectFit="cover"
                style={{ width: 300, height: 170, borderRadius: 12 }}
              />
            </div>
          </div>
        )
      }

      testRoot.render(<ImageScreenshotProbe />)

      const path0 = `${SHOTS_DIR}/gpuix-img-0.png`
      const path1 = `${SHOTS_DIR}/gpuix-img-1.png`

      if (fs.existsSync(path0)) fs.unlinkSync(path0)
      if (fs.existsSync(path1)) fs.unlinkSync(path1)

      testRoot.renderer.captureScreenshot(path0)

      // Click centered panel to set src and start image load.
      testRoot.renderer.nativeSimulateClick(640, 400)
      // Drive extra frames to allow async image decode/load before snapshot.
      testRoot.renderer.flush()
      testRoot.renderer.flush()
      testRoot.renderer.flush()
      testRoot.renderer.captureScreenshot(path1)

      expect(fs.existsSync(path0)).toBe(true)
      expect(fs.existsSync(path1)).toBe(true)
      expect(fs.statSync(path0).size).toBeGreaterThan(0)
      expect(fs.statSync(path1).size).toBeGreaterThan(0)

      // Skipped on CI: Metal on macOS VMs doesn't repaint between captures.
      if (!isCI) {
        const before = fs.readFileSync(path0)
        const after = fs.readFileSync(path1)
        expect(bufferSimilarity(before, after)).toBeLessThan(0.99)
      }
    })
  })
})

describeNative("custom element: svg", () => {
  it("spring-rotates a raw SVG without React frames", () => {
    const testRoot = createTestRoot({ width: 120, height: 120 })
    const source = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M3 4h15l-4 5 4 5H3z"/></svg>'
    const beforePath = `${SHOTS_DIR}/gpuix-svg-rotation-0.png`
    const afterPath = `${SHOTS_DIR}/gpuix-svg-rotation-1.png`

    try {
      testRoot.render(<svg source={source} rotation={0} style={{ width: 72, height: 72, color: "#ffffff" }} />)
      testRoot.renderer.captureScreenshot(beforePath)
      testRoot.render(<svg source={source} rotation={90} style={{ width: 72, height: 72, color: "#ffffff" }} />)
      for (let frame = 0; frame < 40; frame += 1) {
        testRoot.renderer.advanceTime(16)
        testRoot.renderer.flush()
      }
      testRoot.renderer.captureScreenshot(afterPath)

      expect(testRoot.renderer.findByType("svg")[0]?.customProps?.rotation).toBe(90)
      if (!isCI) {
        const before = fs.readFileSync(beforePath)
        const after = fs.readFileSync(afterPath)
        expect(bufferSimilarity(before, after)).toBeLessThan(0.99)
      }
    } finally {
      testRoot.unmount()
    }
  })

  it("renders raw SVG source with the style color", () => {
    const testRoot = createTestRoot()

    function SvgScreenshotProbe() {
      const [loaded, setLoaded] = useState(false)

      return (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: "100%",
            height: "100%",
            backgroundColor: "#0f111a",
          }}
          onClick={() => setLoaded(true)}
        >
          <svg
            source={loaded ? SVG_FIXTURE : ""}
            style={{ width: 240, height: 140, color: "#5ca9ff" }}
          />
        </div>
      )
    }

    testRoot.render(<SvgScreenshotProbe />)

    const beforePath = `${SHOTS_DIR}/gpuix-svg-0.png`
    const afterPath = `${SHOTS_DIR}/gpuix-svg-1.png`
    if (fs.existsSync(beforePath)) fs.unlinkSync(beforePath)
    if (fs.existsSync(afterPath)) fs.unlinkSync(afterPath)

    testRoot.renderer.captureScreenshot(beforePath)
    testRoot.renderer.nativeSimulateClick(640, 400)
    testRoot.renderer.flush()
    testRoot.renderer.flush()
    testRoot.renderer.flush()
    testRoot.renderer.captureScreenshot(afterPath)

    expect(fs.existsSync(beforePath)).toBe(true)
    expect(fs.existsSync(afterPath)).toBe(true)
    if (!isCI) {
      const before = fs.readFileSync(beforePath)
      const after = fs.readFileSync(afterPath)
      expect(bufferSimilarity(before, after)).toBeLessThan(0.99)
    }
  })
})
