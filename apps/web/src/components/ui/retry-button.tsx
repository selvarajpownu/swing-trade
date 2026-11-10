"use client";

import { useRouter } from "next/navigation";
import { useTransition } from "react";
import { RefreshCw } from "lucide-react";

import { cn } from "@/lib/utils";

import { Button } from "./button";

/** Re-runs the current route's server components. */
export function RetryButton({ label = "Try again" }: { label?: string }) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();
  return (
    <Button onClick={() => startTransition(() => router.refresh())} disabled={pending}>
      <RefreshCw
        aria-hidden="true"
        className={cn("size-4", pending && "motion-safe:animate-spin")}
      />
      {label}
    </Button>
  );
}
