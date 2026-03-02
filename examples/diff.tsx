/**
 * GPUIX Diff Viewer Example
 *
 * Ported from: https://github.com/remorses/critique/blob/87fba2b/src/diff.tsx
 *
 * Port of critique's diff.tsx from opentui to GPUIX.
 * Renders unified or split diff views with syntax highlighting (shiki)
 * and word-level diff highlights inside a scrollable container.
 *
 * Key differences from opentui version:
 *   opentui <box>        → GPUIX <div>
 *   opentui <span fg>    → GPUIX <text style={{ color }}>  (inside flex-row div)
 *   opentui <span bg>    → GPUIX <text style={{ backgroundColor }}>
 *   opentui RGBA.fromInts → hex string "#RRGGBBAA"
 *   opentui onMouse       → GPUIX onClick
 *
 * Run with:  cd examples && bun run diff
 * Test with: cd examples && bun run test
 */

import React, { useState } from "react"
import { createRoot, createRenderer, flushSync } from "@gpuix/react"
import { diffWords } from "diff"
import {
  createHighlighter,
  type BundledLanguage,
  type GrammarState,
  type ThemedToken,
} from "shiki"
import type { StructuredPatchHunk as Hunk } from "diff"

// ── Color constants ──────────────────────────────────────────────────
// All colors as hex strings (GPUIX parses #RRGGBB, #RRGGBBAA, and rgba())

const UNCHANGED_CODE_BG = "rgba(15, 15, 15, 1)"
const ADDED_BG = "rgba(100, 250, 120, 0.047)"
const REMOVED_BG = "rgba(255, 0, 0, 0.125)"

const LINE_NUMBER_BG = "rgba(5, 5, 5, 1)"
const REMOVED_LINE_NUMBER_BG = "rgba(60, 0, 0, 1)"
const ADDED_LINE_NUMBER_BG = "rgba(0, 50, 0, 1)"
const LINE_NUMBER_FG_BRIGHT = "#ffffff"
const LINE_NUMBER_FG_DIM = "#6c7086"

// Word diff highlight backgrounds
const WORD_REMOVED_BG = "rgba(255, 50, 50, 0.39)"
const WORD_ADDED_BG = "rgba(0, 200, 0, 0.39)"

// Default code foreground — matches github-dark-default theme.
// Used for text that doesn't have syntax highlighting (word diff, fallbacks).
const CODE_FG = "#e6edf3"

// Hunk separator
const SEPARATOR_FG = "#6c7086"

// ── Shiki highlighter ────────────────────────────────────────────────

const theme = "github-dark-default"
const highlighter = await createHighlighter({
  themes: [theme],
  langs: [
    "javascript",
    "typescript",
    "tsx",
    "jsx",
    "json",
    "markdown",
    "html",
    "css",
    "python",
    "rust",
    "go",
    "java",
    "c",
    "cpp",
    "yaml",
    "toml",
    "bash",
    "sh",
    "sql",
  ],
})

// ── Language detection ────────────────────────────────────────────────

function detectLanguage(filePath: string): BundledLanguage {
  const ext = filePath.split(".").pop()?.toLowerCase()
  switch (ext) {
    case "ts":
      return "typescript"
    case "tsx":
      return "tsx"
    case "jsx":
      return "jsx"
    case "js":
    case "mjs":
    case "cjs":
      return "javascript"
    case "json":
      return "json"
    case "md":
    case "mdx":
    case "markdown":
      return "markdown"
    case "html":
    case "htm":
      return "html"
    case "css":
      return "css"
    case "py":
      return "python"
    case "rs":
      return "rust"
    case "go":
      return "go"
    case "java":
      return "java"
    case "c":
    case "h":
      return "c"
    case "cpp":
    case "cc":
    case "cxx":
    case "hpp":
    case "hxx":
      return "cpp"
    case "yaml":
    case "yml":
      return "yaml"
    case "toml":
      return "toml"
    case "sh":
      return "sh"
    case "bash":
      return "bash"
    case "sql":
      return "sql"
    default:
      return "javascript"
  }
}

// ── Levenshtein / similarity ─────────────────────────────────────────

