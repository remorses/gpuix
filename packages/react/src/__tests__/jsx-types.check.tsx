// Type-only checks for the JSX runtime. `tsc --noEmit` runs them, and nothing
// imports this at runtime.
//
// A `@ts-expect-error` that stops being an error fails the build, so these pin
// the rejections as firmly as the acceptances.

// TypeScript reads `JSX.IntrinsicAttributes` for a component tag but not for a
// built-in one, so `key` has to sit in the props of each built-in tag.
const keyed = (
  <div>
    {[1, 2].map((row) => (
      <text key={row}>{String(row)}</text>
    ))}
  </div>
)

const keyedList = <div key="only-child" />

// @ts-expect-error a tag that is not ours is still not a tag
const notATag = <span />

// @ts-expect-error the props of a tag stay closed
const notAProp = <div nope={1} />

export type Checked = typeof keyed &
  typeof keyedList &
  typeof notATag &
  typeof notAProp
