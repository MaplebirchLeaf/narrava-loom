import audio from "./audio"
import contract from "../../../bindings/script-contract.json"
import events from "./event"
import host from "./host"
import type { BootstrapContract } from "./internal"
import logger from "./logger"
import macro from "./macro"
import random from "./random"
import reaction from "./reaction"
import resources from "./resource"
import runtime from "./runtime"
import save from "./save"
import state from "./state"
import surface from "./surface"
import location from "./location"

state()
random()
location()
reaction()
macro()
logger()
events(contract.builtinEvents)
host()
audio()
save()
resources()
surface()
runtime(contract as BootstrapContract)
