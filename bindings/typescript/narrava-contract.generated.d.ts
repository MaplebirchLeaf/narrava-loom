/** Generated from bindings/script-contract.json. Do not edit by hand. */
declare global {
  type NarravaScriptGlobalName =
    | "State"
    | "V"
    | "T"
    | "setup"
    | "Reaction"
    | "Macro"
    | "Logger"
    | "Event"
    | "Host"
    | "Engine"
    | "Story"
    | "Save"
    | "Resource"
    | "I18n"
    | "Surface"
  type NarravaBuiltinEventName =
    | "passage:init"
    | "passage:start"
    | "passage:render"
    | "passage:display"
    | "passage:end"
  type NarravaSurfaceBuilderName =
    | "text"
    | "hardBreak"
    | "image"
    | "region"
    | "component"
    | "action"
    | "fragment"
  type NarravaRuntimeCommandType =
    | "start"
    | "back"
    | "forward"
    | "activate"
    | "input"
    | "save"
    | "selectLanguage"
    | "resume"
    | "cancel"
  type NarravaRuntimeUpdateType = "ready" | "applied" | "pending"
  type NarravaPendingOperationType = "delay" | "save" | "selectLanguage"
  type NarravaSurfaceNodeType =
    | "text"
    | "hardBreak"
    | "styledText"
    | "image"
    | "region"
    | "container"
    | "component"
    | "replace"
    | "action"
    | "checkbox"
    | "radiobutton"
    | "textbox"
    | "navigation"
    | "button"
    | "safeReturn"
  type NarravaHostErrorDto = { readonly code: string; readonly message: string }
  type NarravaRuntimeSaveOperation = "export" | "import"
  type NarravaPendingResult = { readonly type: "save"; readonly document?: string } | { readonly type: "selectLanguage" } | { readonly type: "failed"; readonly error: NarravaHostErrorDto }
  type NarravaRuntimeCommand = { readonly type: "start" } | { readonly type: "back" } | { readonly type: "forward" } | { readonly type: "activate"; readonly interaction: string } | { readonly type: "input"; readonly interaction: string; readonly value: unknown } | { readonly type: "save"; readonly operation: NarravaRuntimeSaveOperation; readonly target: string } | { readonly type: "selectLanguage"; readonly locale: string } | { readonly type: "resume"; readonly operation: number; readonly result?: NarravaPendingResult } | { readonly type: "cancel"; readonly operation: number }
  type NarravaPendingOperation = { readonly type: "delay"; readonly operation: number; readonly milliseconds: number } | { readonly type: "save"; readonly operation: number; readonly direction: NarravaRuntimeSaveOperation; readonly target: string; readonly document?: string } | { readonly type: "selectLanguage"; readonly operation: number; readonly locale: string }
  type NarravaRuntimeUpdate = { readonly type: "ready"; readonly update: { readonly current: string; readonly nodes: readonly unknown[]; readonly can_back: boolean; readonly can_forward: boolean } } | { readonly type: "applied" } | { readonly type: "pending"; readonly operation: NarravaPendingOperation }
  type NarravaRuntimeRequest = { readonly protocolVersion: 1; readonly session: string; readonly command: NarravaRuntimeCommand }
  type NarravaRuntimeResponse = { readonly protocolVersion: 1; readonly session: string; readonly update: NarravaRuntimeUpdate }
}

export {}
