import {TransportError} from "./http-transport.js";

export type ProviderErrorKind =
  | "authentication"
  | "rate_limit"
  | "timeout"
  | "network"
  | "server"
  | "request"
  | "protocol"
  | "unknown";

export interface ProviderErrorOptions {
  providerId: string;
  providerName: string;
  kind: ProviderErrorKind;
  status?: number;
  requestId?: string;
  retryable: boolean;
  cause?: unknown;
}

export class ProviderError extends Error {
  readonly providerId: string;
  readonly providerName: string;
  readonly kind: ProviderErrorKind;
  readonly status?: number;
  readonly requestId?: string;
  readonly retryable: boolean;

  constructor(message: string, options: ProviderErrorOptions) {
    super(message, {cause: options.cause});
    this.name = "ProviderError";
    this.providerId = options.providerId;
    this.providerName = options.providerName;
    this.kind = options.kind;
    this.retryable = options.retryable;
    if (options.status !== undefined) {
      this.status = options.status;
    }
    if (options.requestId !== undefined) {
      this.requestId = options.requestId;
    }
  }
}

export function createHttpProviderError(params: {
  providerId: string;
  providerName: string;
  status: number;
  body: string;
  apiKey: string;
}): ProviderError {
  const parsed = parseErrorBody(params.body);
  const kind = classifyStatus(params.status);
  const requestId = parsed.requestId;
  const upstreamMessage = redactKnownSecret(parsed.message, params.apiKey).slice(0, 300);
  const retryable = kind === "rate_limit" || kind === "server";
  const message = formatUserMessage({
    providerName: params.providerName,
    kind,
    status: params.status,
    upstreamMessage,
    ...(requestId ? {requestId} : {})
  });

  return new ProviderError(message, {
    providerId: params.providerId,
    providerName: params.providerName,
    kind,
    status: params.status,
    ...(requestId ? {requestId} : {}),
    retryable
  });
}

export function normalizeProviderError(params: {
  providerId: string;
  providerName: string;
  error: unknown;
}): ProviderError {
  if (params.error instanceof ProviderError) {
    return params.error;
  }
  if (params.error instanceof TransportError) {
    const kind = params.error.kind;
    const message =
      kind === "timeout"
        ? `${params.providerName} API 请求超时，请检查网络或供应商网关。`
        : `无法连接 ${params.providerName}，请检查网络和 baseUrl。`;
    return new ProviderError(message, {
      providerId: params.providerId,
      providerName: params.providerName,
      kind,
      retryable: true,
      cause: params.error
    });
  }

  return new ProviderError(`${params.providerName} 请求失败。`, {
    providerId: params.providerId,
    providerName: params.providerName,
    kind: "unknown",
    retryable: false,
    cause: params.error
  });
}

function classifyStatus(status: number): ProviderErrorKind {
  if (status === 401 || status === 403) {
    return "authentication";
  }
  if (status === 429) {
    return "rate_limit";
  }
  if (status >= 500) {
    return "server";
  }
  return "request";
}

function parseErrorBody(body: string): {message: string; requestId?: string} {
  try {
    const parsed = JSON.parse(body) as {
      error?: {message?: string};
      message?: string;
      request_id?: string;
    };
    const result: {message: string; requestId?: string} = {
      message: parsed.error?.message ?? parsed.message ?? "upstream request failed"
    };
    if (parsed.request_id) {
      result.requestId = parsed.request_id;
    }
    return result;
  } catch {
    return {message: body || "upstream request failed"};
  }
}

function redactKnownSecret(message: string, secret: string): string {
  return secret ? message.split(secret).join("[REDACTED]") : message;
}

function formatUserMessage(params: {
  providerName: string;
  kind: ProviderErrorKind;
  status: number;
  upstreamMessage: string;
  requestId?: string;
}): string {
  const requestIdLine = params.requestId ? `\nrequest_id: ${params.requestId}` : "";
  if (params.kind === "authentication") {
    return (
      `${params.providerName} 拒绝了当前 API key。` +
      "\n处理：输入 /api 重新设置当前供应商的有效密钥，并确认 baseUrl、模型和账号属于同一供应商。" +
      requestIdLine
    );
  }
  if (params.kind === "rate_limit") {
    return `${params.providerName} 当前请求过多，请稍后重试。${requestIdLine}`;
  }
  if (params.kind === "server") {
    return `${params.providerName} 服务暂时异常（HTTP ${params.status}），可以稍后重试。${requestIdLine}`;
  }
  return (
    `${params.providerName} API ${params.status}: ${params.upstreamMessage || "request failed"}` +
    requestIdLine
  );
}