function levenshteinDistance(str1: string, str2: string): number {
  const len1 = str1.length
  const len2 = str2.length
  const matrix: number[][] = []

  for (let i = 0; i <= len1; i++) {
    matrix[i] = [i]
  }
  for (let j = 0; j <= len2; j++) {
    matrix[0]![j] = j
  }
  for (let i = 1; i <= len1; i++) {
    for (let j = 1; j <= len2; j++) {
      const cost = str1[i - 1] === str2[j - 1] ? 0 : 1
      matrix[i]![j] = Math.min(
        matrix[i - 1]![j]! + 1,
        matrix[i]![j - 1]! + 1,
        matrix[i - 1]![j - 1]! + cost,
      )
    }
  }
  return matrix[len1]![len2]!
}

function calculateSimilarity(str1: string, str2: string): number {
  const longer = str1.length > str2.length ? str1 : str2
  const shorter = str1.length > str2.length ? str2 : str1
  if (longer.length === 0) return 1.0
  const editDistance = levenshteinDistance(longer, shorter)
  return (longer.length - editDistance) / longer.length
}

// ── Token rendering ──────────────────────────────────────────────────
// In GPUIX, each token is a <text> with its own color inside a flex-row <div>.
// This replaces opentui's <text><span fg={color}>...</span></text> pattern.

function HighlightedTokens({ tokens }: { tokens: ThemedToken[] }) {
  return (
    <div style={{ display: "flex", flexDirection: "row" }}>
      {tokens.map((token, i) => (
        <text
          key={i}
          style={{
            color: token.color || "#e6edf3",
            whiteSpace: "nowrap",
          }}
        >
          {token.content}
        </text>
      ))}
    </div>
  )
}

// Word-diff tokens — shows which words changed within a line.
// Changed words get a background highlight (red for removed, green for added).

function WordDiffTokens({
  parts,
  mode,
}: {
  parts: ReturnType<typeof diffWords>
  mode: "remove" | "add"
}) {
  const highlightBg = mode === "remove" ? WORD_REMOVED_BG : WORD_ADDED_BG
  return (
    <div style={{ display: "flex", flexDirection: "row" }}>
      {parts.map((part, i) => {
        // Show removed parts only in remove mode, added parts only in add mode
        if (mode === "remove" && part.added) return null
        if (mode === "add" && part.removed) return null

        const isHighlighted =
          (mode === "remove" && part.removed) ||
          (mode === "add" && part.added)

        return (
          <text
            key={i}
            style={{
              color: CODE_FG,
              whiteSpace: "nowrap",
              ...(isHighlighted ? { backgroundColor: highlightBg } : {}),
            }}
          >
            {part.value}
          </text>
        )
      })}
    </div>
  )
}

// ── Line number gutter ───────────────────────────────────────────────

function LineNumberGutter({
  lineNumber,
  type,
  maxWidth,
}: {
  lineNumber: string
  type: string
  maxWidth: number
}) {
  const bg =
    type === "add"
      ? ADDED_LINE_NUMBER_BG
      : type === "remove"
        ? REMOVED_LINE_NUMBER_BG
        : LINE_NUMBER_BG

  const fg =
    type === "add" || type === "remove"
      ? LINE_NUMBER_FG_BRIGHT
      : LINE_NUMBER_FG_DIM

  return (
    <div
      style={{
        flexShrink: 0,
        alignSelf: "stretch",
        backgroundColor: bg,
      }}
    >
      <text
        style={{
          color: fg,
          whiteSpace: "nowrap",
          fontSize: 13,
        }}
      >
        {` ${lineNumber.padStart(maxWidth)} `}
      </text>
    </div>
  )
}

// ── Structured diff processing ───────────────────────────────────────
// Processes raw diff lines into renderable elements with syntax highlighting,
// line pairing, and word-level diff when lines are similar enough.

interface DiffLineData {
  code: React.ReactNode
  type: string
  oldLineNumber: string
  newLineNumber: string
  pairedWith?: number
  key: string
}

