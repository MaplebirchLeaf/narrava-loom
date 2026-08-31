import contract from "../../../bindings/script-contract.json"
import events from "./event"
import host from "./host"
import type { BootstrapContract } from "./internal"
import logger from "./logger"
import macro from "./macro"
import reaction from "./reaction"
import resources from "./resource"
import runtime from "./runtime"
import save from "./save"
import state from "./state"
import surface from "./surface"

state()
reaction()
macro()
logger()
events(contract.builtinEvents)
host()
save()
resources()
surface()
runtime(contract as BootstrapContract)
