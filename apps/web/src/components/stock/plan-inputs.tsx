"use client";

import { usePathname, useRouter } from "next/navigation";
import { useId, useState, useTransition, type FormEvent } from "react";
import { Calculator } from "lucide-react";

import {
  DEFAULT_CAPITAL,
  DEFAULT_RISK_PCT,
  MAX_CAPITAL,
  MAX_RISK_PCT,
  MIN_CAPITAL,
  MIN_RISK_PCT,
  parseCapital,
  parseRiskPct,
  type PlanParams,
} from "@/lib/api/params";
import type { Interval } from "@/lib/api/types";
import { cn } from "@/lib/utils";

import { Button } from "@/components/ui/button";
import { Card, CardBody, CardHeader } from "@/components/ui/card";

interface PlanInputsProps {
  plan: PlanParams;
  interval: Interval;
}

/**
 * Capital and risk % inputs. Submitting updates the `capital` and `risk`
 * search params; the server re-fetches the plan from the backend.
 * Works as a plain GET form without JavaScript.
 */
export function PlanInputs({ plan, interval }: PlanInputsProps) {
  const router = useRouter();
  const pathname = usePathname();
  const [pending, startTransition] = useTransition();
  const [capital, setCapital] = useState(String(plan.capital));
  const [risk, setRisk] = useState(String(plan.riskPct));
  const capitalId = useId();
  const riskId = useId();

  const dirty = parseCapital(capital) !== plan.capital || parseRiskPct(risk) !== plan.riskPct;

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const next = { capital: parseCapital(capital), riskPct: parseRiskPct(risk) };
    setCapital(String(next.capital));
    setRisk(String(next.riskPct));

    const query = new URLSearchParams();
    if (interval !== "1d") query.set("interval", interval);
    if (next.capital !== DEFAULT_CAPITAL) query.set("capital", String(next.capital));
    if (next.riskPct !== DEFAULT_RISK_PCT) query.set("risk", String(next.riskPct));
    const qs = query.toString();
    startTransition(() => {
      router.push(qs ? `${pathname}?${qs}` : pathname, { scroll: false });
    });
  };

  const inputClass =
    "h-9 w-full rounded-lg border border-line bg-surface-2 px-3 text-sm text-fg tabular-nums focus:border-accent focus:outline-none";

  return (
    <Card aria-labelledby="sizing-title">
      <CardHeader
        title="Position sizing"
        titleId="sizing-title"
        description="Recomputes the plan's quantity on the server."
      />
      <CardBody>
        <form
          method="get"
          onSubmit={onSubmit}
          className="grid grid-cols-[1fr_6.5rem] items-end gap-2"
        >
          {interval !== "1d" ? <input type="hidden" name="interval" value={interval} /> : null}
          <div>
            <label htmlFor={capitalId} className="mb-1 block text-xs text-muted">
              Capital (₹)
            </label>
            <input
              id={capitalId}
              name="capital"
              type="number"
              inputMode="numeric"
              min={MIN_CAPITAL}
              max={MAX_CAPITAL}
              step={1000}
              required
              value={capital}
              onChange={(e) => setCapital(e.target.value)}
              className={inputClass}
            />
          </div>
          <div>
            <label htmlFor={riskId} className="mb-1 block text-xs text-muted">
              Risk per trade (%)
            </label>
            <input
              id={riskId}
              name="risk"
              type="number"
              inputMode="decimal"
              min={MIN_RISK_PCT}
              max={MAX_RISK_PCT}
              step={0.1}
              required
              value={risk}
              onChange={(e) => setRisk(e.target.value)}
              className={inputClass}
            />
          </div>
          <Button
            type="submit"
            variant={dirty ? "primary" : "secondary"}
            disabled={pending}
            className="col-span-2 mt-1"
          >
            <Calculator
              aria-hidden="true"
              className={cn("size-4", pending && "motion-safe:animate-pulse")}
            />
            {pending ? "Recalculating…" : "Recalculate plan"}
          </Button>
        </form>
      </CardBody>
    </Card>
  );
}
