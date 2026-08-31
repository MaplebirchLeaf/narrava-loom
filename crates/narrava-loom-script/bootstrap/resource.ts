import { runtimeConfiguration, scriptGlobals } from "./internal"

export default function resources(): void {
  scriptGlobals.Resource = Object.seal({
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
  scriptGlobals.I18n = Object.freeze({
    get defaultLocale() {
      return runtimeConfiguration.defaultLocale
    },
    get locale() {
      return runtimeConfiguration.locale
    },
    export: () => runtimeConfiguration.i18nExport,
  })
}
