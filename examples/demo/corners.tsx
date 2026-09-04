/// Corner shapes.
///
/// CSS Borders 4 `corner-shape`. The quad shader draws each corner as a
/// superellipse, so a squircle costs the same as a circle: one quad, no
/// path. Shadows still follow the round corner.

import React, { useState } from "react"
import { motion } from "@gpuix/react"
import { Button, Grid, Panel, Sample, Swatch } from "./ui.js"

const SHAPES: Array<[string, string]> = [
  ["round", "the default, a quarter circle"],
  ["squircle", "superellipse(2), what app icons use"],
  ["square", "superellipse(infinity), the radius does nothing"],
  ["bevel", "superellipse(0), a straight cut"],
  ["scoop", "superellipse(-1), a quarter circle cut out"],
  ["notch", "superellipse(-infinity), a square cut out"],
  ["superellipse(0.5)", "between bevel and round"],
  ["superellipse(4)", "close to square"],
]

const SHORTHANDS: Array<[string, StyleShape]> = [
  ["cornerShape: bevel scoop", { cornerShape: "bevel scoop" }],
  ["cornerTopShape: squircle", { cornerTopShape: "squircle", borderRadius: 24 }],
  ["corner: 24px notch", { corner: "24px notch" }],
  ["cornerTopLeft: 40px bevel", { cornerTopLeft: "40px bevel", borderRadius: 8 }],
  ["cornerInlineEnd: 28px squircle", { cornerInlineEnd: "28px squircle" }],
  ["cornerShape: oval (invalid, dropped)", { cornerShape: "oval", borderRadius: 24 }],
]

type StyleShape = Record<string, string | number>

export function Corners() {
  return (
    <Panel
      title="Corner shape"
      note="Six keywords and superellipse(K). The shape only shows where the corner has a radius; each swatch here has borderRadius 24 unless the shorthand sets its own.">
      <Grid>
        {SHAPES.map(([shape, hint]) => (
          <Sample key={shape} label={shape} hint={hint}>
            <Swatch color="#ff5c8a" style={{ borderRadius: 24, cornerShape: shape }} />
          </Sample>
        ))}
      </Grid>
      <Grid>
        {SHORTHANDS.map(([label, style]) => (
          <Sample key={label} label={label}>
            <Swatch color="#5cc8ff" style={style} />
          </Sample>
        ))}
      </Grid>
      <Morph />
    </Panel>
  )
}

function Morph() {
  const [squircle, setSquircle] = useState(false)
  return (
    <div className="col gap-2" style={{ width: 200 }}>
      <motion.div
        style={{ width: "100%", height: 56, borderRadius: 28, backgroundColor: "#ffc857" }}
        initial={{ cornerShape: "notch" }}
        animate={{ cornerShape: squircle ? "squircle" : "notch" }}
        transition={{ duration: 1, ease: "easeInOut" }}
      />
      <Button label={squircle ? "Notch" : "Squircle"} active={squircle} onClick={() => setSquircle((on) => !on)} />
      <text className="text-xs text-faint">motion moves the shape in half-corner space, so it changes at an even pace</text>
    </div>
  )
}
