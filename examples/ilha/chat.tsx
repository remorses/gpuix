/** @jsxImportSource @gpuix/ilha */
// Bun takes the JSX runtime from the tsconfig of the directory it is started
// in, not the one next to this file. The pragma makes the file run from
// anywhere; the tsconfig here still gives the editor the GPUIX element types.
/**
 * The GPUIX chat example, written in ilha.
 *
 * Same layout and interactions as the React `examples/chat.tsx` and the Solid
 * `examples/solid/chat.tsx`: a transparent titlebar, traffic lights in the
 * sidebar, graphite surfaces, composer chips, and a workspace footer. Markdown
 * turns use the native `<markdown>` element instead of React's safe-mdx tree,
 * because safe-mdx is a React library.
 *
 * State lives in atoms created once in `ChatApp`. A component repaints when an
 * atom it read changes, so each atom is read in the smallest component that
 * needs it: a keystroke in the composer repaints the send button, not the
 * 1,000-row transcript.
 *
 * Run on desktop: cd examples && bun --hot ilha/chat.tsx
 */

import { atom } from "ilha"
import type { AtomHandle, View } from "ilha"
import {
  Button,
  Dialog,
  DialogBackdrop,
  DialogClose,
  DialogPopup,
  DialogPortal,
  DialogTitle,
  motion,
  render,
  Select,
  SelectContent,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  useGpuixRequired,
  windowInsets,
  type DialogPopupProps,
  type HostElement,
  type StyleDesc,
} from "@gpuix/ilha"
import iconCompose from "../assets/icons/compose.svg" with { type: "text" }
import iconSearch from "../assets/icons/search.svg" with { type: "text" }
import iconSidebar from "../assets/icons/panel-left.svg" with { type: "text" }
import iconPanelRight from "../assets/icons/panel-right.svg" with { type: "text" }
import iconArrowLeft from "../assets/icons/arrow-left.svg" with { type: "text" }
import iconArrowRight from "../assets/icons/arrow-right.svg" with { type: "text" }
import iconFolder from "../assets/icons/folder.svg" with { type: "text" }
import iconSettings from "../assets/icons/settings.svg" with { type: "text" }
import iconGitBranch from "../assets/icons/git-branch.svg" with { type: "text" }
import iconLaptop from "../assets/icons/laptop.svg" with { type: "text" }
import iconLockOpen from "../assets/icons/lock-open.svg" with { type: "text" }
import iconLock from "../assets/icons/lock.svg" with { type: "text" }
import iconList from "../assets/icons/list.svg" with { type: "text" }
import iconZap from "../assets/icons/zap.svg" with { type: "text" }
import iconPencil from "../assets/icons/pencil.svg" with { type: "text" }
import iconChevronDown from "../assets/icons/chevron-down.svg" with { type: "text" }
import iconChevronRight from "../assets/icons/chevron-right.svg" with { type: "text" }
import iconListFilter from "../assets/icons/list-filter.svg" with { type: "text" }
import iconSparkle from "../assets/icons/sparkle.svg" with { type: "text" }
import iconWrench from "../assets/icons/wrench.svg" with { type: "text" }
import iconSend from "../assets/icons/arrow-up.svg" with { type: "text" }
import iconCopy from "../assets/icons/copy.svg" with { type: "text" }
import iconCheck from "../assets/icons/check.svg" with { type: "text" }
import iconRetry from "../assets/icons/rotate-ccw.svg" with { type: "text" }
import iconThumbsUp from "../assets/icons/thumbs-up.svg" with { type: "text" }
import iconThumbsDown from "../assets/icons/thumbs-down.svg" with { type: "text" }
import iconShare from "../assets/icons/share.svg" with { type: "text" }
import iconMore from "../assets/icons/ellipsis.svg" with { type: "text" }

const C = {
  canvas: "#1A1A1A",
  sidebar: "#181818",
  raised: "#232323",
  composer: "#212121",
  overlay: "#E6EAF20D",
  overlayStrong: "#E6EAF217",
  item: "#F0F0F00F",
  border: "#E6EAF212",
  borderStrong: "#E6EAF224",
  sidebarBorder: "#292929",
  text: "#E2E2E2",
  secondary: "#A3A3A3",
  tertiary: "#7D7D7D",
  ghost: "#575757",
  accent: "#E2795B",
  inverse: "#E7E9EC",
  onInverse: "#17181C",
  codeText: "#E0A882",
}

const SIDEBAR_WIDTH = 252
const TRAFFIC_LIGHT_CLEARANCE =
  typeof process !== "undefined" && process.platform === "darwin" ? 86 : 8
const CONTENT_MAX_WIDTH = 720
const TITLEBAR_HEIGHT = 48

const FONT_SANS = typeof window === "undefined" ? "Helvetica" : "IBM Plex Sans"

const ICONS = {
  compose: iconCompose,
  search: iconSearch,
  sidebar: iconSidebar,
  panelRight: iconPanelRight,
  arrowLeft: iconArrowLeft,
  arrowRight: iconArrowRight,
  folder: iconFolder,
  settings: iconSettings,
  gitBranch: iconGitBranch,
  laptop: iconLaptop,
  lockOpen: iconLockOpen,
  lock: iconLock,
  list: iconList,
  zap: iconZap,
  pencil: iconPencil,
  chevronDown: iconChevronDown,
  chevronRight: iconChevronRight,
  listFilter: iconListFilter,
  sparkle: iconSparkle,
  wrench: iconWrench,
  send: iconSend,
  copy: iconCopy,
  check: iconCheck,
  retry: iconRetry,
  thumbsUp: iconThumbsUp,
  thumbsDown: iconThumbsDown,
  share: iconShare,
  more: iconMore,
} as const

type IconName = keyof typeof ICONS

function Icon(props: { name: IconName; size?: number; color: string }) {
  return (
    <svg
      source={ICONS[props.name]}
      style={{
        width: props.size ?? 14,
        height: props.size ?? 14,
        flexShrink: 0,
        color: props.color,
        pointerEvents: "none",
      }}
    />
  )
}

const CHAT_THEME = {
  text: C.text,
  textMuted: C.secondary,
  textFaint: C.tertiary,
  textDim: C.secondary,
  border: C.border,
  bg: C.canvas,
  accent: C.accent,
  caret: C.accent,
  fontSans: FONT_SANS,
  codeText: C.codeText,
  codeWash: "#E6EAF214",
  metrics: {
    mdTextSize: 14,
    mdLineHeight: 22,
    mdBlockGap: 14,
    mdHeadingSizes: [20, 16, 14, 14],
    mdHeadingLineHeights: [28, 24, 22, 22],
    codeTextSize: 12.5,
    codeLineHeight: 20,
    diffLineHeight: 20,
    diffFileHeaderHeight: 34,
  },
}

type Turn =
  | { kind: "user"; text: string }
  | { kind: "fold"; duration: string }
  | { kind: "markdown"; source: string }
  | { kind: "code"; language: string; source: string }
  | { kind: "diff"; patch: string }

interface Conversation {
  id: string
  title: string
  group: string
  project: string
  time: string
  turns: Turn[]
}

