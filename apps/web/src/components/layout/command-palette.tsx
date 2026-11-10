"use client";

import { useRouter } from "next/navigation";
import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import { CornerDownLeft, Search } from "lucide-react";

import { fetchStocks, stocksUrl } from "@/lib/api/client";
import { parseSymbol } from "@/lib/api/params";
import { stockHref } from "@/lib/links";
import { marketForExchange } from "@/lib/markets";
import { cn } from "@/lib/utils";

import { SymbolAvatar } from "@/components/ui/symbol-avatar";

import { useDataSource, useDataSourceActions, type SymbolEntry } from "./data-source";

const MAX_RESULTS = 50;

/** Ctrl+K / Cmd+K: jump to a symbol from the current watchlist. */
export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const restoreFocusRef = useRef<HTMLElement | null>(null);

  const openPalette = useCallback(() => {
    restoreFocusRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setOpen(true);
  }, []);

  const closePalette = useCallback(() => {
    setOpen(false);
    // Return focus to where the user was.
    const target = restoreFocusRef.current;
    restoreFocusRef.current = null;
    if (target) requestAnimationFrame(() => target.focus());
  }, []);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        if (open) closePalette();
        else openPalette();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, openPalette, closePalette]);

  return (
    <>
      <button
        type="button"
        onClick={openPalette}
        aria-haspopup="dialog"
        aria-keyshortcuts="Control+K Meta+K"
        className="flex h-9 items-center gap-2 rounded-lg border border-line bg-surface px-3 text-sm text-muted transition-colors hover:border-line-strong hover:text-fg md:w-72"
      >
        <Search aria-hidden="true" className="size-4" />
        <span className="hidden sm:inline">Jump to symbol…</span>
        <span className="sr-only sm:hidden">Jump to symbol</span>
        <kbd className="ml-auto hidden rounded border border-line-strong bg-surface-2 px-1.5 py-0.5 font-sans text-[10px] text-subtle md:inline">
          Ctrl K
        </kbd>
      </button>
      {open ? <PaletteDialog onClose={closePalette} /> : null}
    </>
  );
}

interface Option {
  key: string;
  symbol: string;
  name: string;
  hint: string;
}

function rank(entry: SymbolEntry, needle: string): number {
  const symbol = entry.symbol.toLowerCase();
  const name = entry.name.toLowerCase();
  if (symbol === needle) return 0;
  if (symbol.startsWith(needle)) return 1;
  if (name.startsWith(needle)) return 2;
  if (symbol.includes(needle) || name.includes(needle)) return 3;
  return -1;
}

