"use client";

import { useEffect } from "react";
import { RefreshCw, TriangleAlert } from "lucide-react";

import { Button, ButtonLink } from "@/components/ui/button";
import { Card } from "@/components/ui/card";

export default function GlobalRouteError({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  useEffect(() => {
    console.error(error);
  }, [error]);

  return (
    <Card
      role="alert"
      className="mx-auto mt-8 flex max-w-lg flex-col items-center px-6 py-14 text-center"
    >
      <TriangleAlert aria-hidden="true" className="size-9 text-down" />
      <h1 className="mt-4 text-lg font-semibold">Something went wrong</h1>
      <p className="mt-1 text-sm text-muted">
        This page hit an unexpected error. You can try again, or go back to the stock list.
      </p>
      {error.digest ? (
        <p className="mt-2 font-mono text-xs text-subtle">Reference: {error.digest}</p>
      ) : null}
      <div className="mt-6 flex gap-2">
        <Button variant="primary" onClick={() => reset()}>
          <RefreshCw aria-hidden="true" className="size-4" />
          Try again
        </Button>
        <ButtonLink href="/stocks">Stock list</ButtonLink>
      </div>
    </Card>
  );
}
