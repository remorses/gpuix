/// View transitions, shown as the push and pop of the iOS Settings app.
///
/// The header stays mounted the whole time, and only its content takes part
/// in the transition. The back button enters and leaves through a blur and
/// opacity pair. The title of each screen carries the name "nav-title", so
/// the old text blurs and fades out in place while the new text sharpens in,
/// a text morph. The screens slide under the header as a pair, under the
/// soft scroll edge effect of iOS 26: a variable backdrop blur with a
/// saturation lift on the same mask, under a scrim in the panel colour.

import React, { useLayoutEffect, useRef, useState } from "react"
import { startViewTransition, useGpuix } from "@gpuix/react"
import type { NativeRenderer, ViewTransitionOptions } from "@gpuix/react"
import { Panel } from "./ui.js"

const HEADER_HEIGHT = 56
/// A spring without bounce: fast out of the gate, a long soft landing,
/// and no overshoot, like a critically damped UIKit spring.
const SPRING: [number, number, number, number] = [0.36, 0.66, 0.04, 1]

const PUSH: ViewTransitionOptions = {
  duration: 0.45,
  ease: SPRING,
  groups: {
    screen: {
      old: { translateX: ["0%", "-30%"] },
      new: { translateX: ["100%", "0%"] },
    },
    "nav-back": { new: { opacity: [0, 1], blur: [6, 0] } },
    "nav-title": {
      old: { opacity: [1, 0], blur: [0, 4] },
      new: { opacity: [0, 1], blur: [4, 0] },
    },
  },
}

const POP: ViewTransitionOptions = {
  duration: 0.45,
  ease: SPRING,
  groups: {
    screen: {
      old: { translateX: ["0%", "100%"], onTop: true },
      new: { translateX: ["-30%", "0%"] },
    },
    "nav-back": { old: { opacity: [1, 0], blur: [0, 6] } },
    "nav-title": {
      old: { opacity: [1, 0], blur: [0, 4] },
      new: { opacity: [0, 1], blur: [4, 0] },
    },
  },
}

const ROOT_ROWS = [
  "General", "Display", "Sound", "Focus", "Battery",
  "Privacy", "Wallpaper", "Siri", "Wallet", "Accounts",
  "App Store", "Game Center", "Developer",
]
const GENERAL_ROWS = [
  "About", "Software Update", "Storage", "AppleCare", "AirDrop",
  "AirPlay", "Picture in Picture", "CarPlay", "Keyboard", "Fonts",
  "Language", "Dictionary", "VPN", "Legal",
]

function NavRow({ label, detail, onClick }: {
  label: string
  detail?: string
  onClick?: () => void
}) {
  return (
    <div
      testId={`nav-row-${label}`}
      // The border comes from a class, because the style prop beats a class
      // in every state and `last:` could not take it back.
      className={[
        "row items-center px-4 py-3 border-b last:border-b-0",
        onClick ? "pointer hover:bg-raised" : "",
      ].join(" ")}
      style={{
        flexShrink: 0,
        justifyContent: "space-between",
      }}
      onClick={onClick}
    >
      <text className="text-sm text-fg">{label}</text>
      <text className="text-sm text-faint">{detail ?? (onClick ? ">" : "")}</text>
    </div>
  )
}

/// The header that never unmounts, built as the soft scroll edge effect
/// of iOS 26. Two layers make the effect. The first is a variable backdrop
/// blur with a saturation and contrast lift, and the colour matrix rides
/// the same mask as the blur. The mask holds full width over the top of
/// the bar and then falls off on a log scale, to zero at the bottom edge
/// of the bar, as iOS does. A row at rest below the bar stays sharp, and
/// blurs only where it passes under. The second is a gradient scrim. iOS
/// reads the scrim
/// colour from the content under the bar, which the engine cannot do, so
/// the scrim takes the colour of the panel, the surface the rows scroll
/// on. The title and the back button sit on top, and each carries its own
/// view transition name.
///
/// The bar blocks clicks, as the iOS bar does: a row scrolled under it is
/// not clickable. The layers are absolute, so the engine blocks by
/// default, and the wheel still passes to the list beneath.
function Header({ screen, onBack }: {
  screen: "root" | "general"
  onBack: () => void
}) {
  const title = screen === "root" ? "Settings" : "General"
  return (
    <>
      <div
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          height: HEADER_HEIGHT,
          backdropFilter: "blur(16px) saturate(1.8) contrast(1.05)",
          // The eye reads blur on a log scale. Sigma 4 and sigma 16 both
          // look like mush on small text, so a linear fade of the sigma
          // spends most of the strip on the same look. The stops after
          // the hold halve the sigma at even distances.
          maskImage:
            "linear-gradient(to bottom, black 40%, rgba(0,0,0,0.5) 55%, rgba(0,0,0,0.25) 68%, rgba(0,0,0,0.125) 79%, rgba(0,0,0,0.06) 89%, transparent)",
        }}
      />
      <div
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          height: HEADER_HEIGHT,
          backgroundImage:
            "linear-gradient(to bottom, color-mix(in srgb, var(--color-panel) 72%, transparent), ease-in-out, transparent)",
        }}
      />
      <div
        className="row items-center"
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          height: HEADER_HEIGHT,
          justifyContent: "center",
        }}
      >
        <div key={screen} testId={`nav-title-${screen}`} style={{ viewTransitionName: "nav-title" }}>
          <text className="text-sm font-semibold text-fg">{title}</text>
        </div>
      </div>
      <div
        className="row items-center px-2"
        style={{ position: "absolute", top: 0, left: 0, height: HEADER_HEIGHT }}
      >
        {screen === "general" ? (
          <div
            testId="nav-back"
            className="row pointer select-none px-1"
            style={{ viewTransitionName: "nav-back" }}
            onClick={onBack}
          >
            <text className="text-sm" style={{ color: "var(--color-brand)" }}>{"< Settings"}</text>
          </div>
        ) : null}
      </div>
    </>
  )
}