const MODELS = [
  { id: "deepseek-v4-flash", label: "DeepSeek V4 Flash", group: "DeepSeek", icon: "sparkle" as const },
  { id: "deepseek-v4", label: "DeepSeek V4", group: "DeepSeek", icon: "sparkle" as const },
  { id: "opus-4.6", label: "Claude Opus 4.6", group: "Claude", icon: "sparkle" as const },
  { id: "sonnet-4.6", label: "Claude Sonnet 4.6", group: "Claude", icon: "sparkle" as const },
  { id: "gpt-5.4", label: "GPT-5.4", group: "OpenAI", icon: "sparkle" as const },
  { id: "grok-4", label: "Grok 4", group: "xAI", icon: "sparkle" as const },
]

const REASONING = [
  { id: "high", label: "High", hint: "Default" as string | undefined },
  { id: "medium", label: "Medium", hint: undefined },
  { id: "low", label: "Low", hint: undefined },
]

const ACCESS = [
  { id: "ask", label: "Supervised", description: "Ask before every tool call", icon: "lock" as const },
  { id: "edits", label: "Auto-accept edits", description: "Edit files without asking", icon: "pencil" as const },
  { id: "auto", label: "Auto", description: "Run most tools without asking", icon: "sparkle" as const },
  { id: "full", label: "Full access", description: "No permission prompts", icon: "lockOpen" as const },
]

const PROJECTS = [
  { id: "gpuix", label: "gpuix" },
  { id: "example-app", label: "example-app" },
  { id: "none", label: "No project" },
]

const WORKSPACES = [
  { id: "local", label: "Local", icon: "laptop" as const },
  { id: "worktree", label: "New worktree", icon: "gitBranch" as const },
]

const BRANCHES = [
  { id: "main", label: "main" },
  { id: "feat-selectors", label: "feat/selectors" },
  { id: "chat-example", label: "chat-example" },
]

const OVERVIEW = `**GPUIX** is an ilha renderer for GPUI, Zed's GPU-accelerated UI framework. It renders native desktop interfaces through Metal, DirectX, or Vulkan. No Electron or web view.`
const ARCHITECTURE = `ilha paints through host mutations sent over napi-rs. Rust keeps the retained tree and translates it into GPUI elements for each frame.`
const SELECTION = `Selection is rebuilt from the paint pass. Each string registers in document order, so a drag can cross elements.`
const SELECTION_CODE = `pub fn resolve_spans(
    elements: &[(&str, &str)],
    a: (usize, usize),
    b: (usize, usize),
) -> Vec<Span> {
    let (start, end) = if a <= b { (a, b) } else { (b, a) };
    let mut spans = Vec::new();
    for (ei, (key, text)) in elements.iter().enumerate().take(end.0 + 1).skip(start.0) {
        let from = if ei == start.0 { start.1 } else { 0 };
        let to = if ei == end.0 { end.1 } else { text.len() };
        if from < to {
            spans.push(Span { key: key.to_string(), range: from..to });
        }
    }
    spans
}`
const GUTTER = `The gutter width now follows the largest line number, so a five-digit line no longer hits the accent bar.`
const GUTTER_DIFF = [
  "diff --git a/packages/native/src/diff/mod.rs b/packages/native/src/diff/mod.rs",
  "index 8f2a1c4..d91b7e0 100644",
  "--- a/packages/native/src/diff/mod.rs",
  "+++ b/packages/native/src/diff/mod.rs",
  "@@ -78,12 +78,15 @@ impl FileDiff {",
  " /// Width of one line-number gutter, fitted to the largest line number.",
  "-pub fn gutter_width(file: &FileDiff) -> f32 {",
  "-    GUTTER_WIDTH",
  "+pub fn gutter_width(file: &FileDiff, metrics: &Metrics) -> f32 {",
  "+    let digits = file.max_line.max(1).ilog10() + 1;",
  "+    (digits as f32 * 6.6 + 8.0 + 6.0).max(metrics.diff_gutter_width)",
  " }",
].join("\n")
const HOT_RELOAD = `**No.** A \`.node\` cannot unload. The loop rebuilds and restarts.`
const SKILLS = `Skills are \`SKILL.md\` files. A mail-style list on the left, the body on the right.`
const WIRE_MODELS = `Default is DeepSeek V4 Flash. Keep Opus for long diffs. Hide GPT-5.4 behind the picker.`
const SDK_VS_GPUI = `GPUI is the renderer. A native SDK would still talk to it. GPUIX is the ilha layer on that same GPUI tree, so you keep JSX and skip a second UI stack.`
const SCRIPT_C = `scriptc is a Vercel Labs experiment. This demo has no live runtime for it. The chat still shows how a coding agent would walk that kind of patch in GPUIX.`
const MEMORY = `The chat example keeps one retained ilha component per turn. Pass \`itemCount\` and a window when the list grows. Native paint stays on visible rows only.`

const TURNS: Turn[] = [
  { kind: "user", text: "give me a quick overview" },
  { kind: "fold", duration: "Worked for 10 seconds" },
  { kind: "markdown", source: OVERVIEW },
  { kind: "user", text: "How does ilha reach GPUI?" },
  { kind: "fold", duration: "Worked for 6 seconds" },
  { kind: "markdown", source: ARCHITECTURE },
  { kind: "user", text: "How does cross-element text selection work?" },
  { kind: "fold", duration: "Worked for 14 seconds" },
  { kind: "markdown", source: SELECTION },
  { kind: "code", language: "rust", source: SELECTION_CODE },
  { kind: "user", text: "Make the diff gutter width adapt to the largest line number." },
  { kind: "fold", duration: "Worked for 8 seconds" },
  { kind: "markdown", source: GUTTER },
  { kind: "diff", patch: GUTTER_DIFF },
  { kind: "user", text: "Do I get hot reload when I edit the Rust side?" },
  { kind: "fold", duration: "Worked for 4 seconds" },
  { kind: "markdown", source: HOT_RELOAD },
  { kind: "user", text: "How do skills show up in the app?" },
  { kind: "fold", duration: "Worked for 7 seconds" },
  { kind: "markdown", source: SKILLS },
  { kind: "user", text: "Which models should I wire up?" },
  { kind: "fold", duration: "Worked for 5 seconds" },
  { kind: "markdown", source: WIRE_MODELS },
]

const CONVERSATIONS: Conversation[] = [
  { id: "c1", title: "give me a quick overview", group: "Yesterday", project: "gpuix", time: "16m", turns: TURNS },
  {
    id: "c2",
    title: "Native SDK vs GPUI comparison",
    group: "Yesterday",
    project: "No project",
    time: "14h",
    turns: [
      { kind: "user", text: "Native SDK vs GPUI comparison" },
      { kind: "fold", duration: "Worked for 9 seconds" },
      { kind: "markdown", source: SDK_VS_GPUI },
    ],
  },
  {
    id: "c3",
    title: "Vercel Labs scriptc implementat...",
    group: "Yesterday",
    project: "No project",
    time: "15h",
    turns: [
      { kind: "user", text: "Vercel Labs scriptc implementation notes" },
      { kind: "fold", duration: "Worked for 12 seconds" },
      { kind: "markdown", source: SCRIPT_C },
    ],
  },
  {
    id: "c4",
    title: "check if any memory optimizatio...",
    group: "This Month",
    project: "gpuix",
    time: "2d",
    turns: [
      { kind: "user", text: "check if any memory optimizations are left" },
      { kind: "fold", duration: "Worked for 11 seconds" },
      { kind: "markdown", source: MEMORY },
    ],
  },
]


