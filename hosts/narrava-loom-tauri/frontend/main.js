// Narrava Tauri WebView Renderer。
// 本文件只负责把受验证 DTO 映射为 DOM，并把不透明交互 ID 回送 Rust Worker；
// 游戏脚本、State 与 Story 执行永远不进入 window。
const invoke = window.__TAURI__.core.invoke

// 稳定 DOM 插槽是 Host 的公开主题边界，作者 CSS 可以依赖这些根节点。
const story = document.querySelector("nv-story")
const passageRoot = document.querySelector("nv-passage")
const passage = document.querySelector("#passage-main")
const passageHeader = document.querySelector(".passage-header")
const passageFooter = document.querySelector("#passage-footer-surface")
const bar = document.querySelector("nv-ui-bar")
const barToggle = document.querySelector("#ui-bar-toggle")
const barSurface = document.querySelector("#bar-surface")
const status = document.querySelector("#status")
const dialog = document.querySelector("#nv-dialog")
const dialogTabs = document.querySelector("#dialog-tabs")
const dialogMessage = document.querySelector("#dialog-message")
const dialogSurface = document.querySelector("#dialog-surface")
// 可变运行时状态：作者样式 Blob URL、Resource 路径集合、最近一次更新与侧栏区域缓存。
const objectUrls = new Set()
let resourcePaths = new Set()
let barRegions = { expanded: [], stowed: [] }

/** 同步侧栏视觉状态与无障碍状态；不改变 Core Story。 */
function setBarStowed(stowed) {
  bar.classList.toggle("stowed", stowed)
  story.classList.toggle("bar-stowed", stowed)
  barToggle.setAttribute("aria-expanded", String(!stowed))
  barToggle.setAttribute("aria-label", stowed ? "展开侧栏" : "收起侧栏")
  barToggle.title = stowed ? "展开侧栏" : "收起侧栏"
  reconcile(barSurface, stowed ? barRegions.stowed : barRegions.expanded)
}

barToggle.addEventListener("click", () => setBarStowed(!bar.classList.contains("stowed")))
if (window.matchMedia("(max-width: 39.5em)").matches) setBarStowed(true)

/** 开发者模式只注册 F12 开关 WebView DevTools；调试 State 请使用游戏内表现或未来控制台能力。 */
function configureDeveloperMode(enabled) {
  if (!enabled) return
  window.addEventListener("keydown", (event) => {
    if (event.key !== "F12") return
    event.preventDefault()
    void invoke("toggle_devtools").catch(showError)
  })
  console.info("Narrava 开发者模式已开启。按 F12 开关 WebView DevTools。")
}

/** 按 key 协调 Surface DTO，保留仍存在的控件与焦点。 */
function render(update) {
  const focusedKey = document.activeElement?.closest?.("[data-surface-key]")?.dataset.surfaceKey
  passageRoot.dataset.passage = update.current
  passageRoot.setAttribute("aria-label", update.current)

  const regions = new Map()
  const main = []
  for (const node of update.nodes) {
    if (node.type === "region") regions.set(node.region, node.nodes)
    else main.push(node)
  }
  const standardRegions = new Set(["header", "main", "footer", "bar", "bar-stowed", "dialog"])
  const customRegionNodes = [...regions]
    .filter(([region]) => !standardRegions.has(region))
    .flatMap(([, nodes]) => nodes)
  reconcile(passageHeader, regions.get("header") ?? [])
  // 自定义 Region 没有专用 DOM 插槽时回退到正文，内容不可静默丢失。
  reconcile(passage, [...(regions.get("main") ?? []), ...main, ...customRegionNodes])
  reconcile(passageFooter, regions.get("footer") ?? [])
  barRegions = {
    expanded: regions.get("bar") ?? [],
    stowed: regions.get("bar-stowed") ?? [],
  }
  reconcile(barSurface, bar.classList.contains("stowed") ? barRegions.stowed : barRegions.expanded)
  resetDialogTabs()
  reconcile(dialogSurface, regions.get("dialog") ?? [])
  applyReplacements()
  if (dialogSurface.childElementCount > 0) {
    const panels = buildDialogTabs()
    selectDialogTab(0, panels)
    dialogMessage.hidden = true
    if (!dialog.open) dialog.showModal()
  } else if (dialog.open && dialogMessage.hidden) {
    dialog.close()
  }

  status.textContent = update.nodes.length === 0 ? "当前 Passage 没有可显示内容" : ""
  story.setAttribute("aria-busy", "false")
  const restoredFocus =
    focusedKey === undefined
      ? null
      : story.querySelector(`[data-surface-key="${CSS.escape(focusedKey)}"]`)
  if (restoredFocus instanceof HTMLElement) restoredFocus.focus({ preventScroll: true })
  else if (!passageRoot.contains(document.activeElement)) passageRoot.focus({ preventScroll: true })
}

