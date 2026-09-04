/// Filters, masks and blend modes.
///
/// Each of these makes the element paint itself and its children into a
/// texture of its own. The GPU then paints that texture over the frame with
/// the effect, so the content under it is untouched. `filter` functions
/// other than `blur()` fold into one colour matrix.

import React from "react"
import { Grid, Panel, Sample } from "./ui.js"

const FILTERS: Array<[string, string]> = [
  ["blur(6px)", "a Gaussian, sigma in pixels"],
  ["grayscale(1)", ""],
  ["sepia(1)", ""],
  ["invert(1)", ""],
  ["hue-rotate(150deg)", ""],
  ["saturate(3)", ""],
  ["brightness(0.5) contrast(2)", "two functions, in order"],
  ["opacity(0.4)", "same as opacity, as a filter"],
]

const BLENDS = [
  "normal", "multiply", "screen", "overlay", "darken", "lighten", "color-dodge", "color-burn",
  "hard-light", "soft-light", "difference", "exclusion", "hue", "saturation", "color", "luminosity",
]

function Picture() {
  return (
    <div style={{ width: 120, height: 72, borderRadius: 10, backgroundImage: "linear-gradient(120deg, #ff5c8a, #ffd166, #5cc8ff)", padding: 10 }}>
      <text style={{ color: "#ffffff", fontWeight: 600 }}>Aa</text>
    </div>
  )
}

function Filters() {
  return (
    <Panel title="filter" note="Every function of Filter Effects 1 except drop-shadow() and url(). The blur bleeds past the box, like on the web.">
      <Grid>
        {FILTERS.map(([value, hint]) => (
          <Sample key={value} label={value} hint={hint}>
            <div style={{ filter: value }}>
              <Picture />
            </div>
          </Sample>
        ))}
      </Grid>
    </Panel>
  )
}

function Blends() {
  return (
    <Panel title="mix-blend-mode" note="A magenta disc over a cyan and yellow picture, one mode each. background-blend-mode takes the same words and mixes backgroundImage into backgroundColor.">
      <Grid>
        {BLENDS.map((mode) => (
          <Sample key={mode} label={mode}>
            <div style={{ position: "relative", width: 120, height: 72, borderRadius: 10, overflow: "hidden", backgroundImage: "linear-gradient(to right, #00d8ff, #ffe600)" }}>
              <div style={{ position: "absolute", left: 30, top: 6, width: 60, height: 60, borderRadius: 30, backgroundColor: "#ff2fa0", mixBlendMode: mode }} />
            </div>
          </Sample>
        ))}
      </Grid>
    </Panel>
  )
}

function Masks() {
  return (
    <Panel title="mask-image and backdrop-filter" note="A mask keeps each pixel by the alpha of a gradient. A backdrop filter changes what is under the box, clipped to its corners.">
      <Grid>
        <Sample label="maskImage" hint="linear-gradient(to right, black, transparent)">
          <div style={{ maskImage: "linear-gradient(to right, black, transparent)" }}>
            <Picture />
          </div>
        </Sample>
        <Sample label="maskImage, eased" hint="black 30%, ease-in-out, transparent">
          <div style={{ maskImage: "linear-gradient(to bottom, black 30%, ease-in-out, transparent)" }}>
            <Picture />
          </div>
        </Sample>
        <Sample label="backdropFilter" hint="blur(10px) saturate(1.6) on glass with a radius">
          <div style={{ position: "relative", width: 120, height: 72 }}>
            <Picture />
            <div style={{ position: "absolute", left: 20, top: 16, width: 80, height: 40, borderRadius: 12, backdropFilter: "blur(10px) saturate(1.6)", backgroundColor: "rgb(255 255 255 / 0.2)", borderWidth: 1, borderColor: "rgb(255 255 255 / 0.5)" }} />
          </div>
        </Sample>
        <Sample label="backdropFilter" hint="grayscale(1) on half the picture">
          <div style={{ position: "relative", width: 120, height: 72 }}>
            <Picture />
            <div style={{ position: "absolute", left: 60, top: 0, width: 60, height: 72, backdropFilter: "grayscale(1)" }} />
          </div>
        </Sample>
        <Sample label="background-blend-mode" hint="multiply of a gradient into a colour">
          <div style={{ width: 120, height: 72, borderRadius: 10, backgroundColor: "#ff2fa0", backgroundImage: "linear-gradient(to right, #00d8ff, #ffe600)", backgroundBlendMode: "multiply" }} />
        </Sample>
      </Grid>
    </Panel>
  )
}

export function Effects() {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      <Filters />
      <Masks />
      <Blends />
    </div>
  )
}