function IconButton(props: {
  icon: IconName
  onClick?: () => void
  dimmed?: boolean
  size?: number
  testId?: string
}) {
  return (
    <Button
      testId={props.testId}
      disabled={props.dimmed}
      style={{
        width: 26,
        height: 26,
        flexShrink: 0,
        borderRadius: 6,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        cursor: "pointer",
        opacity: props.dimmed ? 0.35 : 1,
        hover: props.dimmed ? undefined : { backgroundColor: C.overlay },
        active: props.dimmed ? undefined : { backgroundColor: C.overlayStrong },
      }}
      onClick={() => props.onClick?.()}
    >
      <Icon name={props.icon} size={props.size ?? 14} color={C.tertiary} />
    </Button>
  )
}

function SidebarAction(props: { icon: IconName; label: string; onClick?: () => void; testId?: string }) {
  return (
    <Button
      testId={props.testId}
      onClick={() => props.onClick?.()}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 10,
        height: 32,
        paddingLeft: 4,
        paddingRight: 4,
        borderRadius: 7,
        cursor: "pointer",
        hover: { backgroundColor: C.item },
        active: { backgroundColor: C.overlayStrong },
      }}
    >
      <div
        style={{
          width: 20,
          height: 20,
          flexShrink: 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon name={props.icon} size={14} color={C.secondary} />
      </div>
      <text style={{ fontSize: 13, color: C.secondary }}>{props.label}</text>
    </Button>
  )
}

function ConversationRow(props: {
  conversation: Conversation
  active: boolean
  onSelect: (id: string) => void
}) {
  return (
    <Button
      testId={`thread-${props.conversation.id}`}
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 4,
        paddingLeft: 8,
        paddingRight: 8,
        paddingTop: 7,
        paddingBottom: 7,
        borderRadius: 7,
        cursor: "pointer",
        backgroundColor: props.active ? C.item : "#00000000",
        hover: { backgroundColor: C.item },
      }}
      onClick={() => props.onSelect(props.conversation.id)}
    >
      <text
        style={{
          fontSize: 13.5,
          lineHeight: 18,
          color: C.text,
          whiteSpace: "nowrap",
          textOverflow: "ellipsis",
        }}
      >
        {props.conversation.title}
      </text>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 5 }}>
        <Icon name="folder" size={12.5} color={C.tertiary} />
        <text
          style={{
            fontSize: 13,
            lineHeight: 15,
            color: C.tertiary,
            flexGrow: 1,
            minWidth: 0,
            whiteSpace: "nowrap",
            textOverflow: "ellipsis",
          }}
        >
          {props.conversation.project}
        </text>
        <text style={{ fontSize: 12.5, color: C.ghost, flexShrink: 0 }}>{props.conversation.time}</text>
      </div>
    </Button>
  )
}

type Mode = "build" | "plan"

/**
 * The app's state and actions, created once in `ChatApp` and handed to every
 * component. The atoms are read where they are painted, never in `ChatApp`.
 */
interface Chat {
  conversations: AtomHandle<Conversation[]>
  activeId: AtomHandle<string>
  nav: AtomHandle<{ stack: string[]; index: number }>
  collapsed: AtomHandle<boolean>
  inspectorOpen: AtomHandle<boolean>
  overlay: AtomHandle<"search" | "settings" | null>
  query: AtomHandle<string>
  projectOnly: AtomHandle<boolean>
  draft: AtomHandle<string>
  model: AtomHandle<string>
  reasoning: AtomHandle<string>
  access: AtomHandle<string>
  mode: AtomHandle<Mode>
  project: AtomHandle<string>
  workspace: AtomHandle<string>
  branch: AtomHandle<string>
  /** Elements the actions reach by id: focus, scroll, the search field. */
  refs: {
    composer: HostElement | null
    list: HostElement | null
    search: HostElement | null
  }
  goTo(id: string): void
  goBack(): void
  goForward(): void
  newTask(): void
  send(text: string): void
  retry(): void
  cycleFrameOverlay(): void
}

const activeOf = (chat: Chat): Conversation | undefined =>
  chat.conversations().find((conversation) => conversation.id === chat.activeId())

const labelOf = (items: readonly { id: string; label: string }[], id: string): string =>
  items.find((item) => item.id === id)?.label ?? id

function Sidebar(props: { chat: Chat }) {
  const { chat } = props
  const nav = chat.nav()
  const activeId = chat.activeId()
  const filterActive = chat.projectOnly()
  const project = labelOf(PROJECTS, chat.project())
  const groups: { name: string; items: Conversation[] }[] = []
  for (const conversation of chat.conversations()) {
    if (filterActive && conversation.project !== project) continue
    const last = groups[groups.length - 1]
    if (last && last.name === conversation.group) last.items.push(conversation)
    else groups.push({ name: conversation.group, items: [conversation] })
  }

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        width: SIDEBAR_WIDTH,
        flexShrink: 0,
        height: "100%",
        backgroundColor: C.sidebar,
        userSelect: "none",
      }}
    >
      <div
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          height: TITLEBAR_HEIGHT,
          flexShrink: 0,
        }}
      >
        <div style={{ width: TRAFFIC_LIGHT_CLEARANCE, height: "100%", flexShrink: 0 }} />
        <IconButton icon="sidebar" size={16} testId="sidebar-collapse" onClick={() => chat.collapsed.set(true)} />
        <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 2, marginLeft: 6 }}>
          <IconButton icon="arrowLeft" dimmed={nav.index === 0} testId="history-back" onClick={chat.goBack} />
          <IconButton
            icon="arrowRight"
            dimmed={nav.index >= nav.stack.length - 1}
            testId="history-forward"
            onClick={chat.goForward}
          />
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", paddingLeft: 10, paddingRight: 10 }}>
        <SidebarAction icon="compose" label="New Task" testId="new-task" onClick={chat.newTask} />
      </div>

      <div
        style={{
          display: "flex",
          flexDirection: "column",
          flexGrow: 1,
          minHeight: 0,
          overflowY: "scroll",
          paddingLeft: 10,
          paddingRight: 10,
        }}
      >
        <div style={{ paddingBottom: 6 }}>
          <SidebarAction
            icon="search"
            label="Search"
            testId="search"
            onClick={() => {
              chat.query.set("")
              chat.overlay.set("search")
            }}
          />
        </div>
        {groups.map((group, groupIndex) => (
          <div key={group.name} style={{ display: "flex", flexDirection: "column", paddingBottom: 10 }}>
            <div
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                height: 28,
                paddingLeft: 8,
                paddingRight: 8,
              }}
            >
              <text style={{ fontSize: 13, fontWeight: 500, color: C.secondary, flexGrow: 1, minWidth: 0 }}>
                {group.name}
              </text>
              {groupIndex === 0 && (
                <Button
                  testId="thread-filter"
                  aria-label="Filter by project"
                  onClick={() => chat.projectOnly.update((value) => !value)}
                  style={{
                    width: 22,
                    height: 22,
                    borderRadius: 6,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    cursor: "pointer",
                    backgroundColor: filterActive ? C.overlayStrong : "#00000000",
                    hover: { backgroundColor: C.overlay },
                  }}
                >
                  <Icon name="listFilter" size={14} color={filterActive ? C.text : C.secondary} />
                </Button>
              )}
            </div>
            {group.items.map((conversation) => (
              <ConversationRow
                key={conversation.id}
                conversation={conversation}
                active={conversation.id === activeId}
                onSelect={chat.goTo}
              />
            ))}
          </div>
        ))}
      </div>

      <div
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          height: 40,
          flexShrink: 0,
          paddingLeft: 10,
          paddingRight: 10,
        }}
      >
        <IconButton icon="settings" testId="settings" onClick={() => chat.overlay.set("settings")} />
      </div>
    </div>
  )
}

