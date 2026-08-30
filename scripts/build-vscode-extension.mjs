import { mkdir } from "node:fs/promises"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

const repository = dirname(dirname(fileURLToPath(import.meta.url)))
const extensionDirectory = join(repository, "editors", "vscode-narrava-loom")
const outputDirectory = join(repository, "dist", "vscode-narrava-loom")
const metadata = await Bun.file(join(extensionDirectory, "package.json")).json()
const output = join(outputDirectory, `${metadata.name}-${metadata.version}.vsix`)

let install = false
let editor = Bun.env.NARRAVA_VSCODE_COMMAND ?? "code"
const args = Bun.argv.slice(2)

for (let index = 0; index < args.length; index += 1) {
  switch (args[index]) {
    case "--install":
      install = true
      break
    case "--editor":
      editor = args[index + 1]
      if (!editor) fail("--editor 缺少命令")
      index += 1
      break
    case "-h":
    case "--help":
      usage()
      process.exit(0)
    default:
      fail(`未知选项：${args[index]}`)
  }
}

console.log("运行扩展回归测试……")
await run([process.execPath, "run", "test:vscode"], repository)
await mkdir(outputDirectory, { recursive: true })

console.log(`构建 ${output} ……`)
await run(
  [
    process.execPath,
    "x",
    "vsce",
    "package",
    "--no-dependencies",
    "--skip-license",
    "--allow-missing-repository",
    "--out",
    output,
  ],
  extensionDirectory,
)

if (install) {
  console.log(`安装 ${output} ……`)
  await run([editor, "--install-extension", output, "--force"], repository)
}

console.log(`完成：${output}`)

async function run(command, cwd) {
  const child = Bun.spawn(command, {
    cwd,
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
  })
  const exitCode = await child.exited
  if (exitCode !== 0) process.exit(exitCode)
}

function fail(message) {
  console.error(message)
  usage()
  process.exit(2)
}

function usage() {
  console.log(`用法：bun run vsix [选项]

选项：
  --install          构建后安装刚生成的 .vsix
  --editor COMMAND   指定安装命令，例如 code、codium（默认：code）
  -h, --help         显示帮助

也可用 NARRAVA_VSCODE_COMMAND 设置默认编辑器命令。`)
}
