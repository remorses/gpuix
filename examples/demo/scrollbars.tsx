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
        <Sample
          label={`scrollbarColor: "…brand …track"`}
          hint="Scroll to see it. An overlay bar only paints while the box scrolls."
        >
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
      note="Overlay bars, the macOS default, reserve no gutter, so these three boxes look the same. That matches the web. Start the demo with GPUIX_SCROLLBARS=classic to see the differences: stable reserves the gutter even though the content fits, and both-edges adds one more at the start."
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
        testId="two-axes-box"
        className="rounded border w-full"
        style={{ height: 180, overflow: "scroll", backgroundColor: "var(--color-raised)" }}
      >
        <div
          testId="two-axes-inner"
          className="col gap-2 p-3"
          style={{
            // The old width of 900 fit inside the box on a wide
            // window, and then the x axis had nothing to scroll.
            width: 2000,
            height: 400,
            flexShrink: 0,
            backgroundImage: "linear-gradient(135deg, var(--color-brand-soft), var(--color-raised))",
          }}
        >
          <text className="text-xs text-muted">2000 x 400 of content in a 180px tall box.</text>
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
      // container: "nearest" keeps the page still while the box scrolls.
      renderer.scrollIntoView?.(target.current.id, block, undefined, undefined, "nearest")
    }
  }
  return (
    <Panel
      title="scrollIntoView, scroll-margin and scroll-padding"
      note="The buttons scroll row 10 into view with container: nearest, so only this box moves and the page stays put. The box keeps 12px of scroll-padding inside its edges, and the row asks for 16px of scroll-margin around itself, so 28px of space separates the row from the edge."
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

/// A carousel that rests centered on a card. The container declares the
/// snap axis, each card declares where it lands.
function Snap() {
  const palette = ["#7c6cff", "#22c55e", "#e11d48", "#f59e0b", "#06b6d4", "#a855f7"]
  // Enough cards for a long fling to cross many snap positions.
  const colors = Array.from({ length: 30 }, (_, i) => palette[i % palette.length])
  return (
    <Panel
      title="scroll-snap-type, scroll-snap-align and scroll-marker-group"
      note="Scroll the carousel and let go. When the scroll rests, the box glides to the nearest card and centers it. The third card sets scroll-snap-stop: always, so a long scroll cannot pass over it. scroll-marker-group: after adds one dot per card along the bottom edge. A click on a dot glides to its card, and the dot of the nearest card paints stronger."
    >
      <div
        className="row rounded border w-full"
        style={{
          height: 150,
          overflowX: "auto",
          scrollSnapType: "x mandatory",
          scrollMarkerGroup: "after",
          scrollBehavior: "smooth",
          backgroundColor: "var(--color-raised)",
        }}
      >
        {colors.map((color, i) => (
          <div
            key={i}
            className="col items-center justify-center rounded"
            style={{
              width: 200,
              height: 110,
              margin: 12,
              flexShrink: 0,
              scrollSnapAlign: "center",
              scrollSnapStop: i === 2 ? "always" : undefined,
              backgroundColor: color,
            }}
          >
            <text className="text-sm font-semibold" style={{ color: "#ffffff" }}>
              {i === 2 ? `card ${i + 1}, stop: always` : `card ${i + 1}`}
            </text>
          </div>
        ))}
      </div>
    </Panel>
  )
}

/// Programmatic scrolls glide when the box asks for scroll-behavior: smooth.
function Smooth() {
  const { renderer } = useGpuix()
  const box = useRef<{ id: number } | null>(null)
  const go = (y: number) => {
    if (renderer && box.current) {
      renderer.scrollTo?.(box.current.id, 0, y)
    }
  }
  return (
    <Panel
      title="scroll-behavior: smooth"
      note="The buttons set the scroll offset. The box declares scroll-behavior: smooth, so the offset glides instead of jumping, and a wheel move cancels the glide."
    >
      <Row>
        <Button label="top" onClick={() => go(0)} />
        {/* 30 rows of 22 plus 29 gaps of 8 plus 24 of padding is 916
            of content. The box shows 150, so the range is 766 and the
            middle offset is -383, with row 15 near the center. */}
        <Button label="middle" onClick={() => go(-383)} />
        <Button label="bottom" onClick={() => go(-10000)} />
      </Row>
      <div
        ref={box}
        className="col rounded border w-full"
        style={{
          height: 150,
          overflowY: "auto",
          scrollBehavior: "smooth",
          backgroundColor: "var(--color-raised)",
        }}
      >
        <Rows count={30} />
      </div>
    </Panel>
  )
}

/// A progress bar the scroll offset drives. The box publishes a timeline
/// under a name, and the bar's motion points its animation-timeline at it,
/// so the scroll offset replaces the clock.
function Timeline() {
  return (
    <Panel
      title="scroll-timeline and animation-timeline"
      note="The box declares scroll-timeline: --reading. The bar above it points animation-timeline at that name, so its motion runs from initial to animate as the box scrolls, with no clock. Scroll the box and the bar follows, in both directions."
    >
      <div className="col gap-2 w-full">
        <div
          className="rounded"
          style={{ width: 320, height: 6, backgroundColor: "var(--color-track)" }}
        >
          <div
            className="rounded"
            style={{ height: 6, animationTimeline: "--reading", backgroundColor: "var(--color-brand)" }}
            motion={{
              initial: { width: 0 },
              animate: { width: 320 },
              transition: { ease: "linear" },
            }}
          />
        </div>
        <div
          className="col rounded border w-full"
          style={{
            height: 150,
            overflowY: "auto",
            scrollTimeline: "--reading block",
            backgroundColor: "var(--color-raised)",
          }}
        >
          <Rows count={30} />
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
      <Snap />
      <Timeline />
      <Smooth />
    </>
  )
}
