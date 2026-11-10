import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";

export function StockListSkeleton({ rows = 8 }: { rows?: number }) {
  return (
    <div className="space-y-4" aria-busy="true" aria-live="polite">
      <span className="sr-only">Loading stocks…</span>
      <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
        <Skeleton className="h-10 w-full lg:max-w-xs" />
        <div className="flex gap-2 overflow-hidden">
          {Array.from({ length: 7 }, (_, i) => (
            <Skeleton key={i} className="h-7 w-20 shrink-0 rounded-full" />
          ))}
        </div>
      </div>
      <Skeleton className="h-4 w-56" />
      <Card className="overflow-hidden">
        <div className="hidden border-b border-line px-4 py-3 md:flex md:gap-8">
          {Array.from({ length: 8 }, (_, i) => (
            <Skeleton key={i} className="h-3 w-20" />
          ))}
        </div>
        <ul className="divide-y divide-line">
          {Array.from({ length: rows }, (_, i) => (
            <li key={i} className="flex items-center gap-3 px-4 py-3.5">
              <Skeleton className="size-9 shrink-0 rounded-full" />
              <div className="flex-1 space-y-2">
                <Skeleton className="h-3.5 w-40" />
                <Skeleton className="h-3 w-20" />
              </div>
              <div className="hidden gap-8 md:flex">
                <Skeleton className="h-3.5 w-24" />
                <Skeleton className="h-3.5 w-24" />
                <Skeleton className="h-3.5 w-28" />
                <Skeleton className="h-5 w-24 rounded-full" />
                <Skeleton className="h-3.5 w-14" />
              </div>
              <div className="space-y-2 md:hidden">
                <Skeleton className="ml-auto h-3.5 w-20" />
                <Skeleton className="ml-auto h-3 w-24" />
              </div>
            </li>
          ))}
        </ul>
      </Card>
    </div>
  );
}
