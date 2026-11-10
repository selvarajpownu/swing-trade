"use client";

import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

/**
 * App-wide knowledge about the data being shown:
 * - `provider`: drives the "DEMO DATA" pill in the top bar.
 * - `symbols`: the current watchlist, used by the Ctrl+K command palette.
 *
 * Client views report what they loaded via useReportDataSource.
 * State and setters live in separate contexts so reporters do not re-render
 * when the state changes.
 */

export interface SymbolEntry {
  symbol: string;
  name: string;
  exchange: string;
}

interface DataSourceState {
  provider: string | null;
  symbols: SymbolEntry[];
}

interface DataSourceActions {
  setProvider: (provider: string | null) => void;
  setSymbols: (symbols: SymbolEntry[]) => void;
}

const StateContext = createContext<DataSourceState>({ provider: null, symbols: [] });
const ActionsContext = createContext<DataSourceActions | null>(null);

export function DataSourceProvider({ children }: { children: ReactNode }) {
  const [provider, setProvider] = useState<string | null>(null);
  const [symbols, setSymbols] = useState<SymbolEntry[]>([]);

  const state = useMemo(() => ({ provider, symbols }), [provider, symbols]);
  const actions = useMemo(() => ({ setProvider, setSymbols }), []);

  return (
    <ActionsContext.Provider value={actions}>
      <StateContext.Provider value={state}>{children}</StateContext.Provider>
    </ActionsContext.Provider>
  );
}

export function useDataSource(): DataSourceState {
  return useContext(StateContext);
}

export function useDataSourceActions(): DataSourceActions {
  const actions = useContext(ActionsContext);
  if (!actions) throw new Error("useDataSourceActions must be used inside <DataSourceProvider>");
  return actions;
}

/** Reports the provider (and optionally the watchlist) of the data on screen. */
export function useReportDataSource(provider: string | null | undefined, symbols?: SymbolEntry[]) {
  const { setProvider, setSymbols } = useDataSourceActions();

  useEffect(() => {
    setProvider(provider ?? null);
  }, [provider, setProvider]);

  useEffect(() => {
    if (symbols && symbols.length > 0) setSymbols(symbols);
  }, [symbols, setSymbols]);
}
