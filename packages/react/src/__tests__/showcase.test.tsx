/// One screenshot with every native text element on screen at once.
///
/// This is the visual regression net for the whole stack: markdown typography,
/// syntax highlighting, diff gutters and the selection wash all land in one
/// image, so a change that breaks any of them is visible in a single diff.

import fs from "fs"
import path from "path"
import React from "react"
import { beforeAll, describe, expect, it } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"
import { expectScreenshotsDiffer, SHOTS_DIR } from "./test-utils.js"

// The GPU test renderer is macOS and Windows only.
const describeNative = hasNativeTestRenderer ? describe : describe.skip

const NOTES = [
  "## Release notes",
  "",
  "Text is now **selectable** across every element, including `<code>` and `<diff>`.",
  "",
  "- Tree-sitter highlighting runs in Rust",
  "- Diffs virtualize with GPUI's list",
].join("\n")

const SNIPPET = `async function main() {
  const renderer = createRenderer()
  renderer.init({ title: 'GPUIX' })
  startFrameLoop(renderer)
}`

const PATCH = [
  "diff --git a/src/server.ts b/src/server.ts",
  "--- a/src/server.ts",
  "+++ b/src/server.ts",
  "@@ -1,12 +1,14 @@",
  " import { createServer } from 'http'",
  " import { router } from './router'",
  " ",
  "-const port = 3000",
  "+const port = Number(process.env.PORT ?? 8080)",
  "+const host = process.env.HOST ?? '0.0.0.0'",
  " ",
  " export function start() {",
  "   const server = createServer(router)",
  "-  return server.listen(port)",
  "+  return server.listen(port, host)",
  " }",
  " ",
  " export function stop(server: Server) {",
  "-  server.close()",
  "+  return new Promise((done) => server.close(done))",
  " }",
].join("\n")

function Panel({
  title,
  grow,
  children,
}: {
  title: string
  grow?: boolean
  children: React.ReactNode
}) {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        flexGrow: 1,
        minWidth: 0,
        minHeight: grow ? 0 : undefined,
        gap: 10,
      }}
    >
      <text
        style={{
          fontSize: 10,
          color: "#8d8d8d",
          userSelect: "none",
        }}
      >
        {title}
      </text>
      {children}
    </div>
  )
}

beforeAll(() => {
  fs.mkdirSync(SHOTS_DIR, { recursive: true })
})

describeNative("native text showcase", () => {
  it("renders markdown, code and diff together", () => {
    const shot = path.join(SHOTS_DIR, "showcase.png")
    const { render, renderer } = createTestRoot()

    render(
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          width: "100%",
          height: "100%",
          padding: 24,
          gap: 20,
          backgroundColor: "#060606",
        }}
      >
        <div style={{ display: "flex", flexDirection: "row", gap: 24, flexShrink: 0 }}>
          <Panel title="MARKDOWN">
            <markdown source={NOTES} />
          </Panel>
          <Panel title="CODE">
            {/* `<code>` paints no card, so the card is here, in caller code. */}
            <code
              code={SNIPPET}
              language="typescript"
              showLineNumbers
              style={{
                padding: 12,
                borderRadius: 10,
                borderWidth: 1,
                borderColor: "#ffffff1f",
                backgroundColor: "#ffffff09",
              }}
            />
          </Panel>
        </div>
        <Panel title="DIFF" grow>
          {/* flexGrow + minHeight 0 lets the virtualized list take the rest of
              the window instead of leaving dead space under it. */}
           <diff scroll patch={PATCH} wordDiff style={{ flexGrow: 1, minHeight: 0 }} />
        </Panel>
      </div>
    )

    renderer.captureScreenshot(shot)

    // All three elements painted their own text into the same frame.
    const painted = renderer.getPaintedText()
    expect(painted).toContain("Release notes")
    expect(painted).toContain("  const renderer = createRenderer()")
    expect(painted).toContain("src/server.ts")

    expect(fs.existsSync(shot)).toBe(true)
    expect(fs.statSync(shot).size).toBeGreaterThan(0)
  })

  it("selects continuously from markdown into the code block", () => {
    const { render, renderer } = createTestRoot()

    render(
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          padding: 20,
          gap: 12,
          backgroundColor: "#060606",
        }}
      >
        <markdown source="heading text" />
        <code code={"code line one"} language="ts" />
      </div>
    )

    // One drag, two different native elements, one joined result.
    const selected = renderer.dragSelect(22, 30, 900, 600)
    expect(selected).toBe("heading text\ncode line one")
  })

  it("retunes every component from the theme metrics, with no rebuild", () => {
    const dense = path.join(SHOTS_DIR, "metrics-dense.png")
    const roomy = path.join(SHOTS_DIR, "metrics-roomy.png")

    const Doc = ({ theme }: { theme?: object }) => (
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          padding: 24,
          gap: 16,
          height: "100%",
          backgroundColor: "#060606",
        }}
      >
        <markdown source={NOTES} theme={theme} />
        <code code={SNIPPET} language="typescript" showLineNumbers theme={theme} />
        <diff scroll patch={PATCH} style={{ flexGrow: 1, minHeight: 0 }} theme={theme} />
      </div>
    )

    const a = createTestRoot()
    a.render(<Doc />)
    a.renderer.captureScreenshot(dense)

    const b = createTestRoot()
    b.render(
      <Doc
        theme={{
          metrics: {
            mdTextSize: 18,
            mdLineHeight: 30,
            mdBlockGap: 24,
            mdHeadingSizes: [28, 22, 18, 18],
            codeLineHeight: 28,
            codeTextSize: 16,
            diffLineHeight: 34,
            diffFileHeaderHeight: 52,
            diffGutterWidth: 56,
          },
        }}
      />
    )
    b.renderer.captureScreenshot(roomy)

    // Same text, different geometry: the metrics only move layout.
    expect(b.renderer.getPaintedText().length).toBeGreaterThan(0)
    expectScreenshotsDiffer(dense, roomy)
  })

  it("keeps the diff scroll model honest when row heights change", () => {
    // The virtualized list computes its height model from the metrics without
    // measuring. If a taller row height did not reach the list state, fewer
    // rows would fit but the list would still think it showed the old count.
    const short = createTestRoot()
    short.render(<diff scroll patch={PATCH} style={{ height: 200 }} />)
    const shortRows = short.renderer.getPaintedText().length

    const tall = createTestRoot()
    tall.render(
      <diff scroll patch={PATCH} style={{ height: 200 }} theme={{ metrics: { diffLineHeight: 60 } }} />
    )
    const tallRows = tall.renderer.getPaintedText().length

    expect(shortRows).toBeGreaterThan(0)
    expect(tallRows).toBeGreaterThan(0)
    expect(tallRows).toBeLessThan(shortRows)
  })

  it("captures the selection wash", () => {
    const before = path.join(SHOTS_DIR, "selection-before.png")
    const after = path.join(SHOTS_DIR, "selection-after.png")

    const { render, renderer } = createTestRoot()
    render(
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          padding: 24,
          gap: 12,
          backgroundColor: "#060606",
        }}
      >
        <markdown source={NOTES} />
      </div>
    )
    renderer.captureScreenshot(before)

    expect(renderer.dragSelect(26, 34, 900, 400)).not.toBeNull()
    renderer.captureScreenshot(after)

    expect(fs.statSync(before).size).toBeGreaterThan(0)
    expect(fs.statSync(after).size).toBeGreaterThan(0)
  })
})
