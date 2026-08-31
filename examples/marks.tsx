import { createRoot, CanvasPathCommand } from "@gpuix/react"
import { OrganicBody, VectorEyes, DecorativeStroke, VectorPath } from "@gpuix/react/src/components/marks.js"
import { GpuixRenderer } from "@gpuix/native"

const root = createRoot(new GpuixRenderer())

const bodyPath: CanvasPathCommand[] = [
  { type: "move", x: 100, y: 50 },
  { type: "curve", x1: 150, y1: 50, x2: 150, y2: 150, x: 100, y: 150 },
  { type: "curve", x1: 50, y1: 150, x2: 50, y2: 50, x: 100, y: 50 },
  { type: "close" }
]

const eyePath: CanvasPathCommand[] = [
  { type: "move", x: 80, y: 90 },
  { type: "line", x: 90, y: 90 },
  { type: "move", x: 110, y: 90 },
  { type: "line", x: 120, y: 90 }
]

const accentPath: CanvasPathCommand[] = [
  { type: "move", x: 60, y: 60 },
  { type: "curve", x1: 70, y1: 40, x2: 130, y2: 40, x: 140, y: 60 }
]

root.render(
  <div style={{ flexGrow: 1, padding: 40, backgroundColor: "#1e1e1e", gap: 20 }}>
    <text style={{ color: "white", fontSize: 24, marginBottom: 20 }}>Living Vector Marks</text>
    
    <div style={{ width: 200, height: 200, position: "relative" }}>
      <OrganicBody 
        path={bodyPath} 
        fill="#ff6b6b" 
        squash={0.1} 
        breatheLoop={2.0} 
      />
      <div style={{ position: "absolute", top: 0, left: 0, width: "100%", height: "100%" }}>
        <VectorEyes 
          path={eyePath} 
          stroke="#ffffff" 
          strokeWidth={4} 
          glanceX={2} 
          blink={1.0} 
          blinkLoop={0.5} 
        />
      </div>
      <div style={{ position: "absolute", top: 0, left: 0, width: "100%", height: "100%" }}>
        <DecorativeStroke 
          path={accentPath} 
          stroke="#ffe66d" 
          strokeWidth={3} 
          wiggle={2.0} 
          wiggleLoop={3.0} 
        />
      </div>
    </div>
  </div>
)
