/// <reference types="@narrava-loom/types" />

// Twee 只调用明确暴露给 State.global 的作者函数，不直接依赖 Worker 全局对象。

function exportSave(slot = "manual-1"): string {
  Save.export(slot)
  return `已请求导出存档：${slot}`
}

function importSave(slot = "manual-1"): string {
  Save.import(slot)
  return `已请求读取存档：${slot}`
}

function logInfo(message: string, target = "story"): void {
  Logger.info(target, message)
}

function logWarning(message: string, target = "story"): void {
  Logger.warn(target, message)
}

function activeLocale(): string {
  return I18n.locale
}

function defaultLocale(): string {
  return I18n.defaultLocale
}

function selectLanguage(locale: string): string {
  I18n.select(locale)
  return `已请求切换语言：${locale}`
}

function logI18nTemplate(): string {
  const template = I18n.export()
  Logger.info("i18n.export", template)
  return `I18n 模板已导出到日志（${template.length} 字符）`
}

State.global.extend({
  exportSave,
  importSave,
  logInfo,
  logWarning,
  activeLocale,
  defaultLocale,
  selectLanguage,
  logI18nTemplate,
})