/** 64 级色阶（0..=63）→ RGB；与 TUI 的 palette_rgb 使用同一映射。
 *  灰阶 0-7（白 1 → 黑 7），光谱 8-63（红 8 → 橙 16 → 黄 24 → 绿 32 → 蓝 40 → 紫 48 → 深紫 63）。 */
function paletteColor(index) {
  if (!(index >= 1)) return ""
  const stops = [
    [1, [255, 255, 255]],
    [2, [229, 229, 229]],
    [3, [201, 201, 201]],
    [4, [138, 138, 138]],
    [5, [85, 85, 85]],
    [6, [50, 50, 50]],
    [7, [0, 0, 0]],
    [8, [255, 90, 90]],
    [16, [255, 158, 69]],
    [24, [242, 201, 76]],
    [32, [82, 200, 120]],
    [40, [79, 163, 255]],
    [48, [167, 139, 250]],
    [56, [124, 58, 237]],
    [63, [88, 28, 135]],
  ]
  for (let i = 0; i < stops.length - 1; i++) {
    const fromIndex = stops[i][0]
    const from = stops[i][1]
    const toIndex = stops[i + 1][0]
    const to = stops[i + 1][1]
    if (index <= toIndex) {
      const t = (index - fromIndex) / (toIndex - fromIndex)
      const lerp = (a, b) => Math.round(a + (b - a) * t)
      return `rgb(${lerp(from[0], to[0])}, ${lerp(from[1], to[1])}, ${lerp(from[2], to[2])})`
    }
  }
  return ""
}

/** 重绘前把页签 Panel 里的原节点放回 keyed reconcile 容器。 */
function resetDialogTabs() {
  for (const panel of dialogSurface.querySelectorAll(":scope > .dialog-panel")) {
    panel.querySelector(".dialog-heading-source")?.classList.remove("dialog-heading-source")
    panel.replaceWith(...panel.childNodes)
  }
  dialogTabs.replaceChildren()
}

/** 顶层语义标题划分页签，标题之后的节点归入对应页面。 */
function buildDialogTabs() {
  const headings = [...dialogSurface.children].filter((element) => element.matches("h1, h2"))
  const pageHeadings = headings.length > 0 ? headings : [null]
  const panels = pageHeadings.map(() => {
    const panel = document.createElement("section")
    panel.className = "dialog-panel"
    panel.setAttribute("role", "tabpanel")
    return panel
  })
  let panelIndex = 0
  for (const node of Array.from(dialogSurface.childNodes)) {
    const headingIndex = headings.indexOf(node)
    if (headingIndex >= 0) {
      panelIndex = headingIndex
      node.classList.add("dialog-heading-source")
    }
    panels[panelIndex].append(node)
  }
  panels.forEach((panel, index) => {
    const tab = document.createElement("button")
    tab.type = "button"
    tab.className = `dialog-tab${index === 0 ? " active" : ""}`
    tab.textContent = pageHeadings[index]?.textContent?.trim() || "消息"
    tab.setAttribute("role", "tab")
    tab.setAttribute("aria-selected", String(index === 0))
    panel.hidden = index !== 0
    tab.addEventListener("click", () => selectDialogTab(index, panels))
    dialogTabs.append(tab)
  })
  dialogSurface.append(...panels)
  return panels
}

/** 切换活动页签：更新按钮的 active/aria-selected 与对应面板的 hidden。 */
function selectDialogTab(activeIndex, panels) {
  ;[...dialogTabs.children].forEach((tab, index) => {
    const active = index === activeIndex
    tab.classList.toggle("active", active)
    tab.setAttribute("aria-selected", String(active))
    panels[index].hidden = !active
  })
}

