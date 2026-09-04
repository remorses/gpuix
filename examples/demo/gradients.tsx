/// Gradient fills.
///
/// `linear-gradient()` reaches lightningcss as written. The engine fixes the
/// stops up the way CSS Images 3 says and the quad shader paints them, so a
/// gradient costs the same as a flat colour: one quad, no texture. Stop
/// positions are percentages. Radial and conic gradients are not painted yet.

import React from "react"
import { Grid, Panel, Sample, Swatch } from "./ui.js"

const DIRECTIONS: Array<[string, string]> = [
  ["linear-gradient(#ff5c8a, #5cc8ff)", "top to bottom, the default"],
  ["linear-gradient(to right, #ff5c8a, #5cc8ff)", "a side keyword"],
  ["linear-gradient(45deg, #ff5c8a, #5cc8ff)", "an angle, clockwise from top"],
  ["linear-gradient(0.75turn, #ff5c8a, #5cc8ff)", "the same in turns"],
  ["linear-gradient(to top right, #ff5c8a, #5cc8ff)", "a corner: the 50% line joins the other two corners"],
  ["linear-gradient(to bottom left, #ff5c8a, #5cc8ff)", ""],
]

const STOPS: Array<[string, string]> = [
  ["linear-gradient(to right, red, orange, yellow, green, blue, indigo, violet)", "seven stops, spread evenly"],
  ["linear-gradient(to right, #ff5c8a 30%, #5cc8ff 70%)", "flat colour outside the stops"],
  ["linear-gradient(to right, #ff5c8a 50%, #5cc8ff 50%)", "two stops in one place make a hard edge"],
  ["linear-gradient(to right, #ff5c8a, 20%, #5cc8ff)", "a hint moves the half-way point"],
  ["linear-gradient(to right, #ff5c8a 60%, #5cc8ff 20%)", "a stop never goes backwards"],
  ["linear-gradient(to right, var(--color-brand), white)", "a stop over a variable"],
]

const ALPHA: Array<[string, string]> = [
  ["linear-gradient(to right, rgb(255 92 138 / 0), #ff5c8a)", "fades in from clear"],
  ["linear-gradient(to bottom, transparent, black)", "a scrim"],
  ["linear-gradient(to right, currentColor, transparent)", "currentColor as a stop"],
]

function List({ title, note, entries }: {
  title: string
  note: string
  entries: Array<[string, string]>
}) {
  return (
    <Panel title={title} note={note}>
      <Grid>
        {entries.map(([value, hint]) => (
          <Sample key={value} label={value} hint={hint || undefined}>
            <Swatch style={{ backgroundImage: value }} />
          </Sample>
        ))}
      </Grid>
    </Panel>
  )
}

export function Gradients() {
  return (
    <div className="col gap-4" style={{ color: "#f9c74f" }}>
      <List
        title="Direction"
        note="An angle, a side or a corner. The gradient line runs through the centre and its length is the one CSS defines, so 0% and 100% land on the edges."
        entries={DIRECTIONS}
      />
      <List
        title="Colour stops"
        note="Up to eight stops. Missing positions spread evenly, and a position that steps back snaps to the one before it."
        entries={STOPS}
      />
      <List
        title="Alpha"
        note="A stop can be see-through. The gradient is the one fill of the box, so it paints over the parent, not over a backgroundColor on the same box."
        entries={ALPHA}
      />
    </div>
  )
}
