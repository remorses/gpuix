/// The index and child conditions.
///
/// `first:`, `last:`, `odd:`, `even:` and `only:` read the position of the
/// element among its siblings. The walk knows that position at build time, so
/// no measurement and no event is involved. `*:` and `**:` sit on the parent
/// and style its children, the way `space-y-2` and `divide-y` do in Tailwind.
///
/// Every rule here compiles as `:where()` does on the web, with specificity
/// zero. A declaration the child makes itself always wins over a rule from
/// the parent.

import React, { useState } from "react"
import { Button, Grid, Panel, Row, Sample } from "./ui.js"

const ROW = "px-3 py-2"

function Names({ children }: { children: string[] }) {
  return (
    <>
      {children.map((name) => (
        <div key={name} className={ROW}>
          <text className="text-sm text-fg">{name}</text>
        </div>
      ))}
    </>
  )
}

const FRUIT = ["apple", "pear", "plum", "fig"]

function GrowingList() {
  const [count, setCount] = useState(3)
  return (
    <Panel
      title="The position follows the list"
      note="Every row carries the same class string. Add a row and the old last row loses its colour, because :last-child now points at the new one."
    >
      <Row>
        <Button label="add" onClick={() => setCount((n) => n + 1)} />
        <Button label="remove" onClick={() => setCount((n) => Math.max(1, n - 1))} />
      </Row>
      <div className="col rounded border" style={{ width: 260 }}>
        {Array.from({ length: count }, (_, at) => (
          <div key={at} className={`${ROW} first:bg-brand-soft last:bg-brand only:bg-raised`}>
            <text className="text-sm text-fg">{`row ${at + 1}`}</text>
          </div>
        ))}
      </div>
      <text className="text-xs text-faint">
        With one row left, only: takes over and the row is neither brand nor soft.
      </text>
    </Panel>
  )
}

export function Selectors() {
  return (
    <div className="col gap-4">
      <Panel
        title="Index conditions"
        note="The walk hands each element its position among its siblings, so these apply at build time. odd: counts from one, as :nth-child does."
      >
        <Grid>
          <Sample label='"first:bg-brand-soft last:bg-brand-soft"' hint="Both ends coloured, the middle untouched.">
            <div className="col rounded border w-full">
              {FRUIT.map((name) => (
                <div key={name} className={`${ROW} first:bg-brand-soft last:bg-brand-soft`}>
                  <text className="text-sm text-fg">{name}</text>
                </div>
              ))}
            </div>
          </Sample>
          <Sample label='"odd:bg-raised"' hint="Stripes without any index logic in the app.">
            <div className="col rounded border w-full">
              {FRUIT.map((name) => (
                <div key={name} className={`${ROW} odd:bg-raised`}>
                  <text className="text-sm text-fg">{name}</text>
                </div>
              ))}
            </div>
          </Sample>
        </Grid>
      </Panel>

      <Panel
        title="Rules on the parent"
        note="space-y-2 and divide-y resolve to one rule on & > :not(:last-child). The children carry no class of their own."
      >
        <Grid>
          <Sample label='"col space-y-2"' hint="A margin under every child except the last.">
            <div className="col space-y-2 w-full">
              {FRUIT.map((name) => (
                <div key={name} className={`${ROW} bg-raised rounded`}>
                  <text className="text-sm text-fg">{name}</text>
                </div>
              ))}
            </div>
          </Sample>
          <Sample label='"col divide-y border rounded"' hint="A line between rows, and none under the last.">
            <div className="col divide-y border rounded w-full">
              <Names>{FRUIT}</Names>
            </div>
          </Sample>
        </Grid>
      </Panel>

      <Panel
        title="*: styles the children, **: the whole subtree"
        note="The rules have specificity zero, so a declaration the child makes itself wins. The third box below keeps its own colour."
      >
        <div className="row gap-2 *:p-3 *:rounded *:bg-raised">
          <div>
            <text className="text-sm text-fg">plain</text>
          </div>
          <div>
            <text className="text-sm text-fg">plain</text>
          </div>
          <div className="bg-brand">
            <text className="text-sm text-fg">bg-brand</text>
          </div>
        </div>
        <div className="col gap-2 **:text-faint p-3 rounded border">
          <text className="text-sm">this text is two levels down</text>
          <div className="row gap-2">
            <text className="text-sm">and this one is three</text>
            <text className="text-sm text-brand">its own class wins</text>
          </div>
        </div>
      </Panel>

      <GrowingList />
    </div>
  )
}
