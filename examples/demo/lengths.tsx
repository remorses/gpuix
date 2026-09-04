/// Lengths, arithmetic and the units that do not resolve yet.
///
/// A bare number is pixels, which is what the `style` prop has always taken.
/// A string carries the unit, so `8`, `"8px"` and `"var(--pad)"` mean the same
/// padding. `line-height` and `opacity` keep the CSS meaning of a bare number,
/// which is a multiple rather than a length.
///
/// lightningcss folds the arithmetic while it parses, so `calc(8px + 12px)`
/// arrives as `20px`. Two shapes cannot fold. An expression that mixes a
/// percentage with an absolute length needs layout first, and GPUI has no
/// length type that carries an unfolded expression. A unit that needs the font
/// size or the window, such as `em`, `ch`, `vw` or `vh`, has nothing to read
/// yet. Both drop the declaration rather than paint a wrong size.
///
/// `width`, `height`, `minWidth`, `minHeight`, `maxWidth` and `maxHeight` read
/// the same lengths, and `auto` and a percentage on top. The last panel here is
/// about that.

import React from "react"
import type { StyleDesc } from "@gpuix/react"
import { Bar, Grid, Panel, Sample } from "./ui.js"

const SIMPLE: Array<[string, string | number, string]> = [
  ["100", 100, "a bare number is pixels"],
  ['"100px"', "100px", "the same length, with its unit"],
  ['"6rem"', "6rem", "96px at a 16px root"],
  ['"1in"', "1in", "96px"],
  ['"72pt"', "72pt", "96px"],
  ['"2cm"', "2cm", "about 76px"],
  ['"4pc"', "4pc", "64px"],
  ['"var(--spacing)"', "var(--spacing)", "4px"],
]

const FOLDED: Array<[string, string, string]> = [
  ['"calc(100px + 2rem)"', "calc(100px + 2rem)", "132px"],
  ['"calc(2rem * 3)"', "calc(2rem * 3)", "96px"],
  ['"min(180px, 12rem)"', "min(180px, 12rem)", "180px"],
  ['"max(40px, 6rem)"', "max(40px, 6rem)", "96px"],
  ['"clamp(60px, 8rem, 120px)"', "clamp(60px, 8rem, 120px)", "120px"],
  ['"calc(var(--spacing) * 30)"', "calc(var(--spacing) * 30)", "120px, the Tailwind spacing shape"],
]

const DROPPED: Array<[string, string, string]> = [
  ['"50%"', "50%", "a percentage padding resolves against layout"],
  ['"calc(100% - 8px)"', "calc(100% - 8px)", "a percentage next to a length needs layout"],
  ['"12vw"', "12vw", "the viewport units need the window"],
  ['"3em"', "3em", "em needs the font size of the element"],
  ['"4ch"', "4ch", "ch needs the font metrics"],
  ['"banana"', "banana", "not a length at all"],
]

function Bars({ title, note, entries }: {
  title: string
  note: string
  entries: Array<[string, string | number, string]>
}) {
  return (
    <Panel title={title} note={note}>
      <Grid>
        {entries.map(([label, length, hint]) => (
          <Sample key={label} label={label} hint={hint}>
            <Bar length={length} />
          </Sample>
        ))}
      </Grid>
    </Panel>
  )
}

const SENTENCE = "one two three four five six seven eight nine ten eleven twelve"

function Lines({ label, hint, declaration }: {
  label: string
  hint: string
  declaration: StyleDesc
}) {
  return (
    <Sample label={label} hint={hint}>
      <div
        style={{ width: 200, height: 120, fontSize: 16, color: "var(--color-fg)", ...declaration }}
      >
        <text>{SENTENCE}</text>
      </div>
    </Sample>
  )
}

