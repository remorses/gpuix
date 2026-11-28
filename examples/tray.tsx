/**
 * GPUIX Tray Example
 *
 * Shows the process-wide tray item, the flat tray menu, and the
 * `quitMode: "explicit"` lifecycle: the process stays alive after the last
 * window closes, and quits only through `renderer.quit()`.
 *
 * Run: bun tray.tsx
 */

import React, { useEffect, useState } from 'react'
import { render, useGpuix } from '@gpuix/react'
import type { TrayMenuItem, TrayOptions } from '@gpuix/native/host'

function TrayApp() {
  const { renderer } = useGpuix()
  const [lastEvent, setLastEvent] = useState('none')

  useEffect(() => {
    const created = renderer.createTray?.({ title: '✓', tooltip: 'GPUIX tray demo' }) ?? false
    if (!created) return

    const menu: TrayMenuItem[] = [
      { id: 'ping', label: 'Ping', enabled: true },
      { id: 'toggle', label: 'Show last event', checked: true },
      { id: 'quit', label: 'Quit' },
    ]
    renderer.setTrayMenu?.(menu)
    renderer.onTrayEvent?.(event => {
      if (event.type === 'menuItem' && event.id === 'ping') {
        setLastEvent('ping')
      } else if (event.type === 'menuItem' && event.id === 'quit') {
        renderer.quit?.()
      } else if (event.type === 'menuItem' && event.id === 'toggle') {
        setLastEvent('toggle')
      } else {
        setLastEvent('click')
      }
    })

    return () => {
      renderer.removeTray?.()
    }
  }, [renderer])

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 12,
        width: 400,
        height: 300,
        backgroundColor: '#1e1e2e',
        borderRadius: 12,
      }}
    >
      <div style={{ color: '#cdd6f4', fontSize: 18, fontWeight: 'bold' }}>
        Tray demo
      </div>
      <div style={{ color: '#a6adc8', fontSize: 14 }}>
        Last tray event: {lastEvent}
      </div>
      <div
        style={{
          color: '#f38ba8',
          fontSize: 14,
          cursor: 'pointer',
          padding: '8px 16px',
        }}
        onClick={() => renderer.quit?.()}
      >
        Quit
      </div>
    </div>
  )
}

render(<TrayApp />, {
  title: 'Tray',
  width: 400,
  height: 300,
  quitMode: 'explicit',
})
