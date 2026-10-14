import { nativeLayout } from "./native-layout";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { assetBase, exampleFromLocation } from "./routing";

export interface HostInfo {
  ui: string;
  bundled_fonts: boolean;
}
export interface PlatformOutput {
  cursor_icon: string;
  commands: { CopyText?: string; OpenUrl?: { url: string; new_tab: boolean } }[];
  ime?: { cursor_rect: { min: { x: number; y: number }; max: { x: number; y: number } } } | null;
  accesskit_update?: unknown;
}
export interface FrameOutput {
  fps: number;
  pixels_per_point: number;
  gui_regions: [number, number, number, number][];
  gui_input_regions: [number, number, number, number][];
  close: boolean;
  platform: PlatformOutput;
}
export interface TouchEvent {
  device: number;
  id: number;
  phase: "Start" | "Move" | "End" | "Cancel";
  position: [number, number];
  captured: boolean;
}
export interface FrameInput {
  gui_events: unknown[];
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
  info: HostInfo;
  advance(input: FrameInput): Promise<FrameOutput>;
  state(): Promise<unknown>;
  command(command: string, payload?: unknown): Promise<unknown>;
  viewport(id: string): Promise<Uint8Array | undefined>;
  mountViewport?(id: string, canvas: HTMLCanvasElement): (() => void) | undefined;
  dispose(): void;
}
interface WasmBridge {
  mount_gui(canvas: HTMLCanvasElement): void;
  backend(): string;
  software_adapter(): boolean;
  wait_for_gpu(): Promise<void>;
  info(): HostInfo;
  fps(): number;
  enable_accessibility(): void;
  platform_output(): PlatformOutput;
  render(input: FrameInput): FrameOutput;
  state(): unknown;
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

export async function connect(
  canvas: HTMLCanvasElement,
  overlay: HTMLCanvasElement,
): Promise<Transport> {
  if (isTauri()) {
    return {
      native: true,
      info: await invoke<HostInfo>("terrarium_info"),
      advance: (input) =>
        invoke<FrameOutput>("terrarium_frame", {
          input,
          layout: nativeLayout(canvas),
        }),
      state: () => invoke("terrarium_state"),
      command: (command, payload = null) => invoke("terrarium_command", { command, payload }),
      viewport: async () => undefined,
      dispose() {},
    };
  }
  if (!window.__TERRARIUM__) {
    const example = exampleFromLocation() ?? "app";
    if (!/^[a-z_]+$/.test(example)) throw new Error("Invalid example name");
    const readiness = new Promise<void>((resolve, reject) => {
      const ready = () => {
        window.removeEventListener("terrarium-ready", ready);
        window.__TERRARIUM_ERROR__ ? reject(new Error(window.__TERRARIUM_ERROR__)) : resolve();
      };
      window.addEventListener("terrarium-ready", ready);
    });
    const module = await import(/* @vite-ignore */ new URL(`wasm/${example}.js`, assetBase()).href);
    await module.default();
    await readiness;
  }
  const bridge = window.__TERRARIUM__;
  if (!bridge) throw new Error("WASM renderer did not provide an interop bridge");
  if (bridge.backend() === "Gl") canvas.classList.add("combined-canvas");
  else bridge.mount_gui(overlay);
  let disposed = false;
  return {
    native: false,
    info: bridge.info(),
    advance: async (input) => {
      const output = bridge.render(input);
      if (bridge.software_adapter()) await bridge.wait_for_gpu();
      return output;
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
