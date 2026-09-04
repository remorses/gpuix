/// Native motion, and animating a `height` to `auto`.
///
/// `auto` is the height the content takes, and only layout knows that number.
/// The element asks taffy for a measured box, taffy calls back with the width
/// the parent gives it, and the content is measured at that width. So text
/// wraps at the width it will really have, whether the width came from a
/// declared length, from `flex`, from a percentage or from a stretched cross
/// axis.
///
/// Taffy computes one tree at a time, and the measure closure runs inside that
/// computation. The content is laid out in a second tree for the length of the
/// closure. `IsolatedLayout` in the pinned GPUI fork is what holds it.
///
/// A height is pixels plus a share of the content, so `auto` and a length are
/// the same kind of value. That is what lets a collapse start from the height
/// `auto` had, and lets a reversal start from a frame part way between the two.

import React, { useEffect, useState } from "react"
import { motion } from "@gpuix/react"
import type { MotionEase } from "@gpuix/react"
import { Button, Panel, Row } from "./ui.js"

const PARAGRAPH =
  "The measurement runs at the width the element really gets. Two columns of " +
  "different widths hold the same words, so the text wraps into a different " +
  "number of lines, and each box animates to its own height."

function Column({ width, open, label }: { width?: number; open: boolean; label: string }) {
  return (
    <div style={{ width: width ?? "100%", display: "flex", flexDirection: "column", gap: 6 }}>
      <text className="mono text-xs text-muted">{label}</text>
      <motion.div
        initial={{ height: 0 }}
        animate={{ height: open ? "auto" : 0 }}
        transition={{ duration: 0.5, ease: "easeOut" }}
        style={{
          // A box whose height animates has to clip, or the content paints
          // outside it whenever the height is shorter than the content. The
          // web asks the author for this too.
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
          gap: 8,
          borderRadius: 10,
          backgroundColor: "var(--color-raised)",
          borderWidth: 1,
          borderColor: "var(--color-line)",
        }}
      >
        <div style={{ padding: 12, display: "flex", flexDirection: "column", gap: 8 }}>
          <text className="text-sm leading-1.6 text-fg">{PARAGRAPH}</text>
          <div style={{ height: 28, borderRadius: 6, backgroundColor: "var(--color-brand)" }} />
        </div>
      </motion.div>
    </div>
  )
}

function Accordion() {
  const [open, setOpen] = useState(false)
  return (
    <Panel
      title="height: auto, measured at the real width"
      note="Both boxes hold the same words. The narrow one wraps into more lines, so it animates to a taller height. Neither number was written anywhere."
    >
      <Row>
        <Button label={open ? "Close" : "Open"} active={open} onClick={() => setOpen((on) => !on)} />
      </Row>
      <div className="row gap-4 items-start wrap">
        <Column width={240} open={open} label="width: 240" />
        <Column width={380} open={open} label="width: 380" />
        <div style={{ flexGrow: 1, minWidth: 200, display: "flex", flexDirection: "column" }}>
          <Column open={open} label="width from flexGrow" />
        </div>
      </div>
    </Panel>
  )
}

function Growing() {
  const [open, setOpen] = useState(false)
  const [lines, setLines] = useState(2)

  useEffect(() => {
    if (!open) return
    // Add a line part way through the opening animation, so the target height
    // moves while the animation is running.
    const timer = setTimeout(() => setLines((count) => count + 2), 200)
    return () => clearTimeout(timer)
  }, [open])

  return (
    <Panel
      title="Content that changes while the animation runs"
      note="The measurement repeats every frame, so the box follows the content instead of chasing a number it took once. Opening this one adds two rows after 200 milliseconds. The height keeps what is on screen and bends the rest of the curve toward the new rows, so it ends on time with no jump."
    >
      <Row>
        <Button
          label={open ? "Close" : "Open"}
          active={open}
          onClick={() => {
            setLines(2)
            setOpen((on) => !on)
          }}
        />
        <Button label="Add a row" onClick={() => setLines((count) => count + 1)} />
        <Button label={`rows: ${lines}`} onClick={() => setLines(2)} />
      </Row>
      <motion.div
        initial={{ height: 0 }}
        animate={{ height: open ? "auto" : 0 }}
        transition={{ duration: 0.8, ease: "easeOut" }}
        style={{
          // Clip for the same reason the accordion above clips.
          overflow: "hidden",
          width: 360,
          display: "flex",
          flexDirection: "column",
          borderRadius: 10,
          backgroundColor: "var(--color-raised)",
        }}
      >
        <div style={{ padding: 10, display: "flex", flexDirection: "column", gap: 6 }}>
          {Array.from({ length: lines }, (_, index) => (
            <div
              key={index}
              style={{
                height: 24,
                borderRadius: 6,
                backgroundColor: "color-mix(in oklch, var(--color-brand) 70%, var(--color-panel))",
              }}
            />
          ))}
        </div>
      </motion.div>
    </Panel>
  )
}

