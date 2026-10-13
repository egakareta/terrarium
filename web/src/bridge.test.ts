// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { nativeLayout } from "./native-layout";

describe("native presentation layout", () => {
  it("exposes panel and viewport geometry in scene-local CSS coordinates", () => {
    document.body.innerHTML =
      '<div><canvas id="scene"></canvas><div class="ui-layer"><aside></aside><div class="floating-window"><canvas data-terrarium-viewport="42"></canvas></div></div></div>';
    const scene = document.getElementById("scene") as HTMLCanvasElement;
    const panel = document.querySelector("aside")!;
    const view = document.querySelector("[data-terrarium-viewport]")!;
    const rect = (x: number, y: number, width: number, height: number) =>
      ({ left: x, top: y, width, height }) as DOMRect;
    scene.getBoundingClientRect = () => rect(10, 20, 800, 600);
    panel.getBoundingClientRect = () => rect(10, 20, 220, 600);
    view.getBoundingClientRect = () => rect(300, 100, 256, 256);
    const window = view.parentElement!;
    window.getBoundingClientRect = () => rect(280, 80, 300, 320);
    window.style.overflow = "visible";
    expect(nativeLayout(scene).occlusions[0]).toEqual([0, 0, 220, 600]);
    expect(nativeLayout(scene).viewports[0]).toEqual({
      key: expect.any(String),
      id: "42",
      bounds: [290, 80, 256, 256],
      clip: [290, 80, 256, 256],
      occlusions: [],
    });
  });
});
