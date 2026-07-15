import type {Command, RuntimeEvent} from "../protocol.js";

export type ShutdownReason = "user" | "signal" | "fatal";

export interface RuntimeApplication {
  init(): Promise<RuntimeEvent[]>;
  dispatch(command: Command): AsyncGenerator<RuntimeEvent>;
  shutdown(reason: ShutdownReason): Promise<void>;
}

export type {
  ActiveModelSelection,
  ModelRuntimeSnapshot,
  ModelRuntimeSnapshotPort
} from "./model-selection-service.js";
export type {AgentRunEngineDependencies, AgentCapabilityDispatcherFactory} from "./agent-run-engine.js";
export {AgentRunEngine} from "./agent-run-engine.js";
