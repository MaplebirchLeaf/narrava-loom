import assert from "node:assert/strict"
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join, normalize } from "node:path"
import { fileURLToPath } from "node:url"
import { API } from "typescript/unstable/sync"

const root = fileURLToPath(new URL("../", import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), "narrava-author-types-"))
const api = new API({ cwd: root })
try {
  // 模拟编辑器中另一个推断项目加载的测试全局声明。
  const foreign = join(temporary, "mocha.d.ts")
  await writeFile(
    foreign,
    "declare namespace Mocha { interface HookFunction<T = void> { (fn: () => T): void } }\ndeclare var setup: Mocha.HookFunction;\n",
  )
  const script = join(root, "examples/contents/scripts/main.ts")
  const snapshot = api.updateSnapshot({ openFiles: [foreign, script] })
  const project = snapshot.getDefaultProjectForFile(script)
  const source = await readFile(script, "utf8")
  const setupType = project.checker.getTypeAtPosition(script, source.indexOf("setup.build"))
  assert.match(project.checker.typeToString(setupType), /NarravaData/)
  assert.equal(normalize(project.configFileName), join(root, "examples/tsconfig.json"))
  assert.equal(project.program.getSourceFileNames().map(normalize).includes(foreign), false)
  assert.equal(
    project.program.getSourceFileNames().some((file) => file.endsWith("lib.dom.d.ts")),
    false,
  )
  assert.deepEqual(project.program.getSemanticDiagnostics(), [])
  snapshot.dispose()

  // 独立游戏仅使用类型包声明的发行文件，不能依赖仓库测试配置补全缺失类型。
  const game = join(temporary, "game")
  const contents = join(game, "contents/scripts")
  const installed = join(game, "node_modules/@narrava-loom/types")
  await Promise.all([mkdir(contents, { recursive: true }), mkdir(installed, { recursive: true })])
  const packageRoot = join(root, "bindings/typescript")
  const manifest = JSON.parse(await readFile(join(packageRoot, "package.json"), "utf8"))
  await Promise.all(
    ["package.json", ...manifest.files].map((file) =>
      copyFile(join(packageRoot, file), join(installed, file)),
    ),
  )
  await copyFile(join(root, "examples/tsconfig.json"), join(game, "tsconfig.json"))
  const standalone = join(contents, "main.ts")
  await writeFile(
    standalone,
    'setup.build = "standalone"\nEvent.emit("game:ready", { build: setup.build })\n',
  )
  const javascript = join(contents, "helpers.js")
  await writeFile(javascript, 'setup.language = "en"\n')
  const isolated = api.updateSnapshot({ openFiles: [standalone, javascript] })
  const gameProject = isolated.getDefaultProjectForFile(standalone)
  assert.equal(normalize(gameProject.configFileName), join(game, "tsconfig.json"))
  assert.equal(isolated.getDefaultProjectForFile(javascript), gameProject)
  const javascriptSetup = gameProject.checker.getTypeAtPosition(javascript, 0)
  assert.match(gameProject.checker.typeToString(javascriptSetup), /NarravaData/)
  assert.deepEqual(gameProject.program.getSemanticDiagnostics(), [])
  isolated.dispose()
  console.log(
    "Narrava author projects isolate setup/Event and load the complete standalone types package",
  )
} finally {
  api.close()
  await rm(temporary, { recursive: true, force: true })
}