/** The sidebar and its hairline, animated to zero width when collapsed. */
function SidebarPane(props: { chat: Chat }) {
  return (
    <motion.div
      initial={false}
      animate={{ width: props.chat.collapsed() ? 0 : SIDEBAR_WIDTH + 1 }}
      transition={{ duration: 0.2, ease: "easeOut" }}
      style={{ display: "flex", flexDirection: "row", height: "100%", flexShrink: 0, overflow: "hidden" }}
    >
      <Sidebar chat={props.chat} />
      <div style={{ width: 1, height: "100%", flexShrink: 0, backgroundColor: C.sidebarBorder }} />
    </motion.div>
  )
}

function UserTurn(props: { text: string }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-end", width: "100%" }}>
      <div
        style={{
          maxWidth: 540,
          minWidth: 0,
          backgroundColor: C.raised,
          borderRadius: 12,
          paddingTop: 8,
          paddingBottom: 8,
          paddingLeft: 12,
          paddingRight: 12,
        }}
      >
        <text style={{ fontSize: 14, lineHeight: 20, color: C.text, minWidth: 0, maxWidth: "100%" }}>
          {props.text}
        </text>
      </div>
    </div>
  )
}

function WorkedFor(props: { duration: string }) {
  const open = atom(false)
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8, width: "100%" }}>
      <Button
        aria-expanded={open()}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 10,
          height: 24,
          width: "100%",
          cursor: "pointer",
        }}
        onClick={() => open.update((value) => !value)}
      >
        <div style={{ height: 1, flexGrow: 1, backgroundColor: C.border }} />
        <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 5, flexShrink: 0 }}>
          <text style={{ fontSize: 13.5, lineHeight: 18, fontWeight: 500, color: C.tertiary }}>
            {props.duration}
          </text>
          <Icon name={open() ? "chevronDown" : "chevronRight"} size={11.5} color={C.tertiary} />
        </div>
        <div style={{ height: 1, flexGrow: 1, backgroundColor: C.border }} />
      </Button>
      {open() && (
        <text style={{ fontSize: 13, lineHeight: 18, color: C.secondary }}>
          Demo reasoning. No model ran. The fold is here so the chrome has something to open.
        </text>
      )}
    </div>
  )
}

const ROW_INNER_STYLE = { width: CONTENT_MAX_WIDTH, maxWidth: "100%" } as const
const ROW_STYLE = {
  display: "flex",
  flexDirection: "row",
  justifyContent: "center",
  width: "100%",
  paddingTop: 8,
  paddingBottom: 8,
  paddingLeft: 20,
  paddingRight: 20,
} as const
const ROW_STYLE_FIRST = { ...ROW_STYLE, paddingTop: 22 } as const
const ROW_STYLE_LAST = { ...ROW_STYLE, paddingBottom: 22 } as const
const ROW_STYLE_ONLY = { ...ROW_STYLE, paddingTop: 22, paddingBottom: 22 } as const

const CODE_CARD_STYLE = {
  display: "flex",
  flexDirection: "column",
  width: "100%",
  minWidth: 0,
  borderRadius: 10,
  borderWidth: 1,
  borderColor: C.border,
  backgroundColor: "#FFFFFF09",
  overflow: "hidden",
} as const
const CODE_HEADER_STYLE = {
  paddingLeft: 12,
  paddingRight: 12,
  paddingTop: 5,
  paddingBottom: 5,
  borderBottomWidth: 1,
  borderColor: C.border,
  backgroundColor: "#FFFFFF05",
} as const
const CODE_BODY_STYLE = {
  minWidth: 0,
  paddingLeft: 12,
  paddingRight: 12,
  paddingTop: 10,
  paddingBottom: 10,
} as const

function CodeBlock(props: { code: string; language?: string; showLineNumbers?: boolean }) {
  return (
    <div style={CODE_CARD_STYLE}>
      {props.language && (
        <div style={CODE_HEADER_STYLE}>
          <text style={{ fontSize: 12, color: C.secondary }}>{props.language}</text>
        </div>
      )}
      <code
        code={props.code}
        language={props.language}
        showLineNumbers={props.showLineNumbers}
        theme={CHAT_THEME}
        style={CODE_BODY_STYLE}
      />
    </div>
  )
}

function Markdown(props: { source: string }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", width: "100%", minWidth: 0 }}>
      <markdown source={props.source} theme={CHAT_THEME} />
    </div>
  )
}

function expandTurns(count: number): Turn[] {
  if (count <= TURNS.length) return TURNS.slice(0, count)
  const out = new Array<Turn>(count)
  for (let i = 0; i < count; i++) out[i] = TURNS[i % TURNS.length]!
  return out
}

function seedTurnsFor(id: string, turnCount: number): Turn[] {
  if (id === "c1") return expandTurns(turnCount)
  return CONVERSATIONS.find((conversation) => conversation.id === id)?.turns.slice() ?? []
}

function demoReply(args: { text: string; modelLabel: string; mode: "build" | "plan" }): Turn[] {
  const quoted = args.text.length > 80 ? `${args.text.slice(0, 77)}...` : args.text
  const modeLine =
    args.mode === "plan"
      ? "Plan mode is on, so this is a sketch, not a patch."
      : "Build mode is on. This still stays in the demo."
  return [
    { kind: "fold", duration: "Worked for 2 seconds" },
    {
      kind: "markdown",
      source: `This is the GPUIX chat demo. No model ran. You wrote "${quoted}". ${args.modelLabel} would answer here. ${modeLine}`,
    },
  ]
}

function titleFromDraft(text: string) {
  const first = text.trim().split(/\s+/).slice(0, 6).join(" ")
  return first.length > 42 ? `${first.slice(0, 39)}...` : first
}

function Inspector(props: { chat: Chat }) {
  const { chat } = props
  const conversation = activeOf(chat)
  const rows = [
    ["Thread", conversation?.title ?? "New task"],
    ["Project", labelOf(PROJECTS, chat.project())],
    ["Model", labelOf(MODELS, chat.model())],
    ["Reasoning", labelOf(REASONING, chat.reasoning())],
    ["Access", labelOf(ACCESS, chat.access())],
    ["Mode", chat.mode() === "plan" ? "Plan" : "Build"],
    ["Turns", String(conversation?.turns.length ?? 0)],
  ] as const
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        width: 260,
        flexShrink: 0,
        height: "100%",
        backgroundColor: C.sidebar,
        borderLeftWidth: 1,
        borderColor: C.sidebarBorder,
        paddingTop: 14,
        paddingLeft: 14,
        paddingRight: 14,
        gap: 10,
      }}
    >
      <text style={{ fontSize: 13, fontWeight: 600, color: C.text }}>Inspector</text>
      {rows.map(([label, value]) => (
        <div key={label} style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          <text style={{ fontSize: 11.5, color: C.ghost }}>{label}</text>
          <text style={{ fontSize: 13, color: C.secondary }}>{value}</text>
        </div>
      ))}
    </div>
  )
}

