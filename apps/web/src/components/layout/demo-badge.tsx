"use client";

import { useDataSource } from "./data-source";

/** "DEMO DATA" pill, shown while the data on screen comes from the demo provider. */
export function DemoBadge() {
  const { provider } = useDataSource();
  if (provider !== "demo") return null;
  return (
    <span
      className="inline-flex items-center rounded-full border border-warn/40 bg-warn/10 px-2.5 py-0.5 text-[10px] font-bold tracking-wider text-warn"
      title="Prices come from the built-in demo provider, not a live market feed."
    >
      DEMO DATA
    </span>
  );
}
