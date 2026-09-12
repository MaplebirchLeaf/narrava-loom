// 补全只查询成员路径。请求序号避免旧响应覆盖新输入，不预执行任何表达式。
export function createConsoleCompletion(input, list, help, invoke) {
  let version = 0
  let timer
  let items = []
  let selected = 0
  let start = 0
  let end = 0
  function hide() {
    list.hidden = true
    items = []
    input.setAttribute("aria-expanded", "false")
    input.removeAttribute("aria-activedescendant")
  }
  function showHelp(item) {
    help.textContent = item
      ? [item.name, item.signature, item.help].filter(Boolean).join(" · ")
      : "输入对象名查看成员；Tab 补全，Enter 执行 / Type an object; Tab completes, Enter executes"
  }
  function select(index) {
    selected = index
    for (const [i, row] of [...list.children].entries())
      row.setAttribute("aria-selected", String(i === index))
    input.setAttribute("aria-activedescendant", `debug-candidate-${index}`)
    list.children[index]?.scrollIntoView({ block: "nearest" })
    showHelp(items[index])
  }
  function accept() {
    const item = items[selected]
    if (!item) return
    const suffix = item.kind === "function" && input.value[end] !== "(" ? "(" : ""
    input.value = input.value.slice(0, start) + item.name + suffix + input.value.slice(end)
    input.setSelectionRange(
      start + item.name.length + suffix.length,
      start + item.name.length + suffix.length,
    )
    hide()
    input.focus()
    showHelp(item)
    if (suffix) void update()
  }
  async function update() {
    const request = ++version
    hide()
    if (input.readOnly) return
    const text = input.value.slice(0, input.selectionStart ?? input.value.length)
    const call = /([A-Za-z_$][\w$]*(?:\.[\w$]+)*)\([^()]*$/.exec(text)
    const member = /([A-Za-z_$][\w$]*(?:\.[\w$]*)*)$/.exec(text)
    if (!call && !member && text.trim()) {
      showHelp(null)
      return
    }
    const target = call?.[1] ?? member?.[1] ?? ""
    const dot = target.lastIndexOf(".")
    const path = dot < 0 ? "" : target.slice(0, dot)
    const prefix = target.slice(dot + 1)
    try {
      const members = await invoke("debug_complete", { path })
      if (request !== version || input.readOnly) return
      if (call) {
        showHelp(members.find((item) => item.name === prefix))
        return
      }
      items = members
        .filter((item) => item.name.toLowerCase().startsWith(prefix.toLowerCase()))
        .toSorted(
          (a, b) =>
            Number(Boolean(b.signature)) - Number(Boolean(a.signature)) ||
            a.name.localeCompare(b.name),
        )
        .slice(0, 12)
      start = text.length - prefix.length
      end = text.length
      list.replaceChildren()
      if (!items.length) {
        help.textContent = "无匹配成员，请检查名称 / No matching member"
        return
      }
      for (const [index, item] of items.entries()) {
        const row = list.ownerDocument.createElement("div")
        row.id = `debug-candidate-${index}`
        row.setAttribute("role", "option")
        row.textContent = `${item.name}${item.kind === "function" ? "(…)" : ""}`
        row.addEventListener("mousedown", (event) => event.preventDefault())
        row.addEventListener("click", () => {
          selected = index
          accept()
        })
        list.append(row)
      }
      list.hidden = false
      input.setAttribute("aria-expanded", "true")
      select(0)
    } catch {
      if (request === version) {
        hide()
        help.textContent = "Runtime 忙碌或不可用 / Runtime busy or unavailable"
      }
    }
  }
  input.addEventListener("input", () => {
    ++version
    hide()
    clearTimeout(timer)
    timer = setTimeout(update, 100)
  })
  input.addEventListener("blur", () => {
    ++version
    hide()
  })
  return {
    update,
    hide: () => {
      ++version
      clearTimeout(timer)
      hide()
      showHelp(null)
    },
    key(event) {
      if (event.isComposing || list.hidden) return false
      if (event.key === "ArrowUp" || event.key === "ArrowDown") {
        event.preventDefault()
        select((selected + items.length + (event.key === "ArrowUp" ? -1 : 1)) % items.length)
        return true
      }
      if (event.key === "Enter" && input.value.slice(start, end) === items[selected]?.name) {
        hide()
        return false
      }
      if (event.key === "Tab" || event.key === "Enter") {
        event.preventDefault()
        accept()
        return true
      }
      if (event.key === "Escape") {
        event.preventDefault()
        event.stopPropagation()
        hide()
        return true
      }
      return false
    },
  }
}