function InspectorPane(props: { chat: Chat }) {
  return props.chat.inspectorOpen() ? <Inspector chat={props.chat} /> : null
}

/** A modal card. Dialog owns Escape, the backdrop press, the Tab trap, and
 *  moving focus in on open and back out on close. */
function OverlayCard(props: {
  title: string
  open: boolean
  onClose: () => void
  children?: View
  height?: number
  initialFocus?: DialogPopupProps["initialFocus"]
}) {
  return (
    <Dialog open={props.open} onOpenChange={(next) => !next && props.onClose()}>
      <DialogPortal>
        <DialogBackdrop style={{ backgroundColor: "#00000066" }} />
        <DialogPopup
          initialFocus={props.initialFocus}
          style={{
            width: 420,
            height: props.height,
            maxWidth: "90%",
            backgroundColor: C.raised,
            borderRadius: 12,
            borderWidth: 1,
            borderColor: C.borderStrong,
            padding: 16,
            display: "flex",
            flexDirection: "column",
            gap: 12,
          }}
        >
          <div style={{ display: "flex", flexDirection: "row", alignItems: "center" }}>
            <DialogTitle style={{ flexGrow: 1 }}>
              <text style={{ fontSize: 14, fontWeight: 600, color: C.text }}>{props.title}</text>
            </DialogTitle>
            <DialogClose
              testId="overlay-close"
              style={{
                height: 24,
                paddingLeft: 8,
                paddingRight: 8,
                borderRadius: 6,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                cursor: "pointer",
                hover: { backgroundColor: C.overlay },
              }}
            >
              <text style={{ fontSize: 12, color: C.secondary }}>Close</text>
            </DialogClose>
          </div>
          {props.children}
        </DialogPopup>
      </DialogPortal>
    </Dialog>
  )
}

function turnBody(turn: Turn): View {
  switch (turn.kind) {
    case "user":
      return <UserTurn text={turn.text} />
    case "fold":
      return <WorkedFor duration={turn.duration} />
    case "markdown":
      return <Markdown source={turn.source} />
    case "code":
      return <CodeBlock code={turn.source} language={turn.language} showLineNumbers />
    case "diff":
      return <diff patch={turn.patch} wordDiff theme={CHAT_THEME} />
  }
}

function TranscriptRow(props: { turn: Turn; first: boolean; last: boolean; onRetry: () => void }) {
  const style =
    props.first && props.last
      ? ROW_STYLE_ONLY
      : props.first
        ? ROW_STYLE_FIRST
        : props.last
          ? ROW_STYLE_LAST
          : ROW_STYLE
  return (
    <div style={style}>
      <div style={ROW_INNER_STYLE}>
        {turnBody(props.turn)}
        {props.last && props.turn.kind !== "user" && <ActionBar onRetry={props.onRetry} />}
      </div>
    </div>
  )
}

/**
 * The rows, as a list of keyed components and nothing else. ilha reuses such a
 * list by key and reruns only the rows whose props changed, so sending a
 * message mounts the new rows, repaints the row that is no longer last, and
 * leaves the other thousand alone.
 *
 * The key carries the thread, so a fold opened in one thread is not open in
 * the row that takes its place in another.
 */
function TranscriptRows(props: { chat: Chat }) {
  const { chat } = props
  const threadId = chat.activeId()
  const turns = activeOf(chat)?.turns ?? []
  return turns.map((turn, index) => (
    <TranscriptRow
      key={`${threadId}:${index}`}
      turn={turn}
      first={index === 0}
      last={index === turns.length - 1}
      onRetry={chat.retry}
    />
  ))
}

function Transcript(props: { chat: Chat }) {
  const { chat } = props
  return (
    <virtual-list
      ref={(element) => {
        chat.refs.list = element
      }}
      overdraw={240}
      estimatedItemHeight={220}
      style={{ flexGrow: 1, minHeight: 0, width: "100%" }}
    >
      <TranscriptRows chat={chat} />
    </virtual-list>
  )
}

function Header(props: { chat: Chat }) {
  const { chat } = props
  const collapsed = chat.collapsed()
  const nav = chat.nav()
  const conversation = activeOf(chat)
  const turnCount = conversation?.turns.length ?? 0
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 8,
        height: TITLEBAR_HEIGHT,
        flexShrink: 0,
        paddingLeft: collapsed ? 0 : 14,
        paddingRight: 14,
        userSelect: "none",
      }}
    >
      {collapsed && <div style={{ width: TRAFFIC_LIGHT_CLEARANCE - 8, height: "100%", flexShrink: 0 }} />}
      {collapsed && (
        <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}>
          <IconButton icon="sidebar" testId="sidebar-expand" onClick={() => chat.collapsed.set(false)} />
          <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 2 }}>
            <IconButton icon="arrowLeft" dimmed={nav.index === 0} onClick={chat.goBack} />
            <IconButton icon="arrowRight" dimmed={nav.index >= nav.stack.length - 1} onClick={chat.goForward} />
          </div>
        </div>
      )}
      <text
        style={{
          fontSize: 13,
          fontWeight: 500,
          color: C.text,
          whiteSpace: "nowrap",
          textOverflow: "ellipsis",
          minWidth: 0,
          flexShrink: 1,
        }}
      >
        {conversation?.title ?? "New task"}
      </text>
      {turnCount > TURNS.length && (
        <text style={{ fontSize: 12, fontWeight: 500, color: C.tertiary, whiteSpace: "nowrap", flexShrink: 0 }}>
          {`${turnCount.toLocaleString("en-US")} messages`}
        </text>
      )}
      <div style={{ flexGrow: 1 }} />
      <IconButton
        icon="panelRight"
        testId="inspector-toggle"
        onClick={() => chat.inspectorOpen.update((value) => !value)}
      />
    </div>
  )
}

const MENU = {
  minWidth: 220,
  paddingTop: 4,
  paddingBottom: 4,
  paddingLeft: 4,
  paddingRight: 4,
  backgroundColor: C.raised,
  borderWidth: 1,
  borderColor: C.borderStrong,
  borderRadius: 12,
} satisfies StyleDesc

function menuItemStyle(
  state: { selected: boolean; highlighted: boolean },
  description?: boolean,
): StyleDesc {
  return {
    display: "flex",
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
    width: "100%",
    paddingTop: description ? 6 : 5,
    paddingBottom: description ? 6 : 5,
    paddingLeft: 8,
    paddingRight: 8,
    borderRadius: 7,
    backgroundColor: state.highlighted ? "#404040" : state.selected ? "#2C2C2C" : C.raised,
    hover: { backgroundColor: "#404040" },
    cursor: "pointer",
  }
}

