import { afterAll, describe, expect, it } from "bun:test"
import { cpSync, mkdtempSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import solidPlugin from "../bun-plugin.js"
import { hasNativeTestRenderer } from "../testing.js"

const fixture = join(import.meta.dir, "../../fixtures/consumer")
const workspace = resolve(import.meta.dir, "../../../..")
const temp = mkdtempSync(join(tmpdir(), "gpuix-solid-consumer-"))

afterAll(() => rmSync(temp, { recursive: true, force: true }))

function run(command: string[], cwd: string, env?: Record<string, string>) {
  return Bun.spawnSync({
    cmd: command,
    cwd,
    env: { ...process.env, ...env },
    stdout: "pipe",
    stderr: "pipe",
  })
}

function nativeBindingPath(): string {
  const suffix = process.platform === "darwin"
    ? `darwin-${process.arch}`
    : process.platform === "win32"
      ? `win32-${process.arch}-msvc`
      : `linux-${process.arch}-gnu`
  return join(workspace, `packages/native/gpuix-native.${suffix}.node`)
}

describe("Solid package consumer", () => {
  // The fixture renders through createTestRoot(), which is macOS and Windows only.
  it.skipIf(!hasNativeTestRenderer)("runs imported TSX through the bunfig preload", () => {
    const result = Bun.spawnSync({
      cmd: ["bun", "main.tsx"],
      cwd: fixture,
      stdout: "pipe",
      stderr: "pipe",
    })
    expect(result.exitCode, result.stderr.toString()).toBe(0)
    expect(result.stdout.toString()).toContain("consumer 2")
  })

  it("builds TSX with the exported Bun plugin", async () => {
    const result = await Bun.build({
      entrypoints: [join(fixture, "build-entry.tsx")],
      target: "bun",
      plugins: [solidPlugin],
      write: false,
    })
    expect(result.success, result.logs.join("\n")).toBe(true)
    expect(await result.outputs[0]!.text()).toContain("built with plugin")
  })

  // The fixture renders through createTestRoot(), which is macOS and Windows only.
  it.skipIf(!hasNativeTestRenderer)("runs the preload and build plugin from installed tarballs", () => {
    const nativeTarball = "gpuix-native.tgz"
    const solidTarball = "gpuix-solid.tgz"
    for (const [packageDirectory, filename] of [
      [join(workspace, "packages/native"), nativeTarball],
      [join(workspace, "packages/solid"), solidTarball],
    ]) {
      const packed = run([
        "bun",
        "pm",
        "pack",
        "--filename",
        join(temp, filename),
        "--ignore-scripts",
        "--quiet",
      ], packageDirectory)
      expect(packed.exitCode, packed.stderr.toString()).toBe(0)
    }

    writeFileSync(join(temp, "package.json"), JSON.stringify({
      private: true,
      type: "module",
      dependencies: {
        "@gpuix/native": `file:./${nativeTarball}`,
        "@gpuix/solid": `file:./${solidTarball}`,
        "solid-js": "1.9.15",
      },
      overrides: {
        "@gpuix/native": `file:./${nativeTarball}`,
      },
    }))
    for (const filename of [
      "build-entry.tsx",
      "build.ts",
      "bunfig.toml",
      "main.tsx",
      "tsconfig.json",
      "view.tsx",
    ]) {
      cpSync(join(fixture, filename), join(temp, filename))
    }

    const installed = run(["bun", "install", "--ignore-scripts"], temp)
    expect(installed.exitCode, installed.stderr.toString()).toBe(0)
    const env = {
      NAPI_RS_NATIVE_LIBRARY_PATH: nativeBindingPath(),
    }
    const executed = run(["bun", "main.tsx"], temp, env)
    expect(executed.exitCode, executed.stderr.toString()).toBe(0)
    expect(executed.stdout.toString()).toContain("consumer 2")

    const built = run(["bun", "build.ts"], temp, env)
    expect(built.exitCode, built.stderr.toString()).toBe(0)
    expect(built.stdout.toString()).toContain("build plugin ok")
  }, 30_000)
})
