import "server-only";

import { NextResponse } from "next/server";

import { ApiError } from "./errors";

/** Shared helpers for the /api/* route handlers that proxy to the Rust API. */

export const NO_STORE_HEADERS = { "Cache-Control": "no-store" } as const;

export function jsonNoStore(data: unknown, status = 200) {
  return NextResponse.json(data, { status, headers: NO_STORE_HEADERS });
}

export function badRequest(message: string) {
  return jsonNoStore({ error: { code: "bad_request", message } }, 400);
}

/** Maps an error thrown by lib/api/server.ts to a JSON response for the browser. */
export function apiErrorResponse(err: unknown, tag: string) {
  if (err instanceof ApiError) {
    // A 401/403 from the backend is about OUR token, not the visitor: report it as a gateway error.
    const status =
      err.status === 401 || err.status === 403 ? 502 : err.status >= 400 ? err.status : 502;
    if (status >= 500) console.error(`[${tag}] ${err.code}: ${err.message}`);
    return jsonNoStore(err.toJSON(), status);
  }
  console.error(`[${tag}] unexpected error`, err);
  return jsonNoStore({ error: { code: "internal", message: "Unexpected error." } }, 500);
}
