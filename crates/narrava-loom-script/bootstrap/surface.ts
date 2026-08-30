import { globals } from "./internal"

interface SurfaceOptions {
  key?: string
  styles?: unknown[]
  color?: unknown
  delay?: unknown
  heading?: unknown
  alt?: unknown
  caption?: unknown
  role?: unknown
}

function surfaceNode(
  kind: string,
  value: Record<string, unknown>,
): Readonly<Record<string, unknown>> {
  return Object.freeze({ __narravaSurface: kind, ...value })
}

export function installSurface(): void {
  globals.Surface = Object.freeze({
    text: (text: unknown, options: SurfaceOptions = {}) =>
      surfaceNode("text", {
        text: String(text),
        key: options.key,
        styles: Object.freeze([...(options.styles ?? [])]),
        color: options.color ?? 0,
        delay: options.delay,
        heading: options.heading,
      }),
    hardBreak: () => surfaceNode("hard-break", {}),
    image: (resource: unknown, options: SurfaceOptions = {}) =>
      surfaceNode("image", {
        resource: String(resource),
        key: options.key,
        alt: options.alt ?? "",
        caption: options.caption,
      }),
    region: (region: unknown, children: unknown[], options: SurfaceOptions = {}) =>
      surfaceNode("region", {
        region,
        key: options.key,
        children: Object.freeze([...children]),
      }),
    component: (
      capability: unknown,
      version: unknown,
      properties: Record<string, unknown>,
      fallback: unknown[],
      options: SurfaceOptions = {},
    ) =>
      surfaceNode("component", {
        capability,
        version,
        properties: Object.freeze({ ...properties }),
        children: Object.freeze([...fallback]),
        key: options.key,
      }),
    action: (label: unknown, action: unknown, options: SurfaceOptions = {}) =>
      surfaceNode("action", {
        label: String(label),
        action,
        role: options.role ?? "default",
        key: options.key,
      }),
    fragment: (...children: unknown[]) =>
      surfaceNode("fragment", { children: Object.freeze(children) }),
  })
}
