import { SearchX } from "lucide-react";

import { ButtonLink } from "@/components/ui/button";
import { Card } from "@/components/ui/card";

export default function StockNotFound() {
  return (
    <Card className="mx-auto mt-8 flex max-w-lg flex-col items-center px-6 py-14 text-center">
      <SearchX aria-hidden="true" className="size-9 text-subtle" />
      <h1 className="mt-4 text-lg font-semibold">Symbol not found</h1>
      <p className="mt-1 text-sm text-muted">
        We could not find that stock. Check the NSE symbol (for example RELIANCE or M&amp;M) or pick
        one from the stock list. Press Ctrl+K to search.
      </p>
      <ButtonLink href="/stocks" variant="primary" className="mt-6">
        Go to stock list
      </ButtonLink>
    </Card>
  );
}