const EASES: MotionEase[] = [
  "linear",
  "ease",
  "easeIn",
  "easeOut",
  "easeInOut",
  [0.34, 1.56, 0.64, 1],
]

function Eases() {
  const [run, setRun] = useState(0)
  const far = run % 2 === 1
  return (
    <Panel
      title="Easing"
      note="The five named curves, and one cubic bezier that overshoots. Press play and watch them separate."
    >
      <Row>
        <Button label="Play" onClick={() => setRun((count) => count + 1)} />
      </Row>
      <div className="col gap-2">
        {EASES.map((ease) => (
          <div key={String(ease)} className="row items-center gap-3">
            <div style={{ width: 120 }}>
              <text className="mono text-xs text-muted">{JSON.stringify(ease)}</text>
            </div>
            <div style={{ flexGrow: 1, height: 22, borderRadius: 6, backgroundColor: "var(--color-track)", position: "relative" }}>
              <motion.div
                animate={{ left: far ? 300 : 0 }}
                transition={{ duration: 1, ease }}
                style={{
                  position: "absolute",
                  top: 3,
                  width: 16,
                  height: 16,
                  borderRadius: 8,
                  backgroundColor: "var(--color-brand)",
                }}
              />
            </div>
          </div>
        ))}
      </div>
    </Panel>
  )
}

function Properties() {
  const [on, setOn] = useState(false)
  return (
    <Panel
      title="The other animated properties"
      note="width, opacity, borderRadius and the four offsets. Every one of them is a number, so they interpolate without a measurement."
    >
      <Row>
        <Button label={on ? "Back" : "Go"} active={on} onClick={() => setOn((state) => !state)} />
      </Row>
      <div className="row gap-4 wrap items-start">
        <div className="col gap-2">
          <text className="mono text-xs text-muted">width</text>
          <motion.div
            animate={{ width: on ? 200 : 60 }}
            transition={{ duration: 0.5 }}
            style={{ height: 48, borderRadius: 8, backgroundColor: "var(--color-brand)" }}
          />
        </div>
        <div className="col gap-2">
          <text className="mono text-xs text-muted">opacity</text>
          <motion.div
            animate={{ opacity: on ? 0.15 : 1 }}
            transition={{ duration: 0.5 }}
            style={{ width: 120, height: 48, borderRadius: 8, backgroundColor: "var(--color-brand)" }}
          />
        </div>
        <div className="col gap-2">
          <text className="mono text-xs text-muted">borderRadius</text>
          <motion.div
            animate={{ borderRadius: on ? 24 : 2 }}
            transition={{ duration: 0.5 }}
            style={{ width: 120, height: 48, backgroundColor: "var(--color-brand)" }}
          />
        </div>
        <div className="col gap-2">
          <text className="mono text-xs text-muted">top and left</text>
          <div style={{ width: 160, height: 64, position: "relative", borderRadius: 8, backgroundColor: "var(--color-track)" }}>
            <motion.div
              animate={{ left: on ? 112 : 8, top: on ? 8 : 24 }}
              transition={{ duration: 0.5, ease: "easeInOut" }}
              style={{ position: "absolute", width: 40, height: 32, borderRadius: 8, backgroundColor: "var(--color-brand)" }}
            />
          </div>
        </div>
      </div>
    </Panel>
  )
}

export function Motion() {
  return (
    <div className="col gap-4">
      <Accordion />
      <Growing />
      <Eases />
      <Properties />
      <Panel
        title="Opening, closing and turning back"
        note="A height carries a number of pixels and a share of the height the content takes. `auto` is the whole share, a length is none of it, and a frame between the two is part of each. So a collapse starts from the height on screen, and pressing the button twice never jumps."
      >
        <text className="mono text-xs text-muted">
          open 0, 25, 50, 75, 100. close 100, 75, 50, 25, 0. turn back at 50 and it runs 50, 25, 0.
        </text>
      </Panel>
    </div>
  )
}
