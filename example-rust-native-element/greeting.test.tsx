import { connectTest } from '@gpuix/react/automation'
import { createTestRoot } from '@gpuix/react/testing'
import type { EventPayload, Props } from '@gpuix/react'
import { expect, test } from 'vitest'

declare module '@gpuix/react/jsx-runtime' {
  namespace JSX {
    interface IntrinsicElements {
      'rust-greeting': Props & {
        name?: string
        onGreet?: (event: EventPayload) => void
      }
    }
  }
}

// The addon is loaded through NAPI_RS_NATIVE_LIBRARY_PATH (vitest.config.ts),
// so this file only passes when the Rust crate registered <rust-greeting>.
test('a crate adds <rust-greeting>: it paints, takes props, and emits events', async () => {
  const greeted: unknown[] = []
  const { render, renderer, unmount } = createTestRoot()
  const app = (name: string) => (
    <div style={{ width: 300, height: 100, flexDirection: 'column' }}>
      <text>built-in</text>
      <rust-greeting
        testId="greeting"
        name={name}
        style={{ width: 200, height: 40 }}
        onGreet={(event) => greeted.push(event.detail)}
      />
    </div>
  )

  try {
    render(app('world'))
    renderer.flush()
    expect(renderer.getPaintedText()).toEqual(['built-in', 'Hello, world'])

    const automation = await connectTest(renderer)
    await automation.getByTestId('greeting').click()
    expect(greeted).toEqual([{ name: 'world' }])

    render(app('gpuix'))
    renderer.flush()
    expect(renderer.getPaintedText()).toEqual(['built-in', 'Hello, gpuix'])
  } finally {
    unmount()
  }
})
