import { execFileSync } from "node:child_process"
import { existsSync, readFileSync, statSync } from "node:fs"
import { dirname, relative, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const root = fileURLToPath(new URL("../", import.meta.url))
const files = execFileSync(
  "git",
  ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
  {
    cwd: root,
    encoding: "utf8",
  },
)
  .split("\0")
  .filter((name) => name.endsWith(".md") && existsSync(resolve(root, name)))
const pages = new Map()
const errors = []
let checked = 0

for (const name of new Set(files)) {
  const source = readFileSync(resolve(root, name), "utf8")
  const counts = new Map()
  const anchors = new Set()
  let fence
  const lines = source.split("\n").filter((line) => {
    const marker = line.match(/^\s*(`{3,}|~{3,})/)
    if (marker) {
      if (!fence) fence = marker[1]
      else if (marker[1][0] === fence[0] && marker[1].length >= fence.length) fence = undefined
      return false
    }
    return !fence
  })
  if (fence) errors.push(`${name}: 未闭合的代码块`)
  for (const line of lines) {
    const heading = line.match(/^#{1,6}\s+(.+?)(?:\s+#+)?$/)?.[1]
    if (!heading) continue
    const slug = heading
      .replace(/<[^>]*>/g, "")
      .toLowerCase()
      .replace(/[^\p{L}\p{N}\p{M}_\- ]/gu, "")
      .replace(/ /g, "-")
    const count = counts.get(slug) ?? 0
    counts.set(slug, count + 1)
    anchors.add(count ? `${slug}-${count}` : slug)
  }
  const links = [...lines.join("\n").matchAll(/\[[^\]\n]*\]\((?:<([^>]+)>|([^\s)]+))\)/g)].map(
    (match) => match[1] ?? match[2],
  )
  pages.set(name, { anchors, links, targets: [] })
}

for (const [name, page] of pages) {
  for (const link of page.links) {
    if (/^[a-z][a-z\d+.-]*:/i.test(link) || link.startsWith("//")) continue
    const [path, anchor] = link.split("#")
    const target = path
      ? resolve(
          path.startsWith("/") ? root : dirname(resolve(root, name)),
          decodeURIComponent(path.replace(/^\//, "")),
        )
      : resolve(root, name)
    const targetName = relative(root, target)
    checked += 1
    if (!existsSync(target)) {
      errors.push(`${name}: 文件不存在 ${link}`)
      continue
    }
    if (pages.has(targetName)) {
      page.targets.push(targetName)
      if (anchor && !pages.get(targetName).anchors.has(decodeURIComponent(anchor)))
        errors.push(`${name}: 标题不存在 ${link}`)
    } else if (anchor && statSync(target).isDirectory()) {
      errors.push(`${name}: 目录没有标题锚点 ${link}`)
    }
  }
}

const visited = new Set()
const queue = ["docs/README.md"]
while (queue.length) {
  const name = queue.pop()
  if (visited.has(name)) continue
  visited.add(name)
  queue.push(...(pages.get(name)?.targets ?? []))
}
for (const name of pages.keys()) {
  if (name.startsWith("docs/") && !visited.has(name)) errors.push(`${name}: 无法从文档目录到达`)
}
if (errors.length) throw new Error(`文档检查失败：\n${errors.join("\n")}`)
console.log(`文档检查通过：${pages.size} 个 Markdown 文件，${checked} 个本地链接；docs 页面均可达`)
