"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

import { APP_NAME, cn } from "@/lib/utils";

import { AppLogo } from "./app-logo";
import { isNavItemActive, NAV_ITEMS } from "./nav-items";

/** Desktop navigation (md and up). */
export function Sidebar() {
  const pathname = usePathname();

  return (
    <aside className="sticky top-0 hidden h-dvh w-60 shrink-0 flex-col border-r border-line bg-surface md:flex">
      <Link
        href="/stocks"
        className="flex items-center gap-2.5 px-5 py-5 text-[15px] font-semibold tracking-tight"
      >
        <AppLogo />
        {APP_NAME}
      </Link>

      <nav aria-label="Main" className="flex-1 px-3">
        <ul className="space-y-0.5">
          {NAV_ITEMS.map((item) => {
            const active = isNavItemActive(item, pathname);
            const Icon = item.icon;
            return (
              <li key={item.href}>
                <Link
                  href={item.href}
                  aria-current={active ? "page" : undefined}
                  className={cn(
                    "flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors",
                    active
                      ? "bg-surface-3 font-medium text-fg"
                      : "text-muted hover:bg-surface-2 hover:text-fg",
                  )}
                >
                  <Icon aria-hidden="true" className="size-4" />
                  {item.label}
                </Link>
              </li>
            );
          })}
        </ul>
      </nav>

      <p className="px-5 py-4 text-[11px] leading-4 text-subtle">
        View and analysis only. This app never places orders.
      </p>
    </aside>
  );
}
