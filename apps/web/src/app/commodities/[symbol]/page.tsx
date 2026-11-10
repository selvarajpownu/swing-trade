import {
  StockDetailPage,
  stockMetadata,
  type StockRouteProps,
} from "@/components/pages/stock-detail-page";

export const dynamic = "force-dynamic";

export const generateMetadata = stockMetadata;

export default function CommodityPage(props: StockRouteProps) {
  return <StockDetailPage market="mcx" {...props} />;
}
