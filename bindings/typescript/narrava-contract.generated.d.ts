/** Generated from Protocol Rust declarations and bindings/script-contract.json. Do not edit by hand. */
declare global {
  type NarravaScriptGlobalName =
    | "State"
    | "V"
    | "T"
    | "setup"
    | "World"
    | "Reaction"
    | "Macro"
    | "Logger"
    | "Event"
    | "Host"
    | "Engine"
    | "Story"
    | "Save"
    | "Resource"
    | "Audio"
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
  type NarravaRuntimeUpdateType =
    | "audio"
    | "ready"
    | "applied"
    | "pending"
  type NarravaPendingOperationType = "delay" | "save" | "selectLanguage"
  type NarravaSurfaceNodeType =
    | "text"
    | "hardBreak"
    | "styledText"
    | "image"
    | "dialog"
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
  type NarravaHostNodeDto = { readonly type: "text"; readonly key: string; readonly text: string } | { readonly type: "hardBreak"; readonly key: string } | { readonly type: "styledText"; readonly key: string; readonly text: string; readonly styles: readonly string[]; readonly color: number; readonly delay?: number; readonly heading?: number } | { readonly type: "image"; readonly key: string; readonly resource: string; readonly alt: string } | { readonly type: "dialog"; readonly key: string; readonly initial: string; readonly pages: readonly NarravaHostDialogPageDto[] } | { readonly type: "region"; readonly key: string; readonly region: string; readonly nodes: readonly NarravaHostNodeDto[] } | { readonly type: "container"; readonly key: string; readonly presentation: NarravaContainerPresentationDto; readonly flow: NarravaContainerFlowDto; readonly nodes: readonly NarravaHostNodeDto[] } | { readonly type: "component"; readonly key: string; readonly capability: string; readonly version: number; readonly properties: unknown; readonly fallback: readonly NarravaHostNodeDto[] } | { readonly type: "replace"; readonly key: string; readonly target: NarravaHostReplaceTargetDto; readonly nodes: readonly NarravaHostNodeDto[] } | { readonly type: "action"; readonly key: string; readonly label: string; readonly action: string; readonly role: string } | { readonly type: "checkbox"; readonly key: string; readonly id: string; readonly unchecked: unknown; readonly checked: unknown; readonly selected: boolean } | { readonly type: "radiobutton"; readonly key: string; readonly id: string; readonly group: string; readonly value: unknown; readonly selected: boolean } | { readonly type: "textbox"; readonly key: string; readonly id: string; readonly value: string } | { readonly type: "navigation"; readonly key: string; readonly id: string; readonly label: string; readonly target: string | null } | { readonly type: "button"; readonly key: string; readonly id: string; readonly label: string; readonly target: string | null } | { readonly type: "safeReturn"; readonly key: string; readonly id: string; readonly target: string }
  type NarravaHostDialogPageDto = { readonly title: string; readonly nodes: readonly NarravaHostNodeDto[] }
  type NarravaContainerPresentationDto = "plain" | "panel"
  type NarravaContainerFlowDto = "stack" | "row"
  type NarravaHostReplaceTargetDto = { readonly kind: "region"; readonly value: string } | { readonly kind: "key"; readonly value: string }
  type NarravaHostUpdateDto = { readonly current: string; readonly nodes: readonly NarravaHostNodeDto[]; readonly can_back: boolean; readonly can_forward: boolean }
  type NarravaRuntimeCommand = { readonly type: "start" } | { readonly type: "back" } | { readonly type: "forward" } | { readonly type: "activate"; readonly interaction: string } | { readonly type: "input"; readonly interaction: string; readonly value: unknown } | { readonly type: "save"; readonly operation: NarravaRuntimeSaveOperation; readonly target: string } | { readonly type: "selectLanguage"; readonly locale: string } | { readonly type: "resume"; readonly operation: number; readonly result?: NarravaPendingResult } | { readonly type: "cancel"; readonly operation: number }
  type NarravaRuntimeSaveOperation = "export" | "import"
  type NarravaPendingOperation = { readonly type: "delay"; readonly operation: number; readonly milliseconds: number } | { readonly type: "save"; readonly operation: number; readonly direction: NarravaRuntimeSaveOperation; readonly target: string; readonly document?: readonly number[] } | { readonly type: "selectLanguage"; readonly operation: number; readonly locale: string }
  type NarravaPendingResult = { readonly type: "save"; readonly document?: readonly number[] } | { readonly type: "selectLanguage" } | { readonly type: "failed"; readonly error: NarravaHostErrorDto }
  type NarravaAudioEffect = { readonly type: "play"; readonly resource: string; readonly channel: string; readonly loop: boolean; readonly volume: number } | { readonly type: "stop"; readonly channel: string }
  type NarravaRuntimeUpdate = { readonly type: "audio"; readonly effects: readonly NarravaAudioEffect[]; readonly update: NarravaHostUpdateDto | null } | { readonly type: "ready"; readonly update: NarravaHostUpdateDto } | { readonly type: "applied" } | { readonly type: "pending"; readonly operation: NarravaPendingOperation }
  type NarravaRuntimeRequest = { readonly protocolVersion: 2; readonly session: string; readonly command: NarravaRuntimeCommand }
  type NarravaRuntimeResponse = { readonly protocolVersion: 2; readonly session: string; readonly update: NarravaRuntimeUpdate }
}

export {}
