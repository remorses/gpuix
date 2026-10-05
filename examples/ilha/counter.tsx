/** @jsxImportSource @gpuix/ilha */
// Bun takes the JSX runtime from the tsconfig of the directory it is started
// in, not the one next to this file. The pragma makes the file run from
// anywhere; the tsconfig here still gives the editor the GPUIX element types.
/**
 * The GPUIX counter example, written in ilha.
 *
 * Same layout and interactions as `examples/counter.tsx` and
 * `examples/solid/counter.tsx`, but the state is ilha atoms. The card is one
 * component; the "recent" list below it is a keyed list, so a new row moves the
 * old ones instead of recreating them.
 *
 * Run on desktop: cd examples && bun --hot ilha/counter.tsx
 */

import { atom, batch } from 'ilha'
import { render } from '@gpuix/ilha'

const COLORS = {
  base: '#1e1e2e',
  crust: '#11111b',
  mantle: '#181825',
  surface: '#313244',
  text: '#cdd6f4',
  subtext: '#a6adc8',
  overlay: '#6c7086',
  green: '#a6e3a1',
  red: '#f38ba8',
  teal: '#94e2d5',
}

const STEP_BUTTON = {
  alignItems: 'center',
  borderRadius: 8,
  display: 'flex',
  justifyContent: 'center',
  padding: 12,
  paddingLeft: 24,
  paddingRight: 24,
  hover: { opacity: 0.85 },
} as const

const Counter = () => {
  const count = atom(0)
  const history = atom<Array<{ id: number; value: number }>>([])

  // The row id comes from the newest row, so the list stays the only state.
  const record = (next: number) => {
    batch(() => {
      count.set(next)
      history.update((rows) => {
        const id = (rows[0]?.id ?? 0) + 1
        return [{ id, value: next }, ...rows].slice(0, 5)
      })
    })
  }

  const reset = () => {
    batch(() => {
      count.set(0)
      history.set([])
    })
  }

  return (
    <div
      style={{
        alignItems: 'center',
        backgroundColor: COLORS.base,
        borderRadius: 12,
        display: 'flex',
        flexDirection: 'column',
        gap: 16,
        height: 460,
        justifyContent: 'center',
        padding: 32,
        width: 400,
      }}
    >
      <div
        testId="count"
        style={{ cursor: 'pointer', fontSize: 48, fontWeight: 'bold' }}
        onclick={() => record(count() + 1)}
      >
        <text style={{ color: COLORS.text }}>{count}</text>
      </div>

      <text style={{ color: COLORS.subtext, fontSize: 14 }}>
        Click the number or + to increment
      </text>

      <div style={{ display: 'flex', gap: 12 }}>
        <div
          testId="decrement"
          style={{
            ...STEP_BUTTON,
            backgroundColor: count() > 0 ? COLORS.red : COLORS.overlay,
            cursor: count() > 0 ? 'pointer' : 'default',
            opacity: count() > 0 ? 1 : 0.5,
          }}
          onclick={() => {
            if (count() > 0) {
              record(count() - 1)
            }
          }}
        >
          <text style={{ color: COLORS.base, fontWeight: 'bold' }}>-</text>
        </div>

        <div
          testId="increment"
          style={{ ...STEP_BUTTON, backgroundColor: COLORS.teal }}
          onclick={() => record(count() + 1)}
        >
          <text style={{ color: COLORS.base, fontWeight: 'bold' }}>+</text>
        </div>
      </div>

      <div
        testId="reset"
        style={{
          backgroundColor: COLORS.surface,
          borderRadius: 8,
          cursor: 'pointer',
          padding: 16,
          hover: { backgroundColor: '#45475a' },
        }}
        onclick={reset}
      >
        <text style={{ color: '#bac2de', fontSize: 14 }}>Reset</text>
      </div>

      {history().length > 0 && (
        <div
          testId="history"
          style={{
            display: 'flex',
            flexDirection: 'column',
            gap: 6,
            width: '100%',
          }}
        >
          {history().map((row) => (
            <div
              key={row.id}
              testId={`history-${row.id}`}
              style={{
                backgroundColor: COLORS.mantle,
                borderRadius: 6,
                display: 'flex',
                justifyContent: 'space-between',
                padding: 8,
              }}
            >
              <text style={{ color: COLORS.overlay, fontSize: 12 }}>
                {`#${row.id}`}
              </text>
              <text style={{ color: COLORS.text, fontSize: 12 }}>
                {`= ${row.value}`}
              </text>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

const App = () => (
  <div
    style={{
      alignItems: 'center',
      backgroundColor: COLORS.crust,
      display: 'flex',
      height: '100%',
      justifyContent: 'center',
      width: '100%',
    }}
  >
    <Counter />
  </div>
)

render(App, {
  title: 'GPUIX ilha Counter',
  width: 800,
  height: 600,
  // Agent checks need real GPU paint, not control of the user's keyboard.
  focus: process.env.GPUIX_BACKGROUND !== '1',
})
