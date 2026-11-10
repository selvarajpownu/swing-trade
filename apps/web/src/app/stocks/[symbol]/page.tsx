import {
  StockDetailPage,
  stockMetadata,
  type StockRouteProps,
} from "@/components/pages/stock-detail-page";

export const dynamic = "force-dynamic";

export const generateMetadata = stockMetadata;

export default function StockPage(props: StockRouteProps) {
  return <StockDetailPage market="in" {...props} />;
}
