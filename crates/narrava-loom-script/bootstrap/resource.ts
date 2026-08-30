import { configuration, globals } from "./internal"

export function installResource(): void {
  globals.Resource = Object.seal({
    paths: () => __narravaResourcePaths(),
    has: (path: string) => __narravaResourceHas(path),
    pick: (paths: string[]) => paths.find((path) => __narravaResourceHas(path)),
    info: (path: string) => __narravaResourceInfo(path),
    read: (path: string) => {
      const bytes = __narravaResourceRead(path)
      return bytes === undefined ? undefined : Uint8Array.from(bytes)
    },
    text: (path: string) => __narravaResourceText(path),
  })
  globals.I18n = Object.freeze({
    get defaultLocale() {
      return configuration.defaultLocale
    },
    get locale() {
      return configuration.locale
    },
    export: () => configuration.i18nExport,
  })
}
