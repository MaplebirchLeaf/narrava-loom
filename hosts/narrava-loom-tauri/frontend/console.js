import { renderConsoleValue } from "./console-view.js"
import { createConsoleCompletion } from "./console-completion.js"
// 脚本通过主界面的串行 IPC 队列进入游戏 Runtime；这里只呈现结果快照，不执行游戏脚本。
const invoke = window.__TAURI__?.core?.invoke
const opener = document.getElementById("debug-open")
const panel = document.getElementById("debug-console")
const status = document.getElementById("debug-status")
const content = document.getElementById("debug-content")
const input = document.getElementById("debug-input")
const cancelButton = document.getElementById("debug-cancel")
const gameDialog = document.getElementById("nv-dialog")
const home = panel.parentElement
const history = []
let cursor = 0
let draft = ""
let sequence = 0
let busy = false
let previousFocus = null
let resultSequence = 0
const completion = createConsoleCompletion(
  input,
  document.getElementById("debug-candidates"),
  document.getElementById("debug-help"),
  invoke,
)
function usePath(path) {
  input.value = path
  input.focus()
  input.setSelectionRange(path.length, path.length)
  void completion.update()
}

function diagnosticLabel(error) {
  const location = error?.location
  const code = error?.code ?? "console.error"
  if (!location) return code
  const coordinates = [location.source, location.line, location.column]
    .filter((part) => part !== null && part !== undefined)
    .join(":")
  return `${code} · ${coordinates}${location.generated ? "（转译后 / Generated）" : ""}`
}

function appendLine(message, level = "info") {
  const row = document.createElement("div")
  row.className = `debug-line debug-${level}`
  row.textContent = message
  content.append(row)
  while (content.children.length > 500) content.firstElementChild.remove()
  content.scrollTop = content.scrollHeight
}

async function refresh() {
  try {
    const snapshot = await invoke("debug_snapshot")
    status.textContent = snapshot.current ?? "尚未开始 / Not started"
    for (const record of snapshot.logs) {
      if (record.sequence <= sequence) continue
      const label = `[${record.level} ${record.target}]`
      const source = record.diagnostic ? ` (${diagnosticLabel(record.diagnostic)})` : ""
      appendLine(`${label} ${record.message}${source}`, record.level)
      sequence = record.sequence
    }
    if (snapshot.evaluation && snapshot.evaluation.sequence > resultSequence) {
      resultSequence = snapshot.evaluation.sequence
      content.append(renderConsoleValue(document, snapshot.evaluation.value, usePath))
      while (content.children.length > 500) content.firstElementChild.remove()
      content.scrollTop = content.scrollHeight
    }
    return snapshot.logs
  } catch (error) {
    appendLine(`${diagnosticLabel(error)}: ${error?.message ?? String(error)}`, "error")
    return null
  }
}

// 原生模态框会令外部节点 inert；控制台挂入当前模态框后仍能接收输入。
function mountConsole() {
  const parent = gameDialog?.open && !panel.hidden ? gameDialog : home
  if (parent && panel.parentElement !== parent) parent.append(panel)
}

function setOpen(open) {
  if (opener.hidden) return
  panel.hidden = !open
  mountConsole()
  opener.setAttribute("aria-expanded", String(open))
  if (open) {
    previousFocus = document.activeElement
    input.focus()
    void refresh()
  } else {
    completion.hide()
    previousFocus?.focus()
  }
}

function clear() {
  // 仅清屏；保留 Runtime 日志，避免影响诊断或其他日志订阅者。
  content.replaceChildren()
}

async function execute() {
  const source = input.value.trim()
  if (!source || busy) return
  completion.hide()
  busy = true
  cancelButton.disabled = false
  input.readOnly = true
  input.setAttribute("aria-busy", "true")
  if (history.at(-1) !== source) history.push(source)
  if (history.length > 100) history.shift()
  cursor = history.length
  draft = ""
  input.value = ""
  appendLine(`› ${source}`, "command")
  let failure = null
  try {
    await window.narravaDebug.execute(source)
  } catch (error) {
    failure = error
  } finally {
    const records = await refresh()
    // Runtime 错误已在统一日志中；只有 IPC/渲染错误需要前端补一行。
    if (
      failure &&
      records &&
      !records.some(
        (record) =>
          record.diagnostic?.code === failure.code &&
          record.diagnostic?.message === failure.message,
      )
    ) {
      appendLine(`${diagnosticLabel(failure)}: ${failure?.message ?? String(failure)}`, "error")
    }
    busy = false
    cancelButton.disabled = true
    input.readOnly = false
    input.setAttribute("aria-busy", "false")
    if (!panel.hidden) input.focus()
  }
}

input.addEventListener("keydown", (event) => {
  if (event.isComposing || completion.key(event)) return
  if (event.key === "Enter") {
    event.preventDefault()
    void execute()
  } else if (!busy && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
    event.preventDefault()
    if (cursor === history.length) draft = input.value
    cursor = Math.max(0, Math.min(history.length, cursor + (event.key === "ArrowUp" ? -1 : 1)))
    input.value = cursor === history.length ? draft : history[cursor]
    completion.hide()
    input.setSelectionRange(input.value.length, input.value.length)
  }
})
cancelButton.addEventListener("click", () => {
  void invoke("debug_cancel").catch((error) => appendLine(error.message ?? String(error), "error"))
})
opener.addEventListener("click", () => setOpen(panel.hidden))
document.getElementById("debug-close").addEventListener("click", () => setOpen(false))
document.getElementById("debug-clear").addEventListener("click", clear)
document.getElementById("debug-refresh").addEventListener("click", refresh)
document.addEventListener("keydown", (event) => {
  if (event.key === "F10" && !opener.hidden) {
    event.preventDefault()
    setOpen(panel.hidden)
  } else if (!panel.hidden && event.key === "Escape") {
    event.preventDefault()
    setOpen(false)
  } else if (!panel.hidden && event.ctrlKey && event.key.toLowerCase() === "l") {
    event.preventDefault()
    clear()
  }
})
// 打开期间跟随游戏命令更新日志；没有轮询，也不会抢占游戏输入。
window.addEventListener("narrava:updated", () => {
  mountConsole()
  if (!panel.hidden && !busy) void refresh()
})
document.getElementById("debug-examples").addEventListener("click", (event) => {
  const source = event.target.closest("[data-command]")?.dataset.command
  if (source) usePath(source)
})
if (gameDialog) {
  new MutationObserver(mountConsole).observe(gameDialog, {
    attributes: true,
    attributeFilter: ["open"],
  })
}
if (invoke) {
  invoke("developer_enabled")
    .then((enabled) => {
      opener.hidden = !enabled
    })
    .catch(() => {
      opener.hidden = true
    })
}
