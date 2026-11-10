import { StockListSkeleton } from "@/components/stocks/stock-list-skeleton";
import { Skeleton } from "@/components/ui/skeleton";

export default function StocksLoading() {
  return (
    <>
      <div className="mb-5 space-y-2">
        <Skeleton className="h-6 w-32" />
        <Skeleton className="h-4 w-80 max-w-full" />
      </div>
      <StockListSkeleton />
    </>
  );
}
