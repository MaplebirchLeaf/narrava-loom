import { readFileSync, writeFileSync } from "node:fs"
import { execFileSync } from "node:child_process"
import { fileURLToPath } from "node:url"

process.chdir(fileURLToPath(new URL("../", import.meta.url)))
const cargoFiles = [
  "Cargo.toml",
  "crates/narrava-loom-protocol/Cargo.toml",
  "crates/narrava-loom-script/Cargo.toml",
  "hosts/narrava-loom-tauri/Cargo.toml",
  "hosts/narrava-loom-tui/Cargo.toml",
]
const jsonFiles = [
  "package.json",
  "bindings/typescript/package.json",
  "editors/vscode-narrava-loom/package.json",
  "hosts/narrava-loom-tauri/tauri.conf.json",
]
const pattern = /^version\s*=\s*"(\d+\.\d+\.\d+)"/m
const current = readFileSync(cargoFiles[0], "utf8").match(pattern)?.[1]
if (!current) throw new Error("Cargo.toml 缺少稳定语义化版本")
const mode = process.argv[2] ?? "--check"
if (mode === "--check") {
  for (const file of cargoFiles) {
    if (readFileSync(file, "utf8").match(pattern)?.[1] !== current)
      throw new Error(`${file} 版本应为 ${current}`)
  }
  for (const file of jsonFiles) {
    if (JSON.parse(readFileSync(file, "utf8")).version !== current)
      throw new Error(`${file} 版本应为 ${current}`)
  }
  console.log(`版本一致：${current}`)
} else {
  if (!["--sync", "patch", "minor", "major"].includes(mode))
    throw new Error("使用 --check、--sync 或 patch/minor/major")
  const parts = current.split(".").map(Number)
  if (mode !== "--sync") {
    const index = ["major", "minor", "patch"].indexOf(mode)
    parts[index] += 1
    for (let next = index + 1; next < parts.length; next += 1) parts[next] = 0
  }
  const version = parts.join(".")
  for (const file of cargoFiles) {
    writeFileSync(file, readFileSync(file, "utf8").replace(pattern, `version = "${version}"`))
  }
  for (const file of jsonFiles) {
    const data = JSON.parse(readFileSync(file, "utf8"))
    data.version = version
    writeFileSync(file, JSON.stringify(data, null, 2) + "\n")
  }
  execFileSync("cargo", ["update", "--workspace", "--offline"], {
    stdio: ["ignore", "ignore", "inherit"],
  })
  // Bun 可能保留旧 workspace 版本；只更新两个本地包的锁文件元数据。
  let lock = readFileSync("bun.lock", "utf8")
  for (const file of jsonFiles.slice(1, 3)) {
    const directory = file.replace("/package.json", "")
    const workspace = new RegExp(
      `("${directory}":\\s*\\{\\s*"name":\\s*"[^"]+",\\s*"version":\\s*)"[^"]+"`,
    )
    if (!workspace.test(lock)) throw new Error(`bun.lock 缺少 ${directory}`)
    lock = lock.replace(workspace, `$1"${version}"`)
  }
  writeFileSync("bun.lock", lock)
  execFileSync("bun", ["install", "--lockfile-only", "--ignore-scripts"], { stdio: "inherit" })
  console.log(`版本已同步：${version}`)
}
