import type { Zone } from "@/lib/api/types";
import { formatDistancePct, formatNumber } from "@/lib/format";
import { cn } from "@/lib/utils";

import { Badge } from "@/components/ui/badge";
import { Card, CardBody, CardHeader } from "@/components/ui/card";

interface KeyLevelsProps {
  zones: Zone[];
  price: number;
}

/** Support / resistance zones, nearest to price first (as sent by the API). */
export function KeyLevels({ zones, price }: KeyLevelsProps) {
  return (
    <Card aria-labelledby="levels-title">
      <CardHeader
        title="Key levels"
        titleId="levels-title"
        description="Support and resistance zones, nearest first."
      />
      <CardBody>
        {zones.length === 0 ? (
          <p className="text-sm text-muted">No clear zones in the available history.</p>
        ) : (
          <ul className="divide-y divide-line">
            {zones.map((zone, i) => {
              const mid = (zone.low + zone.high) / 2;
              const support = zone.kind === "support";
              return (
                <li
                  key={`${zone.kind}-${zone.low}-${zone.high}-${i}`}
                  className="flex items-center gap-3 py-2.5"
                >
                  <Badge tone={support ? "up" : "down"} className="w-[5.5rem] justify-center">
                    {support ? "Support" : "Resistance"}
                  </Badge>
                  <div className="min-w-0 flex-1">
                    <p className="text-sm font-medium tabular-nums">
                      {formatNumber(zone.low)} – {formatNumber(zone.high)}
                    </p>
                    <p className="text-[11px] text-subtle">
                      {zone.touches} {zone.touches === 1 ? "touch" : "touches"}
                    </p>
                  </div>
                  <span
                    className={cn("text-xs tabular-nums", support ? "text-up/90" : "text-down/90")}
                    title="Distance of the zone's midpoint from the current price"
                  >
                    {formatDistancePct(mid, price)}
                  </span>
                </li>
              );
            })}
          </ul>
        )}
      </CardBody>
    </Card>
  );
}