function MenuRowInner(props: {
  label: string
  description?: string
  icon?: IconName
  selected: boolean
  hint?: string
}) {
  return (
    <>
      {props.icon && <Icon name={props.icon} size={14} color={C.tertiary} />}
      <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, minWidth: 0 }}>
        <text
          style={{
            fontSize: 12.5,
            fontWeight: props.selected ? 600 : 500,
            color: C.text,
            whiteSpace: "nowrap",
            textOverflow: "ellipsis",
          }}
        >
          {props.label}
        </text>
        {props.description && (
          <text style={{ fontSize: 12.5, lineHeight: 14, color: C.tertiary, paddingTop: 2 }}>
            {props.description}
          </text>
        )}
      </div>
      {props.hint && <text style={{ fontSize: 11.5, color: C.ghost, flexShrink: 0 }}>{props.hint}</text>}
      {props.selected && <Icon name="check" size={11} color={C.tertiary} />}
    </>
  )
}

function ChipSelect(props: {
  value: string
  onChange: (next: string) => void
  items: readonly { id: string; label: string }[]
  icon: IconName
  label: string
  caret?: boolean
  accent?: boolean
  menuWidth?: number
  testId?: string
  children?: View
}) {
  return (
    <Select
      items={props.items.map((item) => ({ value: item.id, label: item.label }))}
      value={props.value}
      onValueChange={props.onChange}
      style={{ flexShrink: 0 }}
    >
      <div style={{ position: "relative", display: "flex" }}>
        <SelectTrigger
          testId={props.testId}
          style={(state) => ({
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            height: 26,
            paddingLeft: 7,
            paddingRight: 7,
            borderRadius: 6,
            cursor: "pointer",
            backgroundColor: state.open ? C.overlay : "#00000000",
            hover: { backgroundColor: C.overlay },
          })}
        >
          <Icon name={props.icon} size={12} color={props.accent ? C.accent : C.tertiary} />
          <text
            style={{
              fontSize: 13,
              lineHeight: 16,
              color: props.accent ? C.accent : C.secondary,
              whiteSpace: "nowrap",
              textOverflow: "ellipsis",
            }}
          >
            {props.label}
          </text>
          {(props.caret ?? true) && <Icon name="chevronDown" size={10.5} color={C.ghost} />}
        </SelectTrigger>
        <SelectContent side="top" sideOffset={4} style={{ ...MENU, minWidth: props.menuWidth ?? 220 }}>
          {props.children}
        </SelectContent>
      </div>
    </Select>
  )
}

const MENU_LABEL_STYLE = { height: 22, paddingLeft: 8, paddingRight: 8, display: "flex", alignItems: "center" } as const

function MenuLabel(props: { text: string }) {
  return (
    <SelectLabel style={MENU_LABEL_STYLE}>
      <text style={{ fontSize: 11.5, fontWeight: 500, color: C.ghost }}>{props.text}</text>
    </SelectLabel>
  )
}

function ModelPicker(props: { value: AtomHandle<string> }) {
  const selected = MODELS.find((model) => model.id === props.value()) ?? MODELS[0]!
  const groups: { name: string; items: typeof MODELS }[] = []
  for (const model of MODELS) {
    const last = groups[groups.length - 1]
    if (last && last.name === model.group) last.items.push(model)
    else groups.push({ name: model.group, items: [model] })
  }
  return (
    <ChipSelect value={props.value()} onChange={props.value.set} items={MODELS} icon={selected.icon} label={selected.label} testId="model-picker">
      {groups.map((group, index) => (
        <div key={group.name} style={{ display: "flex", flexDirection: "column" }}>
          {index > 0 && <div style={{ height: 1, backgroundColor: C.border, marginTop: 4, marginBottom: 4 }} />}
          <MenuLabel text={group.name} />
          {group.items.map((model) => (
            <SelectItem
              key={model.id}
              value={model.id}
              testId={`model-${model.id}`}
              style={(state) => menuItemStyle(state)}
            >
              {(state) => <MenuRowInner label={model.label} icon={model.icon} selected={state.selected} />}
            </SelectItem>
          ))}
        </div>
      ))}
    </ChipSelect>
  )
}

function ReasoningPicker(props: { value: AtomHandle<string> }) {
  const selected = REASONING.find((option) => option.id === props.value()) ?? REASONING[0]!
  return (
    <ChipSelect
      value={props.value()}
      onChange={props.value.set}
      items={REASONING}
      icon={props.value() === "low" ? "zap" : "sparkle"}
      label={selected.label}
      caret={false}
    >
      <MenuLabel text="Reasoning" />
      {REASONING.map((option) => (
        <SelectItem
          key={option.id}
          value={option.id}
          testId={`reasoning-${option.id}`}
          style={(state) => menuItemStyle(state)}
        >
          {(state) => <MenuRowInner label={option.label} hint={option.hint} selected={state.selected} />}
        </SelectItem>
      ))}
    </ChipSelect>
  )
}

function AccessPicker(props: { value: AtomHandle<string> }) {
  const selected = ACCESS.find((option) => option.id === props.value()) ?? ACCESS[3]!
  return (
    <ChipSelect
      value={props.value()}
      onChange={props.value.set}
      items={ACCESS}
      icon={selected.icon}
      label={selected.label}
      caret={false}
      menuWidth={288}
    >
      {ACCESS.map((option) => (
        <SelectItem
          key={option.id}
          value={option.id}
          testId={`access-${option.id}`}
          style={(state) => menuItemStyle(state, true)}
        >
          {(state) => (
            <MenuRowInner
              label={option.label}
              description={option.description}
              icon={option.icon}
              selected={state.selected}
            />
          )}
        </SelectItem>
      ))}
    </ChipSelect>
  )
}

function ProjectPicker(props: { value: AtomHandle<string> }) {
  const selected = PROJECTS.find((option) => option.id === props.value()) ?? PROJECTS[0]!
  return (
    <ChipSelect value={props.value()} onChange={props.value.set} items={PROJECTS} icon="folder" label={selected.label} caret={false}>
      {PROJECTS.map((option) => (
        <SelectItem
          key={option.id}
          value={option.id}
          testId={`project-${option.id}`}
          style={(state) => menuItemStyle(state)}
        >
          {(state) => <MenuRowInner label={option.label} icon="folder" selected={state.selected} />}
        </SelectItem>
      ))}
    </ChipSelect>
  )
}

function WorkspacePicker(props: { value: AtomHandle<string> }) {
  const selected = WORKSPACES.find((option) => option.id === props.value()) ?? WORKSPACES[0]!
  return (
    <ChipSelect value={props.value()} onChange={props.value.set} items={WORKSPACES} icon={selected.icon} label={selected.label} caret={false}>
      <MenuLabel text="Work in" />
      {WORKSPACES.map((option) => (
        <SelectItem
          key={option.id}
          value={option.id}
          testId={`workspace-${option.id}`}
          style={(state) => menuItemStyle(state)}
        >
          {(state) => <MenuRowInner label={option.label} icon={option.icon} selected={state.selected} />}
        </SelectItem>
      ))}
    </ChipSelect>
  )
}

function BranchPicker(props: { value: AtomHandle<string> }) {
  const selected = BRANCHES.find((option) => option.id === props.value()) ?? BRANCHES[0]!
  return (
    <ChipSelect value={props.value()} onChange={props.value.set} items={BRANCHES} icon="gitBranch" label={selected.label}>
      {BRANCHES.map((option) => (
        <SelectItem
          key={option.id}
          value={option.id}
          testId={`branch-${option.id}`}
          style={(state) => menuItemStyle(state)}
        >
          {(state) => <MenuRowInner label={option.label} icon="gitBranch" selected={state.selected} />}
        </SelectItem>
      ))}
    </ChipSelect>
  )
}

