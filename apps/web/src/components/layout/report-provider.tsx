"use client";

import { useReportDataSource } from "./data-source";

/** Lets a server-rendered page tell the top bar which data provider answered (DEMO DATA pill). */
export function ReportProvider({ provider }: { provider: string | null }) {
  useReportDataSource(provider);
  return null;
}
