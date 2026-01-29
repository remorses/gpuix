import { useState, useEffect } from "react"
import { useGpuix } from "./use-gpuix"

export interface WindowSize {
  width: number
  height: number
}

/**
 * Get the current window size and subscribe to changes
 */
export function useWindowSize(): WindowSize {
  const { renderer } = useGpuix()
  const [size, setSize] = useState<WindowSize>({ width: 800, height: 600 })

  useEffect(() => {
    if (renderer) {
      try {
        const windowSize = renderer.getWindowSize()
        setSize({
          width: windowSize.width,
          height: windowSize.height,
        })
      } catch {
        // Renderer not ready
      }
    }
  }, [renderer])

  return size
}