function ModeToggle(props: { value: AtomHandle<Mode> }) {
  const plan = props.value() === "plan"
  return (
    <Button
      aria-label="Mode"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        height: 26,
        paddingLeft: 7,
        paddingRight: 7,
        borderRadius: 6,
        cursor: "pointer",
        hover: { backgroundColor: C.overlay },
      }}
      onClick={() => props.value.set(plan ? "build" : "plan")}
    >
      <Icon name={plan ? "list" : "wrench"} size={12} color={plan ? C.accent : C.tertiary} />
      <text style={{ fontSize: 13, lineHeight: 16, color: plan ? C.accent : C.secondary }}>
        {plan ? "Plan" : "Build"}
      </text>
    </Button>
  )
}

/** Reads the draft on its own, so typing repaints this button and nothing else. */
function SendButton(props: { chat: Chat }) {
  const { chat } = props
  const ready = chat.draft().trim().length > 0
  return (
    <div
      testId="send"
      style={{
        width: 26,
        height: 26,
        borderRadius: 13,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        cursor: ready ? "pointer" : undefined,
        backgroundColor: ready ? C.inverse : C.overlayStrong,
        hover: ready ? { opacity: 0.9 } : undefined,
      }}
      onclick={() => chat.send(chat.draft())}
    >
      <Icon name="send" size={16} color={ready ? C.onInverse : C.ghost} />
    </div>
  )
}

function Composer(props: { chat: Chat }) {
  const { chat } = props
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        flexShrink: 0,
        paddingLeft: 20,
        paddingRight: 20,
        overflow: "visible",
        userSelect: "none",
      }}
    >
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          width: "100%",
          maxWidth: CONTENT_MAX_WIDTH,
          overflow: "visible",
          backgroundColor: C.composer,
          borderRadius: 13,
          borderWidth: 1,
          borderColor: C.border,
          paddingTop: 10,
          paddingBottom: 10,
        }}
      >
        <textarea
          ref={(element) => {
            chat.refs.composer = element
          }}
          testId="composer"
          value={chat.draft}
          placeholder="Do anything..."
          minRows={1}
          maxRows={3}
          autoFocus
          theme={CHAT_THEME}
          style={{
            width: "100%",
            minWidth: 0,
            fontSize: 14,
            lineHeight: 20,
            color: C.text,
            backgroundColor: "#00000000",
            borderWidth: 0,
            paddingLeft: 10,
            paddingRight: 10,
          }}
          oninput={(event) => chat.draft.set(event.value ?? "")}
          onsubmit={(event) => chat.send(event.value ?? chat.draft())}
        />
        <div
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 4,
            marginTop: 8,
            paddingLeft: 10,
            paddingRight: 10,
          }}
        >
          <ModelPicker value={chat.model} />
          <ReasoningPicker value={chat.reasoning} />
          <AccessPicker value={chat.access} />
          <ModeToggle value={chat.mode} />
          <div style={{ flexGrow: 1 }} />
          <SendButton chat={chat} />
        </div>
      </div>
    </div>
  )
}

function WorkspaceFooter(props: { chat: Chat }) {
  const { chat } = props
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        flexShrink: 0,
        paddingLeft: 20,
        paddingRight: 20,
        paddingTop: 4,
        paddingBottom: 8,
        userSelect: "none",
      }}
    >
      <div
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 2,
          width: "100%",
          maxWidth: CONTENT_MAX_WIDTH,
          height: 28,
          paddingLeft: 10,
          paddingRight: 10,
        }}
      >
        <ProjectPicker value={chat.project} />
        <WorkspacePicker value={chat.workspace} />
        {chat.project() !== "none" && <BranchPicker value={chat.branch} />}
        <div style={{ flexGrow: 1 }} />
        <div style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: "#3B82F6", flexShrink: 0 }} />
      </div>
    </div>
  )
}

function GhostButton(props: {
  icon: IconName
  label?: string
  active?: boolean
  onClick?: () => void
  testId?: string
}) {
  const color = props.active ? C.text : C.ghost
  return (
    <Button
      testId={props.testId}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        height: 30,
        paddingLeft: props.label ? 9 : 0,
        paddingRight: props.label ? 11 : 0,
        width: props.label ? undefined : 30,
        justifyContent: "center",
        borderRadius: 10,
        cursor: "pointer",
        backgroundColor: props.active ? C.overlayStrong : "#00000000",
        hover: { backgroundColor: C.overlay },
      }}
      onClick={() => props.onClick?.()}
    >
      <Icon name={props.icon} size={16} color={color} />
      {props.label && <text style={{ fontSize: 12.5, color }}>{props.label}</text>}
    </Button>
  )
}

function ActionBar(props: { onRetry?: () => void }) {
  const copied = atom(false)
  const feedback = atom<"up" | "down" | null>(null)
  const note = atom<string | null>(null)
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "flex-start",
        gap: 4,
        paddingTop: 6,
        marginLeft: -7,
        userSelect: "none",
      }}
    >
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4 }}>
        <GhostButton
          icon={copied() ? "check" : "copy"}
          active={copied()}
          onClick={() => {
            copied.set(true)
            note.set("Copied. Clipboard is a demo in this example.")
          }}
        />
        <GhostButton
          icon="thumbsUp"
          active={feedback() === "up"}
          onClick={() => {
            feedback.update((value) => (value === "up" ? null : "up"))
            note.set(null)
          }}
        />
        <GhostButton
          icon="thumbsDown"
          active={feedback() === "down"}
          onClick={() => {
            feedback.update((value) => (value === "down" ? null : "down"))
            note.set(null)
          }}
        />
        <GhostButton
          icon="retry"
          testId="retry"
          onClick={() => {
            props.onRetry?.()
            note.set("Ran the demo reply again.")
          }}
        />
        <GhostButton icon="share" onClick={() => note.set("Share is a demo. No link left this window.")} />
        <GhostButton icon="more" onClick={() => note.set("More actions are a demo.")} />
      </div>
      {note() && <text style={{ fontSize: 12, color: C.tertiary, paddingLeft: 9 }}>{note()}</text>}
    </div>
  )
}

/** The transcript column. `windowInsets()` keeps the composer above the keyboard. */
function Main(props: { chat: Chat }) {
  const { chat } = props
  const insets = windowInsets()
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        flexGrow: 1,
        minWidth: 0,
        height: "100%",
        paddingBottom: insets().ime.bottom,
        backgroundColor: C.canvas,
      }}
    >
      <Header chat={chat} />
      <Transcript chat={chat} />
      <Composer chat={chat} />
      <WorkspaceFooter chat={chat} />
    </div>
  )
}

