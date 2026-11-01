import { afterAll, describe, expect, it } from "vitest"
import { mkdtempSync, rmSync, writeFileSync } from "node:fs"
import { spawnSync } from "node:child_process"
import { fileURLToPath } from "node:url"
import { tmpdir } from "node:os"
import { join } from "node:path"

const temp = mkdtempSync(join(tmpdir(), "gpuix-native-package-"))
afterAll(() => rmSync(temp, { recursive: true, force: true }))

function run(command: string[], cwd = temp) {
  return spawnSync(command[0]!, command.slice(1), { cwd, encoding: "utf8" })
}

describe("native ESM package", () => {
  it("loads the generated addon through ESM", () => {
    const result = run([
      "node",
      "--input-type=module",
      "-e",
      `import(${JSON.stringify(new URL("../../index.js", import.meta.url).href)}).then((native) => console.log(typeof native.GpuixRenderer))`,
    ])
    expect(result.status, result.stderr).toBe(0)
    expect(result.stdout.trim()).toBe("function")

    const commonJs = run([
      "node",
      "-e",
      `console.log(typeof require(${JSON.stringify(fileURLToPath(new URL("../../index.cjs", import.meta.url)))}).GpuixRenderer)`,
    ])
    expect(commonJs.status, commonJs.stderr).toBe(0)
    expect(commonJs.stdout.trim()).toBe("function")
  })

  // npm pack + bun add copy the whole package, including the addon binary and
  // the wasm build. On a cold CI runner that alone can pass the 5s default.
  it("resolves shared subpaths from an installed tarball without an addon", { timeout: 30_000 }, () => {
    const packageRoot = join(fileURLToPath(new URL(".", import.meta.url)), "../..")
    const packed = run([
      "npm",
      "pack",
      "--json",
      "--ignore-scripts",
      "--pack-destination",
      temp,
    ], packageRoot)
    expect(packed.status, packed.stderr).toBe(0)
    const [{ filename }] = JSON.parse(packed.stdout) as Array<{ filename: string }>
    writeFileSync(join(temp, "package.json"), '{"type":"module","private":true}\n')
    const install = run(["bun", "add", join(temp, filename)])
    expect(install.status, install.stderr).toBe(0)

    const imported = run([
      "node",
      "--input-type=module",
      "-e",
      "Promise.all([import('@gpuix/native/host'), import('@gpuix/native/automation')]).then(([host, automation]) => console.log(typeof host.createMutationQueue, typeof automation.connectTest))",
    ])
    expect(imported.status, imported.stderr).toBe(0)
    expect(imported.stdout.trim()).toBe("function function")

    const browser = run([
      "node",
      "--conditions=browser",
      "--input-type=module",
      "-e",
      "console.log(import.meta.resolve('@gpuix/native'))",
    ])
    expect(browser.status, browser.stderr).toBe(0)
    expect(browser.stdout).toContain("browser.mjs")
  })

  it("bundles both public adapters for the browser", () => {
    for (const adapter of ["react", "solid"]) {
      const entry = join(temp, `${adapter}-browser.mjs`)
      writeFileSync(
        entry,
        `export { render } from ${JSON.stringify(fileURLToPath(new URL(`../../../${adapter}/dist/index.js`, import.meta.url)))}\n`
      )
      const output = join(temp, `${adapter}-browser`)
      const bundled = run([
        "bun",
        "build",
        entry,
        "--target",
        "browser",
        "--outdir",
        output,
      ])
      expect(bundled.status, bundled.stderr).toBe(0)
    }
  })
})