/** 以 DTO key 为身份做最小 DOM 更新；key 类型变化时才替换元素。 */
function reconcile(container, nodes) {
  const existing = new Map(
    [...container.children].map((element) => [element.dataset.surfaceKey, element]),
  )
  let cursor = container.firstElementChild
  for (const node of nodes) {
    let element = existing.get(node.key)
    if (element === undefined || element.dataset.surfaceType !== nodeDomType(node)) {
      const replacement = createNode(node)
      if (element === undefined) element = replacement
      else {
        element.replaceWith(replacement)
        element = replacement
      }
    }
    updateNode(element, node)
    existing.delete(node.key)
    if (element !== cursor) container.insertBefore(element, cursor)
    cursor = element.nextElementSibling
  }
  for (const element of existing.values()) element.remove()
}

/** 按 DTO 节点类型创建语义元素骨架并绑定交互事件；字段值由 updateNode 填充。 */
function createNode(node) {
  let element
  if (node.type === "hardBreak") {
    element = document.createElement("br")
  } else if (node.type === "image") {
    element = document.createElement("figure")
    element.append(document.createElement("img"), document.createElement("figcaption"))
  } else if (node.type === "component") {
    element = document.createElement("section")
    if (node.capability === "meter" && node.version === 1) {
      element.className = "component-meter"
      element.append(document.createElement("span"), document.createElement("meter"))
    } else {
      const fallback = document.createElement("div")
      fallback.dataset.componentFallback = ""
      element.append(fallback)
    }
  } else if (node.type === "container") {
    element = document.createElement("div")
    element.className = "surface-slot"
  } else if (node.type === "replace") {
    element = document.createElement("div")
    element.hidden = true
    element.dataset.surfaceReplace = ""
  } else if (node.type === "action") {
    element = document.createElement("button")
    element.type = "button"
    element.addEventListener("click", () => {
      if (element.dataset.surfaceAction === "dismiss") dialog.close()
    })
  } else if (node.type === "checkbox" || node.type === "radiobutton") {
    element = document.createElement("input")
    element.type = node.type === "checkbox" ? "checkbox" : "radio"
    element.addEventListener("change", async () => {
      const value =
        node.type === "checkbox"
          ? JSON.parse(
              element.checked ? element.dataset.checkedValue : element.dataset.uncheckedValue,
            )
          : JSON.parse(element.dataset.inputValue)
      try {
        await submitInput(element.dataset.interaction, value)
        if (node.type === "radiobutton") {
          for (const radio of story.querySelectorAll(
            `input[type="radio"][name="${CSS.escape(element.name)}"]`,
          )) {
            radio.dataset.committedChecked = String(radio === element)
          }
        } else {
          element.dataset.committedChecked = String(element.checked)
        }
      } catch (error) {
        const controls =
          node.type === "radiobutton"
            ? story.querySelectorAll(`input[type="radio"][name="${CSS.escape(element.name)}"]`)
            : [element]
        for (const control of controls)
          control.checked = control.dataset.committedChecked === "true"
        showError(error)
      }
    })
  } else if (node.type === "textbox") {
    element = document.createElement("input")
    element.type = "text"
    element.addEventListener("change", async () => {
      const previous = element.dataset.committedValue ?? ""
      try {
        await submitInput(element.dataset.interaction, element.value)
        element.dataset.committedValue = element.value
      } catch (error) {
        element.value = previous
        showError(error)
      }
    })
  } else if (node.type === "navigation" || node.type === "button" || node.type === "safeReturn") {
    element = document.createElement("button")
    element.type = "button"
    if (node.type === "navigation" || node.type === "safeReturn") element.className = "choice"
    element.addEventListener("click", () => activate(element.dataset.interaction))
  } else {
    element = document.createElement(node.type === "styledText" ? styledTag(node) : "span")
  }
  element.dataset.surfaceKey = node.key
  element.dataset.surfaceType = nodeDomType(node)
  return element
}

/** 返回会影响元素标签或内部结构的类型身份，供 reconcile 判断能否复用。 */
function nodeDomType(node) {
  if (node.type === "styledText") return `${node.type}:${styledTag(node)}`
  if (node.type === "component") return `${node.type}:${node.capability}:${node.version}`
  return node.type
}

