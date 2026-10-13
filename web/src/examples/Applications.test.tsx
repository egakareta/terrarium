// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MantineProvider } from "@mantine/core";
import { EngineContext } from "../context";
import { applications } from "./Applications";
import { WidgetGallery } from "./Widgets";
import { Devtools } from "../components/Devtools";
import type { Snapshot } from "../bridge";

beforeAll(() => {
  Object.defineProperty(window, "matchMedia", {
    value: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
  });
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});
afterEach(cleanup);
function mount(name: string, app: unknown, devtools: Snapshot["devtools"] = null) {
  const commands: { name: string; payload: unknown }[] = [];
  const snapshot: Snapshot = {
    ui: name,
    app,
    fps: 60,
    close: false,
    bundled_fonts: true,
    touch_enabled: false,
    touch_movement: [0, 0],
    devtools,
  };
  const Application = applications[name];
  render(
    <MantineProvider>
      <EngineContext.Provider
        value={{
          snapshot,
          command: async (name, payload) => {
            commands.push({ name, payload });
          },
          transport: {} as never,
        }}
      >
        <Application />
        <Devtools />
      </EngineContext.Provider>
    </MantineProvider>,
  );
  return commands;
}
describe("example controls", () => {
  it("exposes physics actions and live telemetry", async () => {
    const commands = mount("physics", { bodies: 20, gravity: [0, -9.8, 0] });
    expect(screen.getByText("Dynamic bodies: 20")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Kick stack" }));
    await userEvent.click(screen.getByRole("button", { name: "Reset stack" }));
    expect(commands.map((v) => v.name)).toEqual(["kick", "reset"]);
  });
  it("changes outline mode and camera focus", async () => {
    const commands = mount("outlines", { mode: "Stencil", width: 2, focus: true });
    await userEvent.click(screen.getByText("Toon"));
    await userEvent.click(screen.getByRole("checkbox", { name: "Focus on duck" }));
    expect(commands).toEqual([
      { name: "mode", payload: "Toon" },
      { name: "focus", payload: false },
    ]);
  });
  it("keeps the development notice visible while the inspector is hidden", () => {
    mount("simple", null, { visible: false, tree: null, selected: [], properties: null });
    expect(screen.getByText("Development build: Press F12 to enable DevTools")).toBeTruthy();
  });
  it("disables the widget leaf using its checkbox", async () => {
    render(
      <MantineProvider>
        <WidgetGallery name="taffy_egui_widgets" />
      </MantineProvider>,
    );
    const button = screen.getByRole("button", { name: "A widget leaf" }) as HTMLButtonElement;
    expect(button.disabled).toBe(false);
    await userEvent.click(screen.getByRole("checkbox", { name: "Enabled" }));
    expect(button.disabled).toBe(true);
  });
});
