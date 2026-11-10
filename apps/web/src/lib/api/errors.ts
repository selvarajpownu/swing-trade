import type { ApiErrorCodeFromServer } from "./types";

/** Error codes from the backend plus the ones produced on our side. */
export type ApiErrorCode =
  ApiErrorCodeFromServer | "timeout" | "network" | "invalid_response" | "not_configured";

const SERVER_CODES: readonly ApiErrorCodeFromServer[] = [
  "not_found",
  "unauthorized",
  "bad_request",
  "upstream",
  "internal",
];

function isServerCode(value: unknown): value is ApiErrorCodeFromServer {
  return typeof value === "string" && (SERVER_CODES as readonly string[]).includes(value);
}

function codeFromStatus(status: number): ApiErrorCode {
  if (status === 400 || status === 422) return "bad_request";
  if (status === 401 || status === 403) return "unauthorized";
  if (status === 404) return "not_found";
  if (status === 502 || status === 503) return "upstream";
  if (status === 504) return "timeout";
  return "internal";
}

/**
 * Typed error for API calls. Safe to use on both server and client
 * (it carries no secrets).
 */
export class ApiError extends Error {
  readonly code: ApiErrorCode;
  readonly status: number;

  constructor(code: ApiErrorCode, message: string, status: number) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.status = status;
  }

  get isNotFound(): boolean {
    return this.code === "not_found";
  }

  /** Build an ApiError from a non-2xx response body (`{"error": {...}}`). */
  static fromResponse(status: number, body: unknown): ApiError {
    if (typeof body === "object" && body !== null && "error" in body) {
      const err = (body as { error: unknown }).error;
      if (typeof err === "object" && err !== null) {
        const { code, message } = err as { code?: unknown; message?: unknown };
        return new ApiError(
          isServerCode(code) ? code : codeFromStatus(status),
          typeof message === "string" && message.length > 0
            ? message
            : `Request failed with status ${status}`,
          status,
        );
      }
    }
    return new ApiError(codeFromStatus(status), `Request failed with status ${status}`, status);
  }

  toJSON(): { error: { code: ApiErrorCode; message: string } } {
    return { error: { code: this.code, message: this.message } };
  }
}

export function isApiError(value: unknown): value is ApiError {
  return value instanceof ApiError;
}

/** A short, user-facing explanation for an error code. */
export function describeApiError(error: unknown): string {
  if (!isApiError(error)) {
    return error instanceof Error && error.message ? error.message : "Something went wrong.";
  }
  switch (error.code) {
    case "timeout":
      return "The data service took too long to respond.";
    case "network":
      return "Could not reach the data service. Is the API running?";
    case "unauthorized":
      return "The data service rejected our token. Check API_TOKEN on the web server.";
    case "upstream":
      return "The market data provider is unavailable right now.";
    case "not_found":
      return error.message || "Not found.";
    case "bad_request":
      return error.message || "The request was not valid.";
    case "invalid_response":
      return "The data service sent a response we could not read.";
    case "not_configured":
      return "The web server is not configured to reach the data service.";
    case "internal":
    default:
      return "The data service hit an internal error.";
  }
}
