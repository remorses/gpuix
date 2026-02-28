/**
 * GPUIX Counter Example
 *
 * This example shows how to use React with GPUI via GPUIX.
 * The element tree is serialized and sent to Rust/GPUI for rendering.
 *
 * Key change from the old API: instead of renderer.run() (which blocked forever),
 * we now use renderer.init() + setImmediate tick loop, so Node.js stays alive
 * and React state updates work.
 */

import React, { useState } from 'react'
import { createRoot, createRenderer, flushSync } from '@gpuix/react'

function Counter() {
  const [count, setCount] = useState(0)
  const [hovered, setHovered] = useState(false)

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 16,
        padding: 32,
        width: 400,
        height: 300,
        backgroundColor: '#1e1e2e',
        borderRadius: 12,
      }}
    >
      <div
        style={{
          fontSize: 48,
          fontWeight: 'bold',
          color: '#cdd6f4',
          cursor: 'pointer',
        }}
        onClick={() => setCount(c => c + 1)}
      >
        {count}
      </div>

      <div
        style={{
          color: '#a6adc8',
          fontSize: 14,
        }}
      >
        Click the number or + to increment
      </div>

      <div
        style={{
          display: 'flex',
          gap: 12,
        }}
      >
        <div
          style={{
            padding: 12,
            paddingLeft: 24,
            paddingRight: 24,
            backgroundColor: count > 0 ? '#f38ba8' : '#6c7086',
            borderRadius: 8,
            cursor: count > 0 ? 'pointer' : 'default',
            opacity: count > 0 ? 1 : 0.5,
          }}
          onClick={() => count > 0 && setCount(c => c - 1)}
        >
          <div style={{ color: '#1e1e2e', fontWeight: 'bold' }}>-</div>
        </div>

        <div
          style={{
            padding: 12,
            paddingLeft: 24,
            paddingRight: 24,
            backgroundColor: hovered ? '#94e2d5' : '#a6e3a1',
            borderRadius: 8,
            cursor: 'pointer',
          }}
          onClick={() => setCount(c => c + 1)}
          onMouseEnter={() => setHovered(true)}
          onMouseLeave={() => setHovered(false)}
        >
          <div style={{ color: '#1e1e2e', fontWeight: 'bold' }}>+</div>
        </div>
      </div>

      <div
        style={{
          marginTop: 16,
          padding: 16,
          backgroundColor: '#313244',
          borderRadius: 8,
          cursor: 'pointer',
        }}
        onClick={() => setCount(0)}
      >
        <div style={{ color: '#bac2de', fontSize: 14 }}>Reset</div>
      </div>
    </div>
  )
}

function App() {
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        width: '100%',
        height: '100%',
        backgroundColor: '#11111b',
      }}
    >
      <Counter />
    </div>
  )
}

// Initialize GPUIX with non-blocking platform
async function main() {
  // Create the native GPUI renderer with event callback
  const renderer = createRenderer((event) => {
    console.log('GPUI Event:', event.elementId, event.eventType)
  })

  // Initialize GPUI with NodePlatform (non-blocking — returns immediately)
  renderer.init({
    title: 'GPUIX Counter',
    width: 800,
    height: 600,
  })

  // Create React root
  const root = createRoot(renderer)

  // Render the app synchronously to ensure tree is ready
  flushSync(() => {
    root.render(<App />)
  })

  console.log('[GPUIX] Initial render complete, starting tick loop')

  // Drive the frame loop — Node.js event loop stays alive,
  // React state updates work, events flow back from GPUI
  function loop() {
    renderer.tick()
    setImmediate(loop)
  }
  loop()
}

main().catch(console.error)