export function Lengths() {
  return (
    <div className="col gap-4">
      <Bars
        title="One length, several ways to write it"
        note="Each bar is a box whose left padding is the value under it, inside a 200 pixel track."
        entries={SIMPLE}
      />
      <Bars
        title="Arithmetic"
        note="calc(), min(), max() and clamp() fold before the value reaches GPUI."
        entries={FOLDED}
      />
      <Bars
        title="What drops"
        note="Every bar here is empty. The value cannot finish without something the engine does not have yet, so the declaration drops and the element keeps what it had."
        entries={DROPPED}
      />

      <Panel
        title="Line height"
        note="A number is pixels. A bare number in a string is a multiple of the font size, the way CSS reads it. At 16px these four all mean 25.6 pixels, and a line height of zero declares nothing."
      >
        <Grid>
          <Lines label="lineHeight: 25.6" hint="pixels" declaration={{ lineHeight: 25.6 }} />
          <Lines label='lineHeight: "1.6"' hint="the same, as a CSS multiple" declaration={{ lineHeight: "1.6" }} />
          <Lines label='lineHeight: "160%"' hint="the same multiple" declaration={{ lineHeight: "160%" }} />
          <Lines label='lineHeight: "1.6rem"' hint="25.6px at a 16px root" declaration={{ lineHeight: "1.6rem" }} />
          <Lines label="lineHeight: 0" hint="declares nothing" declaration={{ lineHeight: 0 }} />
          <Lines label="nothing declared" hint="the GPUI default" declaration={{}} />
        </Grid>
      </Panel>

      <Panel
        title="The same parser on other properties"
        note="Padding, border width, gap and font size all read it."
      >
        <Grid>
          <Sample label='padding: "calc(var(--spacing) * 5)"' hint="20px on every side">
            <div style={{ padding: "calc(var(--spacing) * 5)", backgroundColor: "var(--color-brand)", borderRadius: 8 }}>
              <div style={{ height: 40, backgroundColor: "var(--color-panel)", borderRadius: 4 }} />
            </div>
          </Sample>
          <Sample label='borderWidth: "calc(1rem + 4px)"' hint="20px">
            <div style={{ height: 56, borderRadius: 8, borderWidth: "calc(1rem + 4px)", borderColor: "var(--color-brand)" }} />
          </Sample>
          <Sample label='gap: "1.5rem"' hint="24px between the two blocks">
            <div style={{ display: "flex", flexDirection: "column", gap: "1.5rem" }}>
              <div style={{ height: 20, borderRadius: 4, backgroundColor: "var(--color-brand)" }} />
              <div style={{ height: 20, borderRadius: 4, backgroundColor: "var(--color-brand)" }} />
            </div>
          </Sample>
          <Sample label='fontSize: "1.5rem"' hint="24px">
            <div style={{ height: 56, color: "var(--color-fg)", fontSize: "1.5rem" }}>
              <text>Aa</text>
            </div>
          </Sample>
        </Grid>
      </Panel>

      <Panel
        title="What width and height accept"
        note="width, height and the four min and max forms read every length above, and `auto` and a percentage on top of them. A value none of that can read drops on its own and leaves the rest of the style alone."
      >
        <Grid>
          <Sample label='width: "calc(100px + 2rem)"' hint="132px, through the parser">
            <div style={{ width: "calc(100px + 2rem)", height: 40, borderRadius: 6, backgroundColor: "var(--color-brand)" }} />
          </Sample>
          <Sample label='width: "60%"' hint="of the 200 pixel sample">
            <div style={{ width: "60%", height: 40, borderRadius: 6, backgroundColor: "var(--color-brand)" }} />
          </Sample>
          <Sample label='maxWidth: "6rem"' hint="96px, clamping a 200px box">
            <div style={{ width: 200, maxWidth: "6rem", height: 40, borderRadius: 6, backgroundColor: "var(--color-brand)" }} />
          </Sample>
          <Sample label='width: "auto"' hint="from the content, here 90 pixels">
            <div style={{ width: "auto", alignSelf: "flex-start", height: 40, borderRadius: 6, backgroundColor: "var(--color-brand)" }}>
              <div style={{ width: 90, height: 40 }} />
            </div>
          </Sample>
          <Sample label='width: "banana"' hint="drops to auto, and the colour still lands">
            <div style={{ width: "banana", height: 40, borderRadius: 6, backgroundColor: "var(--color-brand)" }} />
          </Sample>
        </Grid>
      </Panel>
    </div>
  )
}
