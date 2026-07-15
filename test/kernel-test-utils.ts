import {join} from "node:path";
import {CredentialStore} from "../src/config/credential-store.js";
import type {
  ProviderGateway,
  ProviderGatewayEvent,
  ProviderRequest
} from "../src/providers/provider-gateway.js";
import type {Command, RuntimeEvent} from "../src/protocol.js";
import {ApplicationKernel} from "../src/runtime/application-kernel.js";
import type {ModelAdapter} from "../src/runtime/model-adapter.js";

class ModelAdapterGateway implements ProviderGateway {
  constructor(private readonly adapter: ModelAdapter) {}

  async *stream(request: ProviderRequest): AsyncGenerator<ProviderGatewayEvent> {
    for await (const event of this.adapter.streamResponse(request)) {
      if (event.type === "delta") {
        yield {type: "text.delta", delta: event.delta};
      } else {
        yield event;
      }
    }
  }
}

export function createKernelTestApplication(
  cwd: string,
  stateRoot: string,
  modelAdapter?: ModelAdapter
): ApplicationKernel {
  return new ApplicationKernel({
    cwd,
    stateRoot,
    credentialStore: new CredentialStore({
      keyring: null,
      userConfigDir: join(stateRoot, "user-config"),
      env: {}
    }),
    ...(modelAdapter
      ? {providerGateway: new ModelAdapterGateway(modelAdapter)}
      : {})
  });
}

export async function collectCommand(
  application: ApplicationKernel,
  command: Command
): Promise<RuntimeEvent[]> {
  const events: RuntimeEvent[] = [];
  for await (const event of application.dispatch(command)) {
    events.push(event);
  }
  return events;
}