function processHunkLines(
  lines: string[],
  oldStart: number,
  filePath: string,
): DiffLineData[] {
  const processedLines = lines.map((code) => {
    if (code.startsWith("+")) {
      return { code: code.slice(1), type: "add" as const }
    }
    if (code.startsWith("-")) {
      return { code: code.slice(1), type: "remove" as const }
    }
    return { code: code.slice(1), type: "nochange" as const }
  })

  const lang = detectLanguage(filePath)

  // Tokenize "before" lines (removed + unchanged)
  let beforeState: GrammarState | undefined
  const beforeTokens: (ThemedToken[] | null)[] = []
  for (const line of processedLines) {
    if (line.type === "remove" || line.type === "nochange") {
      const result = highlighter.codeToTokens(line.code, {
        lang,
        theme,
        grammarState: beforeState,
      })
      beforeTokens.push(result.tokens[0] || null)
      beforeState = highlighter.getLastGrammarState(result.tokens)
    } else {
      beforeTokens.push(null)
    }
  }

  // Tokenize "after" lines (added + unchanged)
  let afterState: GrammarState | undefined
  const afterTokens: (ThemedToken[] | null)[] = []
  for (const line of processedLines) {
    if (line.type === "add" || line.type === "nochange") {
      const result = highlighter.codeToTokens(line.code, {
        lang,
        theme,
        grammarState: afterState,
      })
      afterTokens.push(result.tokens[0] || null)
      afterState = highlighter.getLastGrammarState(result.tokens)
    } else {
      afterTokens.push(null)
    }
  }

  // Pair consecutive removed/added lines for word-level diff
  const hasRemovals = processedLines.some((l) => l.type === "remove")
  const hasAdditions = processedLines.some((l) => l.type === "add")
  const shouldShowWordDiff = hasRemovals && hasAdditions

  const linePairs: Array<{ remove?: number; add?: number }> = []
  if (shouldShowWordDiff) {
    let i = 0
    while (i < processedLines.length) {
      if (processedLines[i]?.type === "remove") {
        const removes: number[] = []
        let j = i
        while (j < processedLines.length && processedLines[j]?.type === "remove") {
          removes.push(j)
          j++
        }
        const adds: number[] = []
        while (j < processedLines.length && processedLines[j]?.type === "add") {
          adds.push(j)
          j++
        }
        const minLen = Math.min(removes.length, adds.length)
        for (let k = 0; k < minLen; k++) {
          linePairs.push({ remove: removes[k], add: adds[k] })
        }
        i = j
      } else {
        i++
      }
    }
  }

  // Build result lines with syntax highlighting and word diff
  let oldLineNumber = oldStart
  let newLineNumber = oldStart
  const result: Array<{
    code: React.ReactNode
    type: string
    oldLineNumber: number
    newLineNumber: number
    pairedWith?: number
  }> = []

  for (let i = 0; i < processedLines.length; i++) {
    const line = processedLines[i]
    if (!line) continue

    const { code, type } = line
    const pair = linePairs.find((p) => p.remove === i || p.add === i)

    if (pair && pair.remove === i && pair.add !== undefined) {
      // Removed line paired with an addition — try word diff
      const removedText = processedLines[i]?.code
      const addedLine = processedLines[pair.add]
      if (!removedText || !addedLine) continue

      const similarity = calculateSimilarity(removedText, addedLine.code)
      if (similarity < 0.5) {
        // Too different — just use syntax highlighting
        const tokens = beforeTokens[i]
        result.push({
          code: tokens ? <HighlightedTokens tokens={tokens} /> : <text style={{ color: CODE_FG, whiteSpace: "nowrap" }}>{removedText}</text>,
          type,
          oldLineNumber,
          newLineNumber,
          pairedWith: pair.add,
        })
      } else {
        const parts = diffWords(removedText, addedLine.code)
        result.push({
          code: <WordDiffTokens parts={parts} mode="remove" />,
          type,
          oldLineNumber,
          newLineNumber,
          pairedWith: pair.add,
        })
      }
      oldLineNumber++
    } else if (pair && pair.add === i && pair.remove !== undefined) {
      // Added line paired with a removal — try word diff
      const removedLine = processedLines[pair.remove]
      const addedLine = processedLines[i]
      if (!removedLine || !addedLine) continue

      const similarity = calculateSimilarity(removedLine.code, addedLine.code)
      if (similarity < 0.5) {
        const tokens = afterTokens[i]
        result.push({
          code: tokens ? <HighlightedTokens tokens={tokens} /> : <text style={{ color: CODE_FG, whiteSpace: "nowrap" }}>{addedLine.code}</text>,
          type,
          oldLineNumber,
          newLineNumber,
          pairedWith: pair.remove,
        })
      } else {
        const parts = diffWords(removedLine.code, addedLine.code)
        result.push({
          code: <WordDiffTokens parts={parts} mode="add" />,
          type,
          oldLineNumber,
          newLineNumber,
          pairedWith: pair.remove,
        })
      }
      newLineNumber++
    } else {
      // Unpaired line — just syntax highlight
      const tokens =
        type === "remove"
          ? beforeTokens[i]
          : type === "add"
            ? afterTokens[i]
            : beforeTokens[i] || afterTokens[i]

      result.push({
        code:
          tokens && tokens.length > 0 ? (
            <HighlightedTokens tokens={tokens} />
          ) : (
            <text style={{ color: CODE_FG, whiteSpace: "nowrap" }}>{code}</text>
          ),
        type,
        oldLineNumber,
        newLineNumber,
      })

      if (type === "remove") {
        oldLineNumber++
      } else if (type === "add") {
        newLineNumber++
      } else {
        oldLineNumber++
        newLineNumber++
      }
    }
  }

  return result.map(({ type, code, oldLineNumber, newLineNumber, pairedWith }, index) => ({
    oldLineNumber: oldLineNumber.toString(),
    newLineNumber: newLineNumber.toString(),
    code,
    type,
    pairedWith,
    key: `line-${index}`,
  }))
}