/** 语义字形 → 原生语义标签；结构性标题级别 → h1/h2；无字形时回退 span。 */
function styledTag(node) {
  if (node.heading === 1) return "h1"
  if (node.heading === 2) return "h2"
  const styles = node.styles
  if (styles.includes("quote")) return "q"
  if (styles.includes("code")) return "code"
  if (styles.includes("marked")) return "mark"
  if (styles.includes("inserted")) return "ins"
  if (styles.includes("deleted")) return "del"
  if (styles.includes("strong")) return "strong"
  if (styles.includes("emphasis")) return "em"
  if (styles.includes("small")) return "small"
  return "span"
}

/** 把一个 DTO 的可变字段同步到已经创建的语义元素。 */
function updateNode(element, node) {
  if (node.type === "text") {
    element.className = "surface-text"
    element.textContent = node.text
    return
  }
  if (node.type === "styledText") {
    element.className = `surface-text ${node.styles.map((style) => `text-${style}`).join(" ")}`
    element.dataset.color = String(node.color)
    if (node.color > 0) {
      element.style.setProperty("--narrava-color", paletteColor(node.color))
    } else {
      element.style.removeProperty("--narrava-color")
    }
    element.textContent = node.text
    if (node.delay > 0) {
      // Protocol 只约束可见时机；本 Host 在到期后选择 300ms 淡入。
      element.style.animation = `narrava-reveal var(--narrava-reveal-duration, 300ms) ${node.delay}ms both`
    } else {
      element.style.animation = ""
    }
    return
  }
  if (node.type === "image") {
    const image = element.querySelector("img")
    const caption = element.querySelector("figcaption")
    image.src = resourceUrl(node.resource)
    image.alt = node.alt
    caption.textContent = node.caption ?? ""
    caption.hidden = node.caption === null
    return
  }
  if (node.type === "component") {
    element.dataset.capability = node.capability
    element.dataset.version = String(node.version)
    if (node.capability === "meter" && node.version === 1) {
      const label = element.querySelector("span")
      const meter = element.querySelector("meter")
      label.textContent = typeof node.properties.label === "string" ? node.properties.label : ""
      meter.min = finiteNumber(node.properties.min, 0)
      meter.max = finiteNumber(node.properties.max, 100)
      meter.value = finiteNumber(node.properties.value, meter.min)
      meter.setAttribute("aria-label", label.textContent || "数值")
    } else {
      reconcile(element.querySelector("[data-component-fallback]"), node.fallback)
    }
    return
  }
  if (node.type === "container") {
    reconcile(element, node.nodes)
    return
  }
  if (node.type === "replace") {
    element.dataset.targetKind = node.target.kind
    element.dataset.targetValue = node.target.value
    reconcile(element, node.nodes)
    return
  }
  if (node.type === "action") {
    element.textContent = node.label
    element.dataset.surfaceAction = node.action
    element.dataset.actionRole = node.role
    return
  }
  if (node.type === "checkbox") {
    element.dataset.interaction = node.id
    element.dataset.uncheckedValue = JSON.stringify(node.unchecked)
    element.dataset.checkedValue = JSON.stringify(node.checked)
    element.checked = node.selected
    element.dataset.committedChecked = String(node.selected)
    return
  }
  if (node.type === "radiobutton") {
    element.dataset.interaction = node.id
    element.dataset.inputValue = JSON.stringify(node.value)
    // HTML 根据 name 原生维持互斥；同一 opaque group 也可被 TUI 映射为 RadioGroup。
    element.name = node.group
    element.checked = node.selected
    element.dataset.committedChecked = String(node.selected)
    return
  }
  if (node.type === "textbox") {
    element.dataset.interaction = node.id
    element.value = node.value
    element.dataset.committedValue = node.value
    return
  }
  element.textContent =
    node.type === "navigation" || node.type === "button" ? node.label : "安全返回"
  element.dataset.interaction = node.id
  element.dataset.target = node.target
}

/** 只接受有限数值，否则回退默认值（组件属性的防御性解析）。 */
function finiteNumber(value, fallback) {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback
}