function Overlays(props: { chat: Chat }) {
  const { chat } = props
  const overlay = chat.overlay()
  const query = chat.query().trim().toLowerCase()
  const hits = query
    ? chat.conversations().filter((conversation) => conversation.title.toLowerCase().includes(query))
    : chat.conversations()
  return (
    <>
      <OverlayCard
        title="Search threads"
        height={420}
        open={overlay === "search"}
        onClose={() => chat.overlay.set(null)}
        initialFocus={() => chat.refs.search}
      >
        <input
          ref={(element) => {
            chat.refs.search = element
          }}
          testId="search-input"
          value={chat.query}
          placeholder="Filter by title"
          theme={CHAT_THEME}
          style={{
            width: "100%",
            height: 32,
            flexShrink: 0,
            fontSize: 13,
            color: C.text,
            backgroundColor: C.composer,
            borderRadius: 8,
            paddingLeft: 10,
            paddingRight: 10,
          }}
          oninput={(event) => chat.query.set(event.value ?? "")}
        />
        <div style={{ flexGrow: 1, minHeight: 0, overflowY: "scroll" }}>
          {hits.map((conversation) => (
            <Button
              key={conversation.id}
              testId={`search-${conversation.id}`}
              onClick={() => chat.goTo(conversation.id)}
              style={{
                paddingTop: 8,
                paddingBottom: 8,
                paddingLeft: 8,
                paddingRight: 8,
                borderRadius: 8,
                cursor: "pointer",
                hover: { backgroundColor: C.overlay },
              }}
            >
              <text style={{ fontSize: 13, color: C.text }}>{conversation.title}</text>
            </Button>
          ))}
        </div>
      </OverlayCard>
      <OverlayCard title="Settings" open={overlay === "settings"} onClose={() => chat.overlay.set(null)}>
        <text style={{ fontSize: 13, lineHeight: 18, color: C.secondary }}>
          This is the GPUIX chat demo. Threads, drafts, and replies stay in this window.
        </text>
        <Button
          testId="cycle-overlay"
          onClick={chat.cycleFrameOverlay}
          style={{
            height: 32,
            borderRadius: 8,
            display: "flex",
            alignItems: "center",
            paddingLeft: 10,
            cursor: "pointer",
            backgroundColor: C.overlay,
            hover: { backgroundColor: C.overlayStrong },
          }}
        >
          <text style={{ fontSize: 13, color: C.text }}>Cycle frame overlay</text>
        </Button>
      </OverlayCard>
    </>
  )
}

export function ChatApp(propsIn: { turnCount?: number } = {}) {
  const turnCount = propsIn.turnCount ?? TURNS.length
  const renderer = useGpuixRequired()
  const conversations = atom.lazy(() =>
    CONVERSATIONS.map((conversation) => ({
      ...conversation,
      turns: seedTurnsFor(conversation.id, turnCount),
    })),
  )
  const activeId = atom("c1")
  const nav = atom<{ stack: string[]; index: number }>({ stack: ["c1"], index: 0 })
  const collapsed = atom(false)
  const inspectorOpen = atom(false)
  const overlay = atom<"search" | "settings" | null>(null)
  const query = atom("")
  const projectOnly = atom(false)
  const draft = atom("")
  const model = atom("deepseek-v4-flash")
  const reasoning = atom("high")
  const access = atom("full")
  const mode = atom<Mode>("build")
  const project = atom("gpuix")
  const workspace = atom("local")
  const branch = atom("main")

  // Built on the first render and kept: the atoms above are the same atoms on
  // every render, and nothing here reads one while painting.
  const chat = atom.lazy<Chat>(() => {
    const refs: Chat["refs"] = { composer: null, list: null, search: null }
    let nextTask = 1

    const focusComposer = () => {
      const id = refs.composer?.id
      if (id) renderer.focusElement?.(id)
    }

    // After the commit that adds the rows, so the list knows the new tail.
    const scrollToTail = () => {
      setTimeout(() => {
        const id = refs.list?.id
        const count = conversations().find((conversation) => conversation.id === activeId())?.turns.length ?? 0
        if (id) renderer.scrollToItem?.(id, Math.max(0, count - 1))
      }, 0)
    }

    const show = (id: string, index: number, stack: string[]) => {
      activeId.set(id)
      nav.set({ stack, index })
      focusComposer()
    }

    const reply = (text: string) =>
      demoReply({ text, modelLabel: labelOf(MODELS, model()), mode: mode() })

    const updateActive = (update: (conversation: Conversation) => Conversation) => {
      conversations.update((current) =>
        current.map((conversation) => (conversation.id === activeId() ? update(conversation) : conversation)),
      )
      scrollToTail()
    }

    // The actions are handed out as bare functions, so none of them uses `this`.
    const goTo = (id: string) => {
      overlay.set(null)
      if (id === activeId()) {
        focusComposer()
        return
      }
      const current = nav()
      show(id, current.index + 1, current.stack.slice(0, current.index + 1).concat(id))
    }

    return {
      conversations,
      activeId,
      nav,
      collapsed,
      inspectorOpen,
      overlay,
      query,
      projectOnly,
      draft,
      model,
      reasoning,
      access,
      mode,
      project,
      workspace,
      branch,
      refs,
      goTo,
      goBack() {
        const current = nav()
        if (current.index === 0) return
        show(current.stack[current.index - 1]!, current.index - 1, current.stack)
      },
      goForward() {
        const current = nav()
        if (current.index >= current.stack.length - 1) return
        show(current.stack[current.index + 1]!, current.index + 1, current.stack)
      },
      newTask() {
        const id = `task-${nextTask++}`
        const created: Conversation = {
          id,
          title: "New task",
          group: "Today",
          project: labelOf(PROJECTS, project()),
          time: "now",
          turns: [],
        }
        conversations.update((current) => [created, ...current])
        draft.set("")
        goTo(id)
      },
      send(raw) {
        const text = raw.trim()
        if (!text) return
        updateActive((conversation) => ({
          ...conversation,
          title: conversation.turns.length === 0 ? titleFromDraft(text) : conversation.title,
          turns: [...conversation.turns, { kind: "user", text }, ...reply(text)],
        }))
        draft.set("")
      },
      retry() {
        updateActive((conversation) => {
          const cut = conversation.turns.map((turn) => turn.kind).lastIndexOf("user")
          const lastUser = conversation.turns[cut]
          if (lastUser?.kind !== "user") return conversation
          return { ...conversation, turns: [...conversation.turns.slice(0, cut + 1), ...reply(lastUser.text)] }
        })
      },
      cycleFrameOverlay() {
        renderer.cycleDebugFrameOverlay?.()
      },
    }
  })()

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "row",
        width: "100%",
        height: "100%",
        backgroundColor: C.canvas,
        fontFamily: FONT_SANS,
        color: C.text,
        position: "relative",
      }}
    >
      <SidebarPane chat={chat} />
      <Main chat={chat} />
      <InspectorPane chat={chat} />
      <Overlays chat={chat} />
    </div>
  )
}

const isEntryPoint =
  typeof Bun !== "undefined"
    ? Bun.isStandaloneExecutable || Bun.main === import.meta.path
    : typeof process !== "undefined" && process.argv[1]?.endsWith("chat.tsx")

if (isEntryPoint) {
  render(() => <ChatApp turnCount={1_000} />, {
    title: "GPUIX Chat · ilha · 1,000 messages",
    width: 1180,
    height: 820,
    titlebarTransparent: true,
    windowBackground: "blurred",
    trafficLightX: 16,
    trafficLightY: 17,
    debugFrameOverlay: "full",
    focus: process.env.GPUIX_BACKGROUND !== "1",
  })
}
