// 结果是执行时的只读快照；展开节点不会再次调用 Runtime。
export function renderConsoleValue(document, value, usePath) {
  function render(node, path) {
    const container = document.createElement(
      node.children.length || node.truncated ? "details" : "div",
    )
    container.className = `debug-value debug-value-${node.kind}`
    const label = document.createElement(container.tagName === "DETAILS" ? "summary" : "span")
    label.textContent = `${node.name ? `${node.name}: ` : ""}${node.kind === "undefined" ? "undefined · 执行完成 / Done" : node.preview}`
    label.title = [node.signature, node.help].filter(Boolean).join("\n")
    container.append(label)
    if (node.help || node.signature) {
      const description = document.createElement("small")
      description.className = "debug-description"
      description.textContent = [node.signature, node.help].filter(Boolean).join("\n")
      // 函数说明按需展开，避免对象树被长段帮助文字撑满。
      const help = document.createElement("details")
      const title = document.createElement("summary")
      title.textContent = "说明 / Help"
      help.append(title, description)
      container.append(help)
    }
    if (path) {
      const button = document.createElement("button")
      button.type = "button"
      button.className = "debug-use-path"
      button.textContent = "使用 / Use"
      button.title = path
      button.addEventListener("click", () => usePath(path))
      container.append(button)
    }
    for (const child of node.children) {
      const nextPath = path
        ? /^[A-Za-z_$][\w$]*$/.test(child.name)
          ? `${path}.${child.name}`
          : `${path}[${JSON.stringify(child.name)}]`
        : ""
      container.append(render(child, nextPath))
    }
    if (node.truncated) {
      const more = document.createElement("small")
      more.textContent = "… 预览已截断，请查询更具体的属性 / Query a narrower property"
      container.append(more)
    }
    return container
  }
  const root = render(value, /^[A-Za-z_$][\w$]*(?:\.[\w$]+)*$/.test(value.name) ? value.name : "")
  if (root.tagName === "DETAILS") root.open = true
  return root
}