/// One screen of the stack. The name pairs it with the screen it replaces,
/// and the key makes React mount a new element instead of an update in
/// place, the way a real navigation swaps components. The top padding puts
/// the first row under the header, and the rows scroll under it.
///
/// iOS keeps the scroll position of a screen you go back to. The engine
/// does not: a remounted screen starts at the top, as a keyed remount
/// does on the web. The app restores it. The effect cleanup saves the
/// offset when the screen unmounts, and the mount effect sets it back.
/// A scrollTo this early is safe. When no frame has painted the screen
/// yet, the engine holds the offset for the frame that creates its
/// scroll state.
function Screen({ id, renderer, offsets, children }: {
  id: string
  renderer: NativeRenderer | null
  offsets: React.RefObject<Map<string, [number, number]>>
  children: React.ReactNode
}) {
  const box = useRef<{ id: number } | null>(null)
  useLayoutEffect(() => {
    const el = box.current
    if (!el || !renderer?.scrollTo || !renderer.getScrollOffset) return
    const saved = offsets.current.get(id)
    if (saved) renderer.scrollTo(el.id, saved[0], saved[1], "instant")
    return () => {
      const offset = renderer.getScrollOffset!(el.id)
      if (offset) offsets.current.set(id, [offset[0]!, offset[1]!])
    }
  }, [])
  return (
    <div
      ref={box}
      className="col w-full h-full"
      style={{
        viewTransitionName: "screen",
        backgroundColor: "var(--color-panel)",
        overflowY: "scroll",
        paddingTop: HEADER_HEIGHT,
      }}
    >
      {children}
    </div>
  )
}

function Phone({ renderer }: { renderer: NativeRenderer | null }) {
  const [screen, setScreen] = useState<"root" | "general">("root")
  /// The saved scroll offset of each screen, by its key.
  const offsets = useRef(new Map<string, [number, number]>())
  const go = (next: "root" | "general", options: ViewTransitionOptions) => {
    const leaving = screen
    if (renderer) {
      startViewTransition(renderer, () => setScreen(next), options)
    } else {
      setScreen(next)
    }
    // iOS restores the scroll position of a screen you go back to, and
    // only that one. A pop throws its screen away, so a later push of
    // the same screen starts at the top.
    if (options === POP) offsets.current.delete(leaving)
  }

  return (
    <div
      className="col rounded border"
      style={{
        width: 320,
        height: 440,
        flexShrink: 0,
        overflow: "hidden",
        position: "relative",
      }}
    >
      {screen === "root" ? (
        <Screen key="root" id="root" renderer={renderer} offsets={offsets}>
          {ROOT_ROWS.map((label) => (
            <NavRow
              key={label}
              label={label}
              onClick={label === "General" ? () => go("general", PUSH) : undefined}
            />
          ))}
        </Screen>
      ) : (
        <Screen key="general" id="general" renderer={renderer} offsets={offsets}>
          {GENERAL_ROWS.map((label) => (
            <NavRow key={label} label={label} detail="" />
          ))}
        </Screen>
      )}
      <Header screen={screen} onBack={() => go("root", POP)} />
    </div>
  )
}

export function Navigation() {
  const { renderer } = useGpuix()
  return (
    <Panel
      title="View transitions"
      note="Click General to push its screen. The screens slide as a pair under a header that never unmounts. The back button enters through a blur and opacity pair and leaves the same way, and the title morphs between Settings and General. The strip under the header is the soft scroll edge effect of iOS 26: a variable backdrop blur plus a saturation lift on one mask, under a scrim in the panel colour. Scroll the list, push, and go back: the app saves the offset in JS and restores it, the way iOS does."
    >
      <Phone renderer={renderer ?? null} />
    </Panel>
  )
}
