import {
  StockDetailPage,
  stockMetadata,
  type StockRouteProps,
} from "@/components/pages/stock-detail-page";

export const dynamic = "force-dynamic";

export const generateMetadata = stockMetadata;

export default function UsStockPage(props: StockRouteProps) {
  return <StockDetailPage market="us" {...props} />;
}
