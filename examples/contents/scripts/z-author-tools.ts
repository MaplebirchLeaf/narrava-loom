/// <reference types="@narrava-loom/types" />

// .twee 表达式不能直接调用 Worker 全局；作者先封装，再由 z-register.ts 暴露给 State.global。

function saveGame(slot = "manual-1"): string {
  Save.export(slot)
  return `已请求导出存档：${slot}`
}

function loadGame(slot = "manual-1"): string {
  Save.import(slot)
  return `已请求读取存档：${slot}`
}

function logStory(message: string, target = "story"): void {
  Logger.info(target, message)
}

function logWarnStory(message: string, target = "story"): void {
  Logger.warn(target, message)
}

function currentLocale(): string {
  return I18n.locale
}

function defaultLocale(): string {
  return I18n.defaultLocale
}

function switchLanguage(locale: string): string {
  I18n.select(locale)
  return `已请求切换语言：${locale}`
}

function exportI18nTemplate(): string {
  const template = I18n.export()
  Logger.info("i18n.export", template)
  return `I18n 模板已导出到日志（${template.length} 字符）`
}

State.global.extend({
  saveGame,
  loadGame,
  logStory,
  logWarnStory,
  currentLocale,
  defaultLocale,
  switchLanguage,
  exportI18nTemplate,
})
