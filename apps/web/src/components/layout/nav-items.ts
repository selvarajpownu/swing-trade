import { Bookmark, House, List, NotebookPen, Settings, type LucideIcon } from "lucide-react";

export interface NavItem {
  href: string;
  label: string;
  /** Shorter label for the mobile tab bar. */
  shortLabel: string;
  icon: LucideIcon;
  /** Active when the pathname equals href (true) or starts with it (false). */
  exact?: boolean;
}

export const NAV_ITEMS: readonly NavItem[] = [
  { href: "/", label: "Home", shortLabel: "Home", icon: House, exact: true },
  { href: "/stocks", label: "Stock list", shortLabel: "Stocks", icon: List },
  { href: "/watchlists", label: "Watchlists", shortLabel: "Watchlists", icon: Bookmark },
  { href: "/journal", label: "Journal", shortLabel: "Journal", icon: NotebookPen },
  { href: "/settings", label: "Settings", shortLabel: "Settings", icon: Settings },
];

export function isNavItemActive(item: NavItem, pathname: string): boolean {
  if (item.exact) return pathname === item.href;
  return pathname === item.href || pathname.startsWith(`${item.href}/`);
}
