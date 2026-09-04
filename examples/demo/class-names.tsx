/// The `className` channel.
///
/// GPUIX ships no resolver. A root takes one through
/// `createRoot(renderer, { resolveClassName })`. This demo passes the small one
/// in `classes.ts`, which is shaped like the `@gpuix/tailwind` package this
/// repository plans to publish.
///
/// The resolver reads one token, never a whole string. That is what makes the
/// cache work. `clsx("p-4", a && "bg-brand", b && "text-lg")` writes up to
/// eight strings out of three tokens, and five toggles write thirty-two. A
/// bounded cache over whole strings sits in front of the token cache, because
/// the same string usually repeats between two frames.
///
/// CSS Style Attributes gives the `style` attribute "a specificity higher than
/// any selector", so a declaration in `style` beats one from a class in every
/// state.

import React, { useState } from "react"
import { Button, Grid, Panel, Row, Sample } from "./ui.js"
import { resolverCalls } from "./classes.js"

const CARD = "col gap-2 p-4 rounded bg-raised border w-full"

function Toggles() {
  const [padded, setPadded] = useState(true)
  const [loud, setLoud] = useState(false)
  const [big, setBig] = useState(false)
  const [round, setRound] = useState(true)
  const [asked, setAsked] = useState(resolverCalls())

  const className = [
    "row items-center justify-center h-[80px] border",
    padded ? "p-6" : "p-1",
    loud ? "bg-brand" : "bg-raised",
    big ? "text-2xl" : "text-sm",
    round ? "rounded-xl" : "rounded-none",
  ].join(" ")

  return (
    <Panel
      title="Conditional classes"
      note="Sixteen class strings come out of eight tokens. Flip every switch, then read the counter: once each token has been seen, the resolver is never asked again."
    >
      <Row>
        <Button label="padding" active={padded} onClick={() => setPadded((on) => !on)} />
        <Button label="loud" active={loud} onClick={() => setLoud((on) => !on)} />
        <Button label="big" active={big} onClick={() => setBig((on) => !on)} />
        <Button label="round" active={round} onClick={() => setRound((on) => !on)} />
        <Button label={`resolver calls: ${asked}`} onClick={() => setAsked(resolverCalls())} />
      </Row>
      <div className={className}>
        <text className="font-semibold text-fg">{className}</text>
      </div>
      <text className="text-xs text-faint">
        The counter reads at the moment you press it, so press it after flipping a switch.
      </text>
    </Panel>
  )
}

export function ClassNames() {
  return (
    <div className="col gap-4">
      <Panel
        title="A class and the style it stands for"
        note="Both boxes are the same declarations. One went through the resolver, the other did not."
      >
        <Grid>
          <Sample label='className="p-4 rounded-lg bg-brand"'>
            <div className="p-4 rounded-lg bg-brand w-full">
              <div style={{ height: 40, backgroundColor: "var(--color-panel)", borderRadius: 4 }} />
            </div>
          </Sample>
          <Sample label="the same, written in style">
            <div style={{ padding: 16, borderRadius: 12, backgroundColor: "var(--color-brand)" }} className="w-full">
              <div style={{ height: 40, backgroundColor: "var(--color-panel)", borderRadius: 4 }} />
            </div>
          </Sample>
        </Grid>
      </Panel>

      <Panel
        title="The style prop wins"
        note="An inline declaration beats a class, and it beats the class in a state as well. Point at all three."
      >
        <Grid>
          <Sample label='className="bg-brand"' hint="The class decides.">
            <div className="bg-brand h-[56px] rounded-lg w-full" />
          </Sample>
          <Sample label='className="bg-brand" style={{ backgroundColor: "#e11d48" }}' hint="The style prop decides.">
            <div className="bg-brand h-[56px] rounded-lg w-full" style={{ backgroundColor: "#e11d48" }} />
          </Sample>
          <Sample
            label='className="bg-raised hover:bg-brand" style={{ backgroundColor: "#e11d48" }}'
            hint="Still red on hover, because the class hover was dropped for that key."
          >
            <div className="bg-raised hover:bg-brand h-[56px] rounded-lg w-full" style={{ backgroundColor: "#e11d48" }} />
          </Sample>
          <Sample label='className="bg-raised hover:bg-brand"' hint="No inline colour, so hover works.">
            <div className="bg-raised hover:bg-brand h-[56px] rounded-lg w-full" />
          </Sample>
        </Grid>
      </Panel>

      <Panel
        title="Arbitrary values"
        note="A token in brackets reaches the parser as written, with underscores read as spaces. The resolver does no colour work of its own."
      >
        <Grid>
          <Sample label="bg-[oklch(0.72_0.16_150)]">
            <div className="bg-[oklch(0.72_0.16_150)] h-[56px] rounded-lg w-full" />
          </Sample>
          <Sample label="bg-[color-mix(in_oklch,var(--color-brand)_50%,white)]">
            <div className="bg-[color-mix(in_oklch,var(--color-brand)_50%,white)] h-[56px] rounded-lg w-full" />
          </Sample>
          <Sample label="pt-[calc(100px_+_2rem)]" hint="132px of top padding">
            <div className="bg-brand rounded pt-[calc(100px_+_2rem)] w-full" />
          </Sample>
          <Sample label="not-a-real-token" hint="The resolver returns null and nothing happens.">
            <div className="not-a-real-token h-[56px] rounded-lg w-full border" />
          </Sample>
        </Grid>
      </Panel>

      <Toggles />

      <Panel title="A card built only from classes" note="Every declaration below came through the resolver.">
        <div className={CARD}>
          <text className="text-lg font-bold text-fg">Class channel</text>
          <text className="text-sm text-muted leading-1.6">
            Spacing tokens fold a calc over --spacing, and colour tokens read the palette through
            var(). Change the palette and every class follows it.
          </text>
          <Row>
            <div className="px-3 py-2 rounded bg-brand pointer hover:opacity-80">
              <text className="text-sm font-semibold text-fg">Primary</text>
            </div>
            <div className="px-3 py-2 rounded border pointer hover:bg-raised">
              <text className="text-sm text-muted">Secondary</text>
            </div>
          </Row>
        </div>
      </Panel>
    </div>
  )
}
