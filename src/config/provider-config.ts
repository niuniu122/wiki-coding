import type {AppConfig, ModelProviderConfig} from "../types.js";

export interface ActiveProvider extends ModelProviderConfig {
  id: string;
}

export function getActiveProvider(config: AppConfig): ActiveProvider {
  const provider = config.modelProviders[config.modelProvider];
  if (!provider) {
    throw new Error(`Unknown model provider: ${config.modelProvider}`);
  }
  return {...provider, id: config.modelProvider};
}

export function listProviders(config: AppConfig): ActiveProvider[] {
  return Object.entries(config.modelProviders).map(([id, provider]) => ({...provider, id}));
}

export function formatProviderSummary(config: AppConfig, provider: ActiveProvider): string {
  return `${provider.id} | ${provider.protocol} | ${provider.baseUrl} | model=${
    provider.id === config.modelProvider
      ? config.model
      : provider.defaultModel ?? config.model
  }`;
}
