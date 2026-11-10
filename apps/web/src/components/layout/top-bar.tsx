import Link from "next/link";

import { APP_NAME } from "@/lib/utils";

import { AppLogo } from "./app-logo";
import { CommandPalette } from "./command-palette";
import { DemoBadge } from "./demo-badge";

export function TopBar() {
  return (
    <header className="sticky top-0 z-20 flex h-14 items-center gap-3 border-b border-line bg-canvas/90 px-4 backdrop-blur md:px-6">
      <Link href="/stocks" className="flex items-center gap-2 font-semibold md:hidden">
        <AppLogo className="size-6" />
        <span className="text-sm">{APP_NAME}</span>
      </Link>
      <div className="flex flex-1 items-center justify-end gap-3 md:justify-between">
        <CommandPalette />
        <DemoBadge />
      </div>
    </header>
  );
}
