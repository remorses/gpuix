/// Scroll boxes, the bars they paint, and scrollIntoView.
///
/// The OS picks the kind of bar. An overlay bar floats over the content and
/// fades out after a scroll. A classic bar keeps a track and reserves a
/// gutter in the layout. Every box here also scrolls with the wheel, with a
/// drag on the thumb, and with a click in the track, which moves one page.

import React, { useRef } from "react"
import { useGpuix } from "@gpuix/react"
import type { StyleDesc } from "@gpuix/react"
import { Button, Grid, Panel, Row, Sample } from "./ui.js"

/// Rows tall enough to overflow the box, so a bar shows.
function Rows({ count }: { count: number }) {
  return (
    <div className="col gap-2 p-3">
      {Array.from({ length: count }, (_, i) => (
        <div key={i} className="row items-center gap-2" style={{ flexShrink: 0 }}>
          <div
            style={{
              width: 22,
              height: 22,
              borderRadius: 6,
              backgroundColor: "var(--color-track)",
            }}
          />
          <text className="text-xs text-muted">{`row ${i + 1}`}</text>
        </div>
      ))}
    </div>
  )
}

function ScrollBox({ style, count = 14 }: { style: StyleDesc; count?: number }) {
  return (
    <div
      className="col rounded border w-full"
      style={{ height: 150, backgroundColor: "var(--color-raised)", ...style }}
    >
      <Rows count={count} />
    </div>
  )
}

function Bars() {
  return (
    <Panel
      title="scrollbar-width and scrollbar-color"
      note="Scroll any box. The wheel, a drag on the thumb, and a click in the track all work. The track click moves one page."
    >
      <Grid>
        <Sample label={`overflowY: "auto"`} hint="The bar the OS picks.">
          <ScrollBox style={{ overflowY: "auto" }} />
        </Sample>
        <Sample label={`scrollbarWidth: "thin"`}>
          <ScrollBox style={{ overflowY: "auto", scrollbarWidth: "thin" }} />
        </Sample>
        <Sample label={`scrollbarWidth: "none"`} hint="No bar and no gutter. The wheel still scrolls.">
          <ScrollBox style={{ overflowY: "auto", scrollbarWidth: "none" }} />
        </Sample>
        <Sample label={`scrollbarColor: "…brand …track"`}>
          <ScrollBox
            style={{
              overflowY: "auto",
              scrollbarColor: "var(--color-brand) var(--color-track)",
            }}
          />
        </Sample>
      </Grid>
    </Panel>
  )
}

/// The content fits, so only the reserved gutter tells the boxes apart.
/// The full-width band paints the content area, and the gutter is the strip
/// the band does not cover.
function Gutters() {
  const band: StyleDesc = {
    height: 100,
    margin: 8,
    borderRadius: 6,
    backgroundColor: "var(--color-track)",
  }
  return (
    <Panel
      title="scrollbar-gutter"
      note="The content of these boxes fits. With classic bars, stable reserves the gutter anyway, and both-edges adds one more at the start. Overlay bars reserve nothing, so the three boxes then look the same. GPUIX_SCROLLBARS=classic|overlay picks the kind of bar."
    >
      <Grid>
        <Sample label={`scrollbarGutter: "auto"`}>
          <div className="col rounded border w-full" style={{ height: 126, overflowY: "auto", backgroundColor: "var(--color-raised)" }}>
            <div style={band} />
          </div>
        </Sample>
        <Sample label={`scrollbarGutter: "stable"`}>
          <div className="col rounded border w-full" style={{ height: 126, overflowY: "auto", scrollbarGutter: "stable", backgroundColor: "var(--color-raised)" }}>
            <div style={band} />
          </div>
        </Sample>
        <Sample label={`"stable both-edges"`}>
          <div className="col rounded border w-full" style={{ height: 126, overflowY: "auto", scrollbarGutter: "stable both-edges", backgroundColor: "var(--color-raised)" }}>
            <div style={band} />
          </div>
        </Sample>
      </Grid>
    </Panel>
  )
}

function BothAxes() {
  return (
    <Panel
      title="Two axes"
      note="overflow: scroll on both axes. The content is wider and taller than the box, so each axis gets its own bar."
    >
      <div
        className="rounded border w-full"
        style={{ height: 180, overflow: "scroll", backgroundColor: "var(--color-raised)" }}
      >
        <div
          className="col gap-2 p-3"
          style={{
            width: 900,
            height: 400,
            backgroundImage: "linear-gradient(135deg, var(--color-brand-soft), var(--color-raised))",
          }}
        >
          <text className="text-xs text-muted">900 x 400 of content in a smaller box.</text>
        </div>
      </div>
    </Panel>
  )
}

export function IntoView() {
  const { renderer } = useGpuix()
  const target = useRef<{ id: number } | null>(null)
  const show = (block: string) => {
    if (renderer && target.current) {
      renderer.scrollIntoView?.(target.current.id, block)
    }
  }
  return (
    <Panel
      title="scrollIntoView, scroll-margin and scroll-padding"
      note="The buttons scroll row 10 into view. The box keeps 12px of scroll-padding inside its edges, and the row asks for 16px of scroll-margin around itself, so 28px of space separates the row from the edge."
    >
      <Row>
        <Button label="start" onClick={() => show("start")} />
        <Button label="center" onClick={() => show("center")} />
        <Button label="end" onClick={() => show("end")} />
        <Button label="nearest" onClick={() => show("nearest")} />
      </Row>
      <div
        testId="into-view-box"
        className="col rounded border w-full"
        style={{ height: 180, overflowY: "auto", scrollPadding: 12, backgroundColor: "var(--color-raised)" }}
      >
        <div className="col gap-2 p-3">
          {Array.from({ length: 24 }, (_, i) => {
            const isTarget = i === 9
            return (
              <div
                key={i}
                ref={isTarget ? target : undefined}
                testId={isTarget ? "into-view-target" : undefined}
                className="row items-center gap-2 rounded px-2 py-1"
                style={{
                  flexShrink: 0,
                  scrollMargin: isTarget ? 16 : undefined,
                  backgroundColor: isTarget ? "var(--color-brand-soft)" : undefined,
                }}
              >
                <text className={isTarget ? "text-xs font-semibold text-fg" : "text-xs text-muted"}>
                  {isTarget ? "row 10, the target" : `row ${i + 1}`}
                </text>
              </div>
            )
          })}
        </div>
      </div>
    </Panel>
  )
}

export function Scrollbars() {
  return (
    <>
      <Bars />
      <Gutters />
      <BothAxes />
      <IntoView />
    </>
  )
}