// ── Unified view ─────────────────────────────────────────────────────

function UnifiedView({
  diff,
  maxWidth,
}: {
  diff: DiffLineData[]
  maxWidth: number
}) {
  return (
    <>
      {diff.map(({ code, type, key, newLineNumber }) => {
        const lineNumber =
          newLineNumber && newLineNumber !== "0"
            ? newLineNumber.padStart(maxWidth)
            : " ".repeat(maxWidth)

        const codeBg =
          type === "add"
            ? ADDED_BG
            : type === "remove"
              ? REMOVED_BG
              : UNCHANGED_CODE_BG

        return (
          <div key={key} style={{ display: "flex", flexDirection: "row" }}>
            <LineNumberGutter
              lineNumber={lineNumber}
              type={type}
              maxWidth={maxWidth}
            />
            <div
              style={{
                flexGrow: 1,
                paddingLeft: 4,
                backgroundColor: codeBg,
              }}
            >
              {code}
            </div>
          </div>
        )
      })}
    </>
  )
}

// ── Split view ───────────────────────────────────────────────────────

interface SplitLine {
  left: DiffLineData & { lineNumber: string }
  right: DiffLineData & { lineNumber: string }
}

function buildSplitLines(
  diff: DiffLineData[],
  leftMaxWidth: number,
  rightMaxWidth: number,
): SplitLine[] {
  const splitLines: SplitLine[] = []
  const processedIndices = new Set<number>()

  for (let i = 0; i < diff.length; i++) {
    if (processedIndices.has(i)) continue
    const line = diff[i]
    if (!line) continue

    if (line.type === "remove" && line.pairedWith !== undefined) {
      const pairedLine = diff[line.pairedWith]
      if (pairedLine) {
        splitLines.push({
          left: { ...line, lineNumber: line.oldLineNumber.padStart(leftMaxWidth) },
          right: { ...pairedLine, lineNumber: pairedLine.newLineNumber.padStart(rightMaxWidth) },
        })
        processedIndices.add(i)
        processedIndices.add(line.pairedWith)
      }
    } else if (line.type === "add" && line.pairedWith !== undefined) {
      continue
    } else if (line.type === "remove") {
      splitLines.push({
        left: { ...line, lineNumber: line.oldLineNumber.padStart(leftMaxWidth) },
        right: {
          lineNumber: " ".repeat(rightMaxWidth),
          code: <text>{""}</text>,
          type: "empty",
          oldLineNumber: "",
          newLineNumber: "",
          key: `${line.key}-empty-right`,
        },
      })
      processedIndices.add(i)
    } else if (line.type === "add") {
      splitLines.push({
        left: {
          lineNumber: " ".repeat(leftMaxWidth),
          code: <text>{""}</text>,
          type: "empty",
          oldLineNumber: "",
          newLineNumber: "",
          key: `${line.key}-empty-left`,
        },
        right: { ...line, lineNumber: line.newLineNumber.padStart(rightMaxWidth) },
      })
      processedIndices.add(i)
    } else {
      splitLines.push({
        left: { ...line, lineNumber: line.oldLineNumber.padStart(leftMaxWidth) },
        right: { ...line, lineNumber: line.newLineNumber.padStart(rightMaxWidth) },
      })
      processedIndices.add(i)
    }
  }

  return splitLines
}