/** 把 Core 的 Region 或 Surface key 替换映射到 Tauri 页面。 */
function applyReplacements() {
  const regions = new Map([
    ["header", passageHeader],
    ["main", passage],
    ["footer", passageFooter],
    ["bar", barSurface],
    ["dialog", dialogSurface],
  ])
  for (const command of story.querySelectorAll("[data-surface-replace]")) {
    const target =
      command.dataset.targetKind === "region"
        ? (regions.get(command.dataset.targetValue) ?? passage)
        : story.querySelector(`[data-surface-key="${CSS.escape(command.dataset.targetValue)}"]`)
    if (!(target instanceof Element) || target === command)
      throw new Error("replace 目标不存在或形成自引用")
    target.replaceChildren(...command.childNodes)
  }
}

/** 把交互 ID 送回 Worker 执行；成功后渲染并返回新的 Surface 更新。 */
async function activate(interaction) {
  setBusy(true, "正在处理行动…")
  try {
    const update = await invoke("activate", { interaction })
    render(update)
    return structuredClone(update)
  } catch (error) {
    showError(error)
  } finally {
    setBusy(false)
  }
}

/** 输入先由 Worker 校验并写入 State；失败时调用方负责恢复控件的已提交值。 */
async function submitInput(interaction, value) {
  await invoke("input", { interaction, value })
}

/** 忙碌期间禁用全部交互控件并同步 aria-busy；message 非空时写入状态行。 */
function setBusy(isBusy, message = "") {
  story.setAttribute("aria-busy", String(isBusy))
  for (const control of story.querySelectorAll(
    "button[data-interaction], button[data-surface-action], input[data-interaction]",
  )) {
    control.disabled = isBusy
  }
  if (message) status.textContent = message
}

/** Runtime 错误复用 Host Dialog，但清空普通 Dialog 页签以免混入旧内容。 */
function showError(error) {
  const code = typeof error?.code === "string" ? error.code : "tauri_host.unknown"
  const message = typeof error?.message === "string" ? error.message : String(error)
  resetDialogTabs()
  reconcile(dialogSurface, [])
  dialogSurface.hidden = true
  dialogTabs.replaceChildren()
  const errorTab = document.createElement("button")
  errorTab.type = "button"
  errorTab.className = "dialog-tab active"
  errorTab.textContent = "运行错误"
  errorTab.setAttribute("role", "tab")
  errorTab.setAttribute("aria-selected", "true")
  dialogTabs.append(errorTab)
  dialogMessage.hidden = false
  dialogMessage.textContent = `${code}：${message}`
  dialog.showModal()
  status.textContent = "运行失败"
  story.setAttribute("aria-busy", "false")
}

/** 作者 CSS 只在 Host 默认主题后追加；resource() 被收敛到只读自定义协议。 */
function applyAuthorStyles(assets) {
  document.title = assets.title
  resourcePaths = new Set(assets.resources.map((resource) => resource.path))
  for (const style of assets.styles) {
    const css = style.css.replace(
      /resource\(\s*(["'])([^"']+)\1\s*\)/gu,
      (_match, _quote, path) => `url("${resourceUrl(path)}")`,
    )
    const url = URL.createObjectURL(new Blob([css], { type: "text/css" }))
    objectUrls.add(url)
    const link = document.createElement("link")
    link.rel = "stylesheet"
    link.href = url
    link.dataset.narravaStyle = style.path
    document.head.append(link)
  }
}

/** 把 Resource 逻辑路径编码为只读自定义协议的 URL；未收录的路径抛错。 */
function resourceUrl(path) {
  if (!resourcePaths.has(path)) throw new Error(`Resource 不存在：${path}`)
  const encoded = path.split("/").map(encodeURIComponent).join("/")
  return `narrava-resource://localhost/${encoded}`
}

/** 并行取得只读资产、开发开关和首个 Surface，再进行首次渲染。 */
async function start() {
  try {
    const [assets, developer, update] = await Promise.all([
      invoke("host_assets"),
      invoke("developer_enabled"),
      invoke("start_game"),
    ])
    configureDeveloperMode(developer)
    applyAuthorStyles(assets)
    render(update)
  } catch (error) {
    showError(error)
  }
}

window.addEventListener("beforeunload", () => {
  for (const url of objectUrls) URL.revokeObjectURL(url)
})

start()
