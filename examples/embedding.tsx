import React, { useEffect, useRef } from 'react'
import { render, useGpuixRequired, type PublicInstance } from '@gpuix/react'

function Diagnostic() {
  const renderer = useGpuixRequired()
  const target = useRef<PublicInstance>(null)
  useEffect(() => {
    // These are observations, never a safe point to attach/destroy a native child.
    const timer = setInterval(() => {
      const native = renderer.getNativeWindowHandle?.()
      console.log({
        kind: native?.kind,
        handleBytes: native?.handle.length,
        displayBytes: native?.display?.length,
        paint: target.current && renderer.getElementPaintState?.(target.current.id),
      })
    }, 1000)
    return () => clearInterval(timer)
  }, [renderer])
  return (
    <div ref={target} style={{ width: 240, height: 100, padding: 16, backgroundColor: '#243044' }}>
      <text style={{ color: '#ffffff' }}>Native integration diagnostic</text>
    </div>
  )
}

render(<Diagnostic />, { title: 'Embedding snapshots', width: 400, height: 240, focus: false })
