import React from "react"
import { describe, expect, it } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "../testing"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

describeNative("browser", () => {
  it("retains the browser intrinsic while keeping native views disabled in test windows", () => {
    const root = createTestRoot({ width: 640, height: 480 })
    try {
      root.render(
        <browser
          source="https://example.com"
          profileId="workspace"
          profilePath="/tmp/gpuix-browser-test"
          visible
          command={JSON.stringify({ serial: 1, kind: "navigate", value: "https://example.com" })}
          style={{ width: 640, height: 480 }}
          onBrowserState={() => undefined}
          onBrowserOpen={() => undefined}
          onBrowserError={() => undefined}
        />,
      )
      root.renderer.flush()
      expect(root.renderer.findByType("browser")).toHaveLength(1)
      expect(root.renderer.supportsNativeBrowser()).toBe(false)
      expect(root.renderer.nativeBrowserEngine()).toBe("unavailable")
      expect(root.renderer.nativeBrowserProfileIsolation()).toBe("limited")
      expect(root.renderer.getRetainedElementCount()).toBe(1)
    } finally {
      root.unmount()
    }
  })
})
