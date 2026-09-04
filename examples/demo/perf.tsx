/// What the style work costs.
///
/// GPUI rebuilds its element tree every frame, so the number that matters is
/// how much of that rebuild repeats work the renderer already did. A style
/// resolves once and is kept until the element, its class, or something it
/// inherits changes. An element that reads no variable keeps its resolution
/// through a change anywhere else in the tree.
///
/// The test renderer counts resolutions directly, through `styleResolutions()`.
/// A live window has no such counter, so this panel reads the frame timer that
/// GPUI already keeps. Turn the churn on: React re-renders the whole grid at
/// 60 frames a second with the same declarations, and the frame time does not
/// move.

import React, { useEffect, useState } from "react"
import type { EventPayload, NativeRenderer } from "@gpuix/react"
import { Button, Panel, Row } from "./ui.js"

/// The part of a renderer this panel reads. The overlay methods are optional
/// on `NativeRenderer`, so a renderer either has all four or is not one this
/// panel can show.
export type FrameOverlay = Required<
  Pick<
    NativeRenderer,
    | "getDebugFrameOverlay"
    | "cycleDebugFrameOverlay"
    | "getDebugFrameOverlayStats"
    | "resetDebugFrameOverlayStats"
  >
>

/// `renderer` as a `FrameOverlay`, or `null` when it has no frame overlay.
export function frameOverlay(renderer: NativeRenderer): FrameOverlay | null {
  const { getDebugFrameOverlay, cycleDebugFrameOverlay, getDebugFrameOverlayStats, resetDebugFrameOverlayStats } = renderer
  if (!getDebugFrameOverlay || !cycleDebugFrameOverlay || !getDebugFrameOverlayStats || !resetDebugFrameOverlayStats) {
    return null
  }
  return {
    getDebugFrameOverlay: getDebugFrameOverlay.bind(renderer),
    cycleDebugFrameOverlay: cycleDebugFrameOverlay.bind(renderer),
    getDebugFrameOverlayStats: getDebugFrameOverlayStats.bind(renderer),
    resetDebugFrameOverlayStats: resetDebugFrameOverlayStats.bind(renderer),
  }
}

const CELLS = Array.from({ length: 240 }, (_, index) => index)

function Grid({ tick }: { tick: number }) {
  return (
    <div className="row wrap gap-1">
      {CELLS.map((index) => (
        <div
          key={index}
          className="rounded"
          style={{
            width: 18,
            height: 18,
            backgroundColor: `oklch(0.7 0.14 ${(index * 7 + tick) % 360})`,
          }}
        />
      ))}
    </div>
  )
}

function Frames({ renderer }: { renderer: FrameOverlay }) {
  const [stats, setStats] = useState(() => renderer.getDebugFrameOverlayStats())
  const [overlay, setOverlay] = useState(() => renderer.getDebugFrameOverlay())
  const [churn, setChurn] = useState(false)
  const [tick, setTick] = useState(0)

  useEffect(() => {
    const timer = setInterval(() => setStats(renderer.getDebugFrameOverlayStats()), 250)
    return () => clearInterval(timer)
  }, [renderer])

  useEffect(() => {
    if (!churn) return
    const timer = setInterval(() => setTick((count) => count + 1), 16)
    return () => clearInterval(timer)
  }, [churn])

  const ms = (value?: number) => (value === undefined ? "-" : `${value.toFixed(2)} ms`)

  return (
    <Panel
      title="Frame time"
      note="The same numbers the on-screen overlay paints. Reset, then turn the churn on and watch p99."
    >
      <Row>
        <Button
          label={`overlay: ${overlay}`}
          onClick={() => setOverlay(renderer.cycleDebugFrameOverlay())}
        />
        <Button
          label="Reset"
          onClick={() => {
            renderer.resetDebugFrameOverlayStats()
            setStats(renderer.getDebugFrameOverlayStats())
          }}
        />
        <Button label={churn ? "Churn on" : "Churn off"} active={churn} onClick={() => setChurn((on) => !on)} />
      </Row>
      <div className="row gap-6 wrap">
        {[
          ["current", ms(stats.currentMs)],
          ["p90", ms(stats.p90Ms)],
          ["p99", ms(stats.p99Ms)],
          ["max", ms(stats.maxMs)],
          ["frames", String(stats.frames)],
          ["samples", String(stats.samples)],
        ].map(([label, value]) => (
          <div key={label} className="col gap-1">
            <text className="text-xs text-faint">{label}</text>
            <text className="mono text-lg text-fg">{value}</text>
          </div>
        ))}
      </div>
      <text className="text-xs text-faint">
        Churn re-renders 240 boxes every 16 milliseconds. Each one gets a new colour, so this is the
        expensive case rather than the cached one.
      </text>
      <Grid tick={churn ? tick : 0} />
    </Panel>
  )
}

const ROWS = 5000
/// Rows built around the visible range, so a scroll never waits for React.
const OVERDRAW_ROWS = 12

/// Windowing is the app's job: the list reports the visible range, and this
/// component renders that slice plus an overdraw. `windowStart` tells the
/// native list which logical index the first child is.
function Rows() {
  const [range, setRange] = useState({ start: 0, end: 30 })
  const start = Math.max(0, range.start - OVERDRAW_ROWS)
  const end = Math.min(ROWS, range.end + OVERDRAW_ROWS)
  return (
    <Panel
      title="A long list"
      note="Five thousand rows, of which only the ones near the viewport are built. Scroll it."
    >
      <div style={{ height: 260, borderRadius: 10, borderWidth: 1, borderColor: "var(--color-line)" }}>
        <virtual-list
          itemCount={ROWS}
          estimatedItemHeight={34}
          windowStart={start}
          style={{ width: "100%", height: 260 }}
          onVisibleRange={(event: EventPayload) => {
            setRange({ start: event.startIndex ?? 0, end: event.endIndex ?? 30 })
          }}
        >
          {Array.from({ length: end - start }, (_, offset) => {
            const index = start + offset
            return (
              <div
                key={index}
                className="row items-center gap-3 px-3"
                style={{
                  height: 34,
                  backgroundColor:
                    index % 2 === 0 ? "var(--color-panel)" : "var(--color-raised)",
                }}
              >
                <text className="mono text-xs text-faint">{String(index).padStart(4, "0")}</text>
                <div
                  style={{
                    width: 60 + (index % 7) * 30,
                    height: 8,
                    borderRadius: 4,
                    backgroundColor: `oklch(0.7 0.13 ${(index * 11) % 360})`,
                  }}
                />
              </div>
            )
          })}
        </virtual-list>
      </div>
    </Panel>
  )
}

export function Perf({ renderer }: { renderer: FrameOverlay }) {
  return (
    <div className="col gap-4">
      <Frames renderer={renderer} />
      <Rows />
    </div>
  )
}