function PaletteDialog({ onClose }: { onClose: () => void }) {
  const router = useRouter();
  const { symbols } = useDataSource();
  const { setSymbols, setProvider } = useDataSourceActions();

  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const [loading, setLoading] = useState(symbols.length === 0);
  const [loadFailed, setLoadFailed] = useState(false);

  const titleId = useId();
  const listId = useId();
  const optionId = (index: number) => `${listId}-opt-${index}`;
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  // Load the watchlist if no page has reported it yet.
  const needsLoad = symbols.length === 0;
  useEffect(() => {
    if (!needsLoad) return;
    const controller = new AbortController();
    fetchStocks(stocksUrl(), controller.signal)
      .then((data) => {
        setSymbols(
          data.items.map((item) => ({
            symbol: item.symbol,
            name: item.name,
            exchange: item.exchange,
          })),
        );
        setProvider(data.provider);
        setLoading(false);
      })
      .catch(() => {
        if (controller.signal.aborted) return;
        setLoadFailed(true);
        setLoading(false);
      });
    return () => controller.abort();
  }, [needsLoad, setSymbols, setProvider]);

  // Lock page scroll while open.
  useEffect(() => {
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    inputRef.current?.focus();
    return () => {
      document.body.style.overflow = previous;
    };
  }, []);

  const options = useMemo<Option[]>(() => {
    const needle = query.trim().toLowerCase();
    const matches = needle
      ? symbols
          .map((entry) => ({ entry, score: rank(entry, needle) }))
          .filter((m) => m.score >= 0)
          .sort((a, b) => a.score - b.score || a.entry.symbol.localeCompare(b.entry.symbol))
          .map((m) => m.entry)
      : symbols;

    const result: Option[] = matches.slice(0, MAX_RESULTS).map((entry) => ({
      key: entry.symbol,
      symbol: entry.symbol,
      name: entry.name,
      hint: entry.exchange,
    }));

    // Allow jumping to any valid symbol, even if it is not in the watchlist.
    const typed = parseSymbol(query);
    if (typed && !result.some((o) => o.symbol === typed)) {
      result.push({ key: `typed-${typed}`, symbol: typed, name: `Open ${typed}`, hint: "Symbol" });
    }
    return result;
  }, [query, symbols]);

  const safeIndex = options.length === 0 ? -1 : Math.min(activeIndex, options.length - 1);

  useEffect(() => {
    if (safeIndex < 0) return;
    const el = listRef.current?.querySelector<HTMLElement>(`[data-index="${safeIndex}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [safeIndex]);

  const go = (option: Option | undefined) => {
    if (!option) return;
    onClose();
    // The hint is the exchange, which tells which market's page to open.
    router.push(stockHref(option.symbol, undefined, undefined, marketForExchange(option.hint)));
  };

  const onKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        if (options.length > 0) setActiveIndex((safeIndex + 1) % options.length);
        break;
      case "ArrowUp":
        event.preventDefault();
        if (options.length > 0) setActiveIndex((safeIndex - 1 + options.length) % options.length);
        break;
      case "Enter":
        event.preventDefault();
        go(options[safeIndex]);
        break;
      case "Escape":
        event.preventDefault();
        onClose();
        break;
      case "Tab":
        // The input is the only focusable element: keep focus inside the dialog.
        event.preventDefault();
        break;
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center px-4 pt-[12vh]">
      <div aria-hidden="true" className="absolute inset-0 bg-black/60" onClick={onClose} />
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="relative w-full max-w-lg animate-fade-in overflow-hidden rounded-2xl border border-line-strong bg-surface-2 shadow-2xl shadow-black/50"
      >
        <h2 id={titleId} className="sr-only">
          Jump to symbol
        </h2>
        <div className="flex items-center gap-2 border-b border-line px-4">
          <Search aria-hidden="true" className="size-4 text-subtle" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setActiveIndex(0);
            }}
            onKeyDown={onKeyDown}
            role="combobox"
            aria-expanded="true"
            aria-controls={listId}
            aria-autocomplete="list"
            aria-activedescendant={safeIndex >= 0 ? optionId(safeIndex) : undefined}
            placeholder="Search name or symbol (e.g. RELIANCE)"
            autoComplete="off"
            spellCheck={false}
            className="h-12 flex-1 bg-transparent text-sm text-fg placeholder:text-subtle focus:outline-none"
          />
          <kbd className="rounded border border-line-strong px-1.5 py-0.5 text-[10px] text-subtle">
            Esc
          </kbd>
        </div>

        <ul
          ref={listRef}
          id={listId}
          role="listbox"
          aria-label="Symbols"
          className="max-h-80 overflow-y-auto p-2"
        >
          {options.map((option, index) => (
            <li
              key={option.key}
              id={optionId(index)}
              data-index={index}
              role="option"
              aria-selected={index === safeIndex}
              onMouseMove={() => index !== safeIndex && setActiveIndex(index)}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => go(option)}
              className={cn(
                "flex cursor-pointer items-center gap-3 rounded-lg px-3 py-2",
                index === safeIndex ? "bg-surface-3" : "hover:bg-surface-3/60",
              )}
            >
              <SymbolAvatar symbol={option.symbol} name={option.name} size="sm" />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-medium text-fg">{option.name}</span>
                <span className="block text-xs text-muted">{option.symbol}</span>
              </span>
              <span className="text-[10px] text-subtle uppercase">{option.hint}</span>
              {index === safeIndex ? (
                <CornerDownLeft aria-hidden="true" className="size-3.5 text-subtle" />
              ) : null}
            </li>
          ))}
        </ul>

        {options.length === 0 ? (
          <p className="px-4 pb-4 text-sm text-muted" role="status">
            {loading
              ? "Loading watchlist…"
              : loadFailed
                ? "Could not load the watchlist. Type a full symbol to open it."
                : "No matching symbols."}
          </p>
        ) : null}
      </div>
    </div>
  );
}