function SplitSideGutter({
  lineNumber,
  type,
  maxWidth,
}: {
  lineNumber: string
  type: string
  maxWidth: number
}) {
  const bg =
    type === "remove"
      ? REMOVED_LINE_NUMBER_BG
      : type === "add"
        ? ADDED_LINE_NUMBER_BG
        : LINE_NUMBER_BG
  const fg =
    type === "remove" || type === "add"
      ? LINE_NUMBER_FG_BRIGHT
      : LINE_NUMBER_FG_DIM

  return (
    <div
      style={{
        flexShrink: 0,
        alignSelf: "stretch",
        backgroundColor: bg,
      }}
    >
      <text style={{ color: fg, whiteSpace: "nowrap", fontSize: 13 }}>
        {` ${lineNumber} `}
      </text>
    </div>
  )
}

function SplitSideCode({
  code,
  type,
}: {
  code: React.ReactNode
  type: string
}) {
  const bg =
    type === "remove"
      ? REMOVED_BG
      : type === "add"
        ? ADDED_BG
        : UNCHANGED_CODE_BG

  return (
    <div
      style={{
        flexGrow: 1,
        paddingLeft: 4,
        minWidth: 0,
        backgroundColor: bg,
      }}
    >
      {code}
    </div>
  )
}

function SplitView({
  diff,
  leftMaxWidth,
  rightMaxWidth,
}: {
  diff: DiffLineData[]
  leftMaxWidth: number
  rightMaxWidth: number
}) {
  const splitLines = buildSplitLines(diff, leftMaxWidth, rightMaxWidth)

  return (
    <>
      {splitLines.map(({ left, right }) => (
        <div key={left.key} style={{ display: "flex", flexDirection: "row" }}>
          {/* Left side (removals / old) */}
          <div style={{ display: "flex", flexDirection: "row", width: "50%" }}>
            <SplitSideGutter
              lineNumber={left.lineNumber}
              type={left.type}
              maxWidth={leftMaxWidth}
            />
            <SplitSideCode code={left.code} type={left.type} />
          </div>

          {/* Right side (additions / new) */}
          <div style={{ display: "flex", flexDirection: "row", width: "50%" }}>
            <SplitSideGutter
              lineNumber={right.lineNumber}
              type={right.type}
              maxWidth={rightMaxWidth}
            />
            <SplitSideCode code={right.code} type={right.type} />
          </div>
        </div>
      ))}
    </>
  )
}

// ── DiffViewer (main export) ─────────────────────────────────────────

export interface DiffViewerProps {
  hunks: Hunk[]
  filePath?: string
  splitView?: boolean
}

export function DiffViewer({
  hunks,
  filePath = "",
  splitView = false,
}: DiffViewerProps) {
  if (hunks.length === 0) {
    return (
      <div style={{ padding: 16 }}>
        <text style={{ color: LINE_NUMBER_FG_DIM }}>No changes</text>
      </div>
    )
  }

  // Calculate max line number widths across all hunks
  const allLines = hunks.flatMap((h) => h.lines)
  let oldLineNum = hunks[0]?.oldStart || 1
  let newLineNum = hunks[0]?.newStart || 1

  // Count to find max old line
  let tempOld = oldLineNum
  let tempNew = newLineNum
  for (const line of allLines) {
    if (line.startsWith("-")) tempOld++
    else if (line.startsWith("+")) tempNew++
    else {
      tempOld++
      tempNew++
    }
  }
  const leftMaxWidth = Math.max(tempOld.toString().length, 2)
  const rightMaxWidth = Math.max(tempNew.toString().length, 2)
  const maxWidth = Math.max(leftMaxWidth, rightMaxWidth)

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        fontFamily: "Menlo",
        fontSize: 13,
      }}
    >
      {hunks.map((hunk, hunkIdx) => {
        const diff = processHunkLines(hunk.lines, hunk.oldStart, filePath)

        return (
          <React.Fragment key={hunk.newStart}>
            {splitView ? (
              <SplitView
                diff={diff}
                leftMaxWidth={leftMaxWidth}
                rightMaxWidth={rightMaxWidth}
              />
            ) : (
              <UnifiedView diff={diff} maxWidth={maxWidth} />
            )}
            {hunkIdx < hunks.length - 1 && (
              <div style={{ paddingLeft: 4 }}>
                <text
                  style={{
                    color: SEPARATOR_FG,
                    whiteSpace: "nowrap",
                    fontSize: 13,
                  }}
                >
                  {`${" ".repeat(maxWidth + 2)}...`}
                </text>
              </div>
            )}
          </React.Fragment>
        )
      })}
    </div>
  )
}

