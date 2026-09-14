import { useEffect, useRef } from 'react'
import { render, useGpuixRequired } from '@gpuix/react'

export function DynamicImage() {
  const renderer = useGpuixRequired()
  const image = useRef<{ id: number }>(null)

  useEffect(() => {
    const id = image.current!.id
    const pixels = new Uint8Array(32 * 32 * 4)
    let frame = 0
    const paint = () => {
      for (let y = 0; y < 32; y++) {
        for (let x = 0; x < 32; x++) {
          const i = (y * 32 + x) * 4
          pixels[i] = (x * 8 + frame) % 256
          pixels[i + 1] = (y * 8 + frame) % 256
          pixels[i + 2] = 180
          pixels[i + 3] = 255
        }
      }
      renderer.updateImage!(id, 32, 32, pixels)
      frame += 4
    }
    paint()
    const timer = setInterval(paint, 40)
    // Unmount releases the image; no explicit clearImage is needed here.
    return () => clearInterval(timer)
  }, [renderer])

  return (
    <div style={{ padding: 24, backgroundColor: '#202030', height: '100%' }}>
      <img ref={image} testId="pixels" alt="Animated color field"
        style={{ width: 256, height: 256, borderRadius: 24 }} />
    </div>
  )
}

if (import.meta.main) {
  render(<DynamicImage />, {
    title: 'Dynamic image', width: 304, height: 304,
    focus: process.env.GPUIX_BACKGROUND !== '1',
  })
}
