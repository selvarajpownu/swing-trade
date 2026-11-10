import {
  Bookmark,
  DollarSign,
  Gem,
  House,
  IndianRupee,
  Newspaper,
  NotebookPen,
  Radar,
  Settings,
  type LucideIcon,
} from "lucide-react";

export interface NavItem {
  href: string;
  label: string;
  /** Shorter label for the mobile tab bar. */
  shortLabel: string;
  icon: LucideIcon;
  /** Active when the pathname equals href (true) or starts with it (false). */
  exact?: boolean;
  /** Shown in the five-slot mobile tab bar. */
  mobile?: boolean;
}

export const NAV_ITEMS: readonly NavItem[] = [
  { href: "/", label: "Home", shortLabel: "Home", icon: House, exact: true, mobile: true },
  { href: "/stocks", label: "IN Stock", shortLabel: "IN", icon: IndianRupee, mobile: true },
  { href: "/us", label: "US Stock", shortLabel: "US", icon: DollarSign, mobile: true },
  { href: "/commodities", label: "Commodity", shortLabel: "MCX", icon: Gem, mobile: true },
  { href: "/news", label: "Stock news", shortLabel: "News", icon: Newspaper, mobile: true },
  { href: "/scan", label: "Scanner", shortLabel: "Scanner", icon: Radar },
  { href: "/watchlists", label: "Watchlists", shortLabel: "Watchlists", icon: Bookmark },
  { href: "/journal", label: "Journal", shortLabel: "Journal", icon: NotebookPen },
  { href: "/settings", label: "Settings", shortLabel: "Settings", icon: Settings },
];

/** The items of the mobile tab bar (five slots). */
export const MOBILE_NAV_ITEMS: readonly NavItem[] = NAV_ITEMS.filter((item) => item.mobile);

export function isNavItemActive(item: NavItem, pathname: string): boolean {
  if (item.exact) return pathname === item.href;
  return pathname === item.href || pathname.startsWith(`${item.href}/`);
}
