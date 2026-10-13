import { nativeLayout } from "./native-layout";
import { invoke, isTauri } from "@tauri-apps/api/core";

export interface PropertyValue {
  Bool?: boolean;
  Number?: number;
  Text?: string;
  Json?: string;
  Vector?: number[];
  Color?: number[];
  Choice?: { selected: string; options: string[] };
}
export interface SceneNode {
  id: string;
  name: string;
  class: string;
  children: SceneNode[];
}
export interface InspectorState {
  visible: boolean;
  tree: SceneNode | null;
  selected: string[];
  properties:
    | { property: { path: string; value: PropertyValue; editable: boolean }; mixed: boolean }[]
    | null;
}
export interface Snapshot {
  ui: string;
  app: unknown;
  fps: number;
  close: boolean;
  bundled_fonts: boolean;
  touch_geometry?: [number, number, number] | null;
  touch_enabled: boolean;
  touch_movement: [number, number];
  devtools: InspectorState | null;
}
export interface TouchEvent {
  device: number;
  id: number;
  phase: "Start" | "Move" | "End" | "Cancel";
  position: [number, number];
  captured: boolean;
}
export interface FrameInput {
  delta: number;
  size: [number, number];
  scale: number;
  focused: boolean;
  keys: string[];
  pressed: string[];
  buttons: number[];
  mouse_delta: [number, number];
  modifiers: { ctrl: boolean; command: boolean; shift: boolean };
  pointer_captured: boolean;
  keyboard_captured: boolean;
  touches: TouchEvent[];
  touchscreen: boolean;
}
export interface Transport {
  native: boolean;
  frame(input: FrameInput): Promise<Snapshot>;
  state(): Promise<Snapshot>;
  command(command: string, payload?: unknown): Promise<unknown>;
  viewport(id: string): Promise<Uint8Array | undefined>;
  mountViewport?(id: string, canvas: HTMLCanvasElement): (() => void) | undefined;
  dispose(): void;
}
interface WasmBridge {
  backend(): string;
  wait_for_gpu(): Promise<void>;
  frame(input: FrameInput): Snapshot;
  state(): Snapshot;
  command(command: string, payload: unknown): unknown;
  viewport(id: string): Uint8Array | undefined;
  mount_viewport(id: string, canvas: HTMLCanvasElement): number | undefined;
  unmount_viewport(key: number): void;
  free(): void;
}
declare global {
  interface Window {
    __TERRARIUM__?: WasmBridge;
    __TERRARIUM_ERROR__?: string;
  }
}

export async function connect(canvas: HTMLCanvasElement): Promise<Transport> {
  if (isTauri()) {
    return {
      native: true,
      frame: (input) =>
        invoke<Snapshot>("terrarium_frame", { input, layout: nativeLayout(canvas) }),
      state: () => invoke<Snapshot>("terrarium_state"),
      command: (command, payload = null) => invoke("terrarium_command", { command, payload }),
      viewport: async () => undefined,
      dispose() {},
    };
  }
  if (!window.__TERRARIUM__) {
    const example = new URLSearchParams(location.search).get("example") ?? "app";
    if (!/^[a-z_]+$/.test(example)) throw new Error("Invalid example name");
    const readiness = new Promise<void>((resolve, reject) => {
      const ready = () => {
        window.removeEventListener("terrarium-ready", ready);
        window.__TERRARIUM_ERROR__ ? reject(new Error(window.__TERRARIUM_ERROR__)) : resolve();
      };
      window.addEventListener("terrarium-ready", ready);
    });
    const module = await import(
      /* @vite-ignore */ new URL(`./wasm/${example}.js`, document.baseURI).href
    );
    await module.default();
    await readiness;
  }
  const bridge = window.__TERRARIUM__;
  if (!bridge) throw new Error("WASM renderer did not provide an interop bridge");
  let disposed = false;
  return {
    native: false,
    frame: async (input) => {
      const snapshot = bridge.frame(input);
      await bridge.wait_for_gpu();
      return snapshot;
    },
    state: async () => bridge.state(),
    command: async (command, payload = null) => bridge.command(command, payload),
    viewport: async (id) => bridge.viewport(id),
    mountViewport(id, canvas) {
      const key = bridge.mount_viewport(id, canvas);
      return key === undefined
        ? undefined
        : () => {
            if (!disposed) bridge.unmount_viewport(key);
          };
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      bridge.free();
      if (window.__TERRARIUM__ === bridge) delete window.__TERRARIUM__;
    },
  };
}
