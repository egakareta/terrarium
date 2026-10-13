import { createContext, useContext } from "react";
import type { Snapshot, Transport } from "./bridge";

export interface EngineContextValue {
  snapshot: Snapshot;
  command(command: string, payload?: unknown): Promise<void>;
  transport: Transport;
}
export const EngineContext = createContext<EngineContextValue | null>(null);
export function useEngine<T = unknown>() {
  const value = useContext(EngineContext);
  if (!value) throw new Error("A Terrarium component must be mounted inside EngineProvider");
  return { ...value, state: value.snapshot.app as T };
}