// ── Example app ──────────────────────────────────────────────────────

// Hardcoded example patch — a realistic TypeScript file change
const exampleHunks: Hunk[] = [
  {
    oldStart: 1,
    oldLines: 8,
    newStart: 1,
    newLines: 10,
    lines: [
      " import React from 'react'",
      " import { useState } from 'react'",
      " ",
      "-function Counter({ initial }: { initial: number }) {",
      "-  const [count, setCount] = useState(initial)",
      "+interface CounterProps {",
      "+  initial: number",
      "+  step?: number",
      "+}",
      "+",
      "+function Counter({ initial, step = 1 }: CounterProps) {",
      "+  const [count, setCount] = useState(initial)",
      " ",
      "   return (",
      "     <div>",
    ],
  },
  {
    oldStart: 12,
    oldLines: 5,
    newStart: 14,
    newLines: 7,
    lines: [
      "       <span>{count}</span>",
      "-      <button onClick={() => setCount(c => c + 1)}>+</button>",
      "-      <button onClick={() => setCount(c => c - 1)}>-</button>",
      "+      <button onClick={() => setCount(c => c + step)}>",
      "+        Increment by {step}",
      "+      </button>",
      "+      <button onClick={() => setCount(c => c - step)}>",
      "+        Decrement by {step}",
      "+      </button>",
      "     </div>",
      "   )",
    ],
  },
]

function App() {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        width: "100%",
        height: "100%",
        backgroundColor: "#11111b",
      }}
    >
      {/* Title bar */}
      <div
        style={{
          padding: 12,
          paddingLeft: 16,
          backgroundColor: "#1e1e2e",
        }}
      >
        <text style={{ color: "#cdd6f4", fontSize: 14, fontWeight: "bold" }}>
          counter.tsx
        </text>
      </div>

      {/* Scrollable diff — bg matches unchanged code so empty area below
           content doesn't look different (minHeight: 100% doesn't work
           inside scroll containers in GPUI) */}
      <div
        style={{
          flexGrow: 1,
          overflow: "scroll",
          backgroundColor: UNCHANGED_CODE_BG,
        }}
      >
        <DiffViewer
          hunks={exampleHunks}
          filePath="counter.tsx"
          splitView={false}
        />
      </div>
    </div>
  )
}

// ── Main ─────────────────────────────────────────────────────────────
// Only runs when executed directly (not when imported by tests).

async function main() {
  const renderer = createRenderer((event) => {
    // Event logging for debug
  })

  renderer.init({
    title: "GPUIX Diff Viewer",
    width: 900,
    height: 600,
  })

  const root = createRoot(renderer)

  flushSync(() => {
    root.render(<App />)
  })

  console.log("[GPUIX] Diff viewer running")

  function loop() {
    renderer.tick()
    setImmediate(loop)
  }
  loop()
}

// Guard: only run main() when this file is the entry point.
// Bun sets Bun.main, Node has require.main or process.argv[1].
const isEntryPoint =
  typeof Bun !== "undefined"
    ? Bun.main === import.meta.path
    : process.argv[1]?.endsWith("diff.tsx") || process.argv[1]?.endsWith("diff.js")

if (isEntryPoint) {
  main().catch(console.error)
}
