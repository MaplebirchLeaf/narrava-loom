import contract from "../../../bindings/script-contract.json"
import { installEvent } from "./event"
import { installHost } from "./host"
import type { BootstrapContract } from "./internal"
import { installLogger } from "./logger"
import { installMacro } from "./macro"
import { installReaction } from "./reaction"
import { installResource } from "./resource"
import { installRuntime } from "./runtime"
import { installSave } from "./save"
import { installState } from "./state"
import { installSurface } from "./surface"

installState()
installReaction()
installMacro()
installLogger()
installEvent(contract.builtinEvents)
installHost()
installSave()
installResource()
installSurface()
installRuntime(contract as BootstrapContract)
