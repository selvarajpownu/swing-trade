import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";

export default function StockLoading() {
  return (
    <div className="space-y-5" aria-busy="true">
      <span className="sr-only" role="status">
        Loading stock…
      </span>
      <Skeleton className="h-3 w-20" />
      <div className="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
        <div className="flex items-start gap-3">
          <Skeleton className="size-12 rounded-full" />
          <div className="space-y-2">
            <Skeleton className="h-6 w-48" />
            <Skeleton className="h-4 w-28" />
            <Skeleton className="h-7 w-56" />
          </div>
        </div>
        <Skeleton className="h-16 w-full rounded-2xl lg:w-[520px]" />
      </div>

      <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_360px]">
        <Card className="p-4">
          <div className="flex justify-between">
            <Skeleton className="h-4 w-28" />
            <Skeleton className="h-7 w-32 rounded-lg" />
          </div>
          <Skeleton className="mt-4 h-[360px] w-full rounded-xl sm:h-[440px] lg:h-[500px]" />
        </Card>
        <div className="space-y-5">
          <Card className="space-y-5 p-4">
            <Skeleton className="h-4 w-24" />
            {Array.from({ length: 3 }, (_, i) => (
              <div key={i} className="flex items-center gap-3">
                <Skeleton className="size-9 rounded-xl" />
                <Skeleton className="h-4 flex-1" />
                <Skeleton className="h-4 w-20" />
              </div>
            ))}
            <div className="grid grid-cols-2 gap-2">
              {Array.from({ length: 4 }, (_, i) => (
                <Skeleton key={i} className="h-14 rounded-xl" />
              ))}
            </div>
          </Card>
          <Card className="space-y-3 p-4">
            <Skeleton className="h-4 w-24" />
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-2/3" />
          </Card>
        </div>
      </div>
    </div>
  );
}
