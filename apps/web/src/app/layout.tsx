import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";

import { APP_NAME } from "@/lib/utils";

import { DataSourceProvider } from "@/components/layout/data-source";
import { FooterNote } from "@/components/layout/footer-note";
import { MobileNav } from "@/components/layout/mobile-nav";
import { Sidebar } from "@/components/layout/sidebar";
import { TopBar } from "@/components/layout/top-bar";

import "./globals.css";

export const metadata: Metadata = {
  title: {
    default: APP_NAME,
    template: `%s · ${APP_NAME}`,
  },
  description:
    "Swing-trading analysis for Indian stocks: setups, trade plans, key levels. View only, never places orders.",
  applicationName: APP_NAME,
  robots: { index: false, follow: false },
};

export const viewport: Viewport = {
  themeColor: "#0a0d12",
  colorScheme: "dark",
  width: "device-width",
  initialScale: 1,
  viewportFit: "cover",
};

export default function RootLayout({ children }: Readonly<{ children: ReactNode }>) {
  return (
    <html lang="en-IN" className="dark">
      <body>
        <a
          href="#main"
          className="sr-only focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-[60] focus:rounded-lg focus:bg-accent focus:px-3 focus:py-2 focus:text-sm focus:text-white"
        >
          Skip to content
        </a>
        <DataSourceProvider>
          <div className="flex min-h-dvh">
            <Sidebar />
            <div className="flex min-w-0 flex-1 flex-col pb-[calc(4rem+env(safe-area-inset-bottom))] md:pb-0">
              <TopBar />
              <main
                id="main"
                tabIndex={-1}
                className="w-full max-w-[1440px] flex-1 px-4 py-5 focus:outline-none md:px-6 lg:px-8"
              >
                {children}
              </main>
              <FooterNote />
            </div>
          </div>
          <MobileNav />
        </DataSourceProvider>
      </body>
    </html>
  );
}
