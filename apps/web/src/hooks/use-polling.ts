"use client";

import { useCallback, useEffect, useRef, useState } from "react";

export const DEFAULT_POLL_INTERVAL_MS = 15_000;

export interface UsePollingOptions<T> {
  /** Data rendered on the server; shown until the first refresh lands. */
  initialData: T;
  intervalMs?: number;
  enabled?: boolean;
}

export interface UsePollingResult<T> {
  data: T;
  /** Last refresh error; cleared on the next successful refresh. */
  error: Error | null;
  isRefreshing: boolean;
  /** Epoch ms of the last successful load (null = server data, not yet refreshed). */
  lastUpdatedAt: number | null;
  refresh: () => Promise<void>;
}

/**
 * Re-runs `fetcher` every `intervalMs` while the tab is visible.
 *
 * - Pauses when the document is hidden and aborts any request in flight.
 * - When the tab becomes visible again, refreshes at once if data is stale.
 * - Keeps the last good data on error, so a blip does not blank the page.
 *
 * Remount the caller (React `key`) to start over with new `initialData`.
 */
export function usePolling<T>(
  fetcher: (signal: AbortSignal) => Promise<T>,
  { initialData, intervalMs = DEFAULT_POLL_INTERVAL_MS, enabled = true }: UsePollingOptions<T>,
): UsePollingResult<T> {
  const [data, setData] = useState<T>(initialData);
  const [error, setError] = useState<Error | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [lastUpdatedAt, setLastUpdatedAt] = useState<number | null>(null);

  const fetcherRef = useRef(fetcher);
  const controllerRef = useRef<AbortController | null>(null);
  const lastRunRef = useRef<number>(0);

  useEffect(() => {
    fetcherRef.current = fetcher;
  }, [fetcher]);

  const refresh = useCallback(async () => {
    controllerRef.current?.abort();
    const controller = new AbortController();
    controllerRef.current = controller;
    setIsRefreshing(true);
    try {
      const next = await fetcherRef.current(controller.signal);
      if (controller.signal.aborted) return;
      const now = Date.now();
      lastRunRef.current = now;
      setData(next);
      setError(null);
      setLastUpdatedAt(now);
    } catch (err) {
      if (controller.signal.aborted) return;
      setError(err instanceof Error ? err : new Error(String(err)));
    } finally {
      if (controllerRef.current === controller) {
        controllerRef.current = null;
        setIsRefreshing(false);
      }
    }
  }, []);

  useEffect(() => {
    if (!enabled) return;
    // Server-rendered data counts as fresh.
    if (lastRunRef.current === 0) lastRunRef.current = Date.now();

    let timer: ReturnType<typeof setInterval> | undefined;
    const start = () => {
      if (timer === undefined) timer = setInterval(() => void refresh(), intervalMs);
    };
    const stop = () => {
      if (timer !== undefined) {
        clearInterval(timer);
        timer = undefined;
      }
    };
    const onVisibilityChange = () => {
      if (document.visibilityState === "hidden") {
        stop();
        controllerRef.current?.abort();
        return;
      }
      if (Date.now() - lastRunRef.current >= intervalMs) void refresh();
      start();
    };

    if (document.visibilityState === "visible") start();
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      stop();
      document.removeEventListener("visibilitychange", onVisibilityChange);
      controllerRef.current?.abort();
    };
  }, [enabled, intervalMs, refresh]);

  return { data, error, isRefreshing, lastUpdatedAt, refresh };
}
