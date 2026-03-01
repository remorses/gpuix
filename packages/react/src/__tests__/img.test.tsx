/// Tests for GPUIX custom <img> element — validates native image rendering
/// via the custom-element pipeline and visual screenshot behavior.

import fs from "fs"
import { beforeEach, describe, expect, it } from "vitest"
import React, { useState } from "react"
import { createTestRoot, hasNativeTestRenderer } from "../testing"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

const IMAGE_FIXTURE_PATH = "/tmp/gpuix-img-fixture.svg"

function writeSvgFixture(filePath: string): void {
  const svg = [
    '<svg xmlns="http://www.w3.org/2000/svg" width="240" height="140" viewBox="0 0 240 140">',
    '<rect x="0" y="0" width="240" height="140" fill="#1e2d59"/>',
    '<rect x="16" y="16" width="208" height="108" rx="14" fill="#5ca9ff"/>',
    '<circle cx="68" cy="70" r="24" fill="#ffd166"/>',
    '<rect x="112" y="50" width="88" height="14" rx="7" fill="#20304f"/>',
    '<rect x="112" y="74" width="70" height="12" rx="6" fill="#2a3c61"/>',
    "</svg>",
  ].join("")
  fs.writeFileSync(filePath, svg, "utf8")
}

describeNative("custom element: img", () => {
  let testRoot: ReturnType<typeof createTestRoot>

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

      const path0 = "/tmp/gpuix-img-0.png"
      const path1 = "/tmp/gpuix-img-1.png"

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

      const before = fs.readFileSync(path0)
      const after = fs.readFileSync(path1)
      expect(before.equals(after)).toBe(false)
    })
  })
})
