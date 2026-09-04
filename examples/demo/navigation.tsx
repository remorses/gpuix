/// View transitions, shown as the push and pop of the iOS Settings app.
///
/// The header stays mounted the whole time, and only its content takes part
/// in the transition. The back button enters and leaves through a blur and
/// opacity pair. The title of each screen carries the name "nav-title", so
/// the old text blurs and fades out in place while the new text sharpens in,
/// a text morph. The screens slide under the header as a pair, and a
/// backdrop blur with an eased mask blurs the rows progressively where they
/// pass under it.

import React, { useState } from "react"
import { startViewTransition, useGpuix } from "@gpuix/react"
import type { NativeRenderer, ViewTransitionOptions } from "@gpuix/react"
import { Panel } from "./ui.js"

const HEADER_HEIGHT = 56
/// How far past the bar the backdrop blur fades out.
const BLUR_TAIL = 28
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
      className={["row items-center px-4 py-3", onClick ? "pointer hover:bg-raised" : ""].join(" ")}
      style={{
        flexShrink: 0,
        justifyContent: "space-between",
        borderBottomWidth: 1,
        borderColor: "var(--color-line)",
      }}
      onClick={onClick}
    >
      <text className="text-sm text-fg">{label}</text>
      <text className="text-sm text-faint">{detail ?? (onClick ? ">" : "")}</text>
    </div>
  )
}

/// The header that never unmounts. The first layer is the progressive blur:
/// a backdrop blur whose eased mask fades it out past the bar, so the rows
/// blur where they pass under it. The title and the back button sit on top
/// of that layer, and each carries its own view transition name.
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
          height: HEADER_HEIGHT + BLUR_TAIL,
          backdropFilter: "blur(16px)",
          maskImage: "linear-gradient(to bottom, black 50%, ease-in-out, transparent)",
          pointerEvents: "none",
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
          pointerEvents: "none",
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
function Screen({ children }: { children: React.ReactNode }) {
  return (
    <div
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
  const go = (next: "root" | "general", options: ViewTransitionOptions) => {
    if (renderer) {
      startViewTransition(renderer, () => setScreen(next), options)
    } else {
      setScreen(next)
    }
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
        <Screen key="root">
          {ROOT_ROWS.map((label) => (
            <NavRow
              key={label}
              label={label}
              onClick={label === "General" ? () => go("general", PUSH) : undefined}
            />
          ))}
        </Screen>
      ) : (
        <Screen key="general">
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
      note="Click General to push its screen. The screens slide as a pair under a header that never unmounts. The back button enters through a blur and opacity pair and leaves the same way, and the title morphs between Settings and General. The strip under the header is a backdrop blur with an eased mask, so the rows blur progressively as they scroll under it."
    >
      <Phone renderer={renderer ?? null} />
    </Panel>
  )
}
