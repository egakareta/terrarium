import { readFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import { widgetNames } from "../src/examples/Widgets";

interface EguiNode {
  role: string;
  label?: string;
  value?: string;
  toggled?: string;
  bounds: { x0: number; y0: number; x1: number; y1: number };
}
async function eguiNodes(page: import("@playwright/test").Page): Promise<EguiNode[]> {
  return page.evaluate(() => {
    const output = window.__TERRARIUM__!.platform_output();
    const update = output.accesskit_update as
      | { nodes: [unknown, { role: string; properties: object }][] }
      | undefined;
    return (
      (update?.nodes.map(([, node]) => ({ role: node.role, ...node.properties })) as EguiNode[]) ??
      []
    );
  });
}
async function eguiHas(page: import("@playwright/test").Page, name: string) {
  return (await eguiNodes(page)).some((node) => node.label === name || node.value === name);
}
async function eguiNode(
  page: import("@playwright/test").Page,
  name: string,
  role?: string,
  exact = true,
) {
  const find = async () =>
    (await eguiNodes(page)).find(
      (node) =>
        (!role || node.role === role) &&
        [node.label, node.value].some((value) =>
          exact ? value === name : value?.includes(name),
        ) &&
        node.bounds,
    );
  await expect.poll(find, { message: `Egui widget ${name}` }).toBeDefined();
  return (await find())!;
}
async function eguiClick(
  page: import("@playwright/test").Page,
  name: string,
  role?: string,
  exact = true,
) {
  const node = await eguiNode(page, name, role, exact);
  await page.mouse.click(
    (node.bounds.x0 + node.bounds.x1) / 2,
    (node.bounds.y0 + node.bounds.y1) / 2,
    { delay: 100 },
  );
}

test.afterEach(async ({ page }, testInfo) => {
  if (testInfo.status === testInfo.expectedStatus || page.isClosed()) return;
  console.log(
    JSON.stringify({
      example: testInfo.title,
      alerts: await page.getByRole("alert").allTextContents(),
      state: await page.evaluate(() => window.__TERRARIUM__?.state()),
    }),
  );
});

const examples = [
  "app",
  "bevy",
  "gltf",
  "outlines",
  "physics",
  "simple",
  "sound",
  "viewport",
  "headless",
  "render_benchmark",
  "widgets",
];
for (const example of examples) {
  test(`${example} starts its WASM renderer and React UI`, async ({ page }, testInfo) => {
    const failures: string[] = [];
    page.on("pageerror", (error) => failures.push(error.message));
    await page.goto(
      `/${example}/${example === "render_benchmark" ? "?parts=32&warmup=1&frames=2&width=128&height=128" : ""}`,
    );
    await expect(page.getByRole("status")).toHaveCount(0, { timeout: 90_000 });
    await expect(page.getByRole("alert")).toHaveCount(0);
    const info = await page.evaluate(() => window.__TERRARIUM__!.info());
    const state = await page.evaluate(() => window.__TERRARIUM__!.state());
    expect(info.ui).toBe(example);
    if (example === "viewport") {
      expect(await page.evaluate(() => window.__TERRARIUM__!.backend())).toBe(
        testInfo.project.name === "webgl" ? "Gl" : "BrowserWebGpu",
      );
      await expect(page.getByText(/^Red pixels: \d+$/)).toBeVisible({ timeout: 30_000 });
      await expect(page.locator("[data-terrarium-viewport]")).toHaveCount(2);
      await expect
        .poll(
          async () =>
            page.evaluate(() => {
              const state = window.__TERRARIUM__!.state();
              const textures = (state as { textures: { id: string; size: [number, number] }[] })
                .textures;
              return textures.map((texture) => {
                const bytes = window.__TERRARIUM__!.viewport(texture.id);
                return bytes?.length ?? -1;
              });
            }),
          { timeout: 30_000 },
        )
        .toEqual(
          (state as { textures: { size: [number, number] }[] }).textures.map(
            (texture) => texture.size[0] * texture.size[1] * 4,
          ),
        );
      await expect(page.getByRole("alert")).toHaveCount(0);
    }
    if (example !== "headless" && example !== "render_benchmark") {
      await page.evaluate(() => window.__TERRARIUM__!.enable_accessibility());
      await page.keyboard.press("F12");
      await expect.poll(() => eguiHas(page, "DevTools")).toBe(true);
      await expect.poll(() => eguiHas(page, "Explorer")).toBe(true);
      await expect.poll(() => eguiHas(page, "Workspace (Workspace)")).toBe(true);
      await expect
        .poll(
          async () => {
            const explorer = await eguiNode(page, "Explorer");
            return page.evaluate(
              ({ x0, y0, x1, y1 }) =>
                document
                  .elementFromPoint((x0 + x1) / 2, (y0 + y1) / 2)
                  ?.hasAttribute("data-egui-region"),
              explorer.bounds,
            );
          },
          { timeout: 30000 },
        )
        .toBe(true);
    }
    expect(failures).toEqual([]);
  });
}
test("the examples hub opens and closes an interactive preview", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Terrarium examples 🌍" })).toBeVisible();
  await expect(page.getByText("the main example", { exact: true })).toBeVisible();
  await expect(page.getByText("a custom 3d model", { exact: true })).toBeVisible();
  await page.getByRole("link", { name: "open app", exact: true }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.getByRole("link", { name: "open full-size app" })).toHaveAttribute(
    "href",
    /\/app\/$/,
  );
  const preview = page.frameLocator('iframe[title="app example"]');
  await expect(preview.getByRole("status")).toHaveCount(0, { timeout: 90_000 });
  await expect(preview.getByRole("alert")).toHaveCount(0);
  await preview.getByRole("slider", { name: "Clock time" }).press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByRole("link", { name: "open app", exact: true })).toBeFocused();
});
test("egui renders the development notice and original dock layout", async ({ page }) => {
  await page.goto("/simple/");
  await expect(page.getByRole("status")).toHaveCount(0, { timeout: 90000 });
  await page.evaluate(() => window.__TERRARIUM__!.enable_accessibility());
  await expect
    .poll(() => eguiHas(page, "Development build: Press F12 to enable DevTools"))
    .toBe(true);
  const notice = await eguiNode(page, "Development build: Press F12 to enable DevTools");
  expect((notice.bounds.x0 + notice.bounds.x1) / 2).toBeCloseTo(page.viewportSize()!.width / 2, 0);
  await page.keyboard.press("F12");
  await expect.poll(() => eguiHas(page, "Explorer")).toBe(true);
  const explorer = await eguiNode(page, "Explorer"),
    fonts = await eguiNode(page, "Font Market"),
    properties = await eguiNode(page, "Properties");
  expect(properties.bounds.y0).toBeGreaterThan(explorer.bounds.y1);
  expect(fonts.bounds.y0).toBeCloseTo(explorer.bounds.y0, 0);
});
test("outline controls edit the running scene", async ({ page }) => {
  await page.goto("/?example=outlines");
  await expect(page.getByRole("checkbox", { name: "Focus on duck" })).toBeVisible({
    timeout: 90_000,
  });
  await expect.poll(() => page.evaluate(() => window.__TERRARIUM__!.fps())).toBeGreaterThan(0);
  await page.getByText("Toon", { exact: true }).click();
  await page.getByRole("checkbox", { name: "Focus on duck" }).uncheck();
  await expect
    .poll(() => page.evaluate(() => window.__TERRARIUM__!.state()))
    .toMatchObject({ mode: "Toon", focus: false });
});
test("egui inspector selects, edits, reopens, and deletes scene instances", async ({ page }) => {
  await page.goto("/simple/");
  await expect(page.getByRole("status")).toHaveCount(0, { timeout: 90000 });
  await page.evaluate(() => window.__TERRARIUM__!.enable_accessibility());
  await page.keyboard.press("F12");
  await expect.poll(() => eguiHas(page, "Part (Part)")).toBe(true);
  await page.waitForTimeout(1500);
  await eguiClick(page, "Part (Part)");
  await expect.poll(() => eguiHas(page, "1 selected"), { timeout: 30000 }).toBe(true);
  const editor = await eguiNode(page, "Part", "textInput");
  await page.mouse.click(
    (editor.bounds.x0 + editor.bounds.x1) / 2,
    (editor.bounds.y0 + editor.bounds.y1) / 2,
  );
  await expect(page.locator("[data-egui-input]")).toBeFocused();
  await page.keyboard.press("Control+A");
  await page.keyboard.type("Renamed cube");
  await page.keyboard.press("Enter");
  await expect.poll(() => eguiHas(page, "Renamed cube (Part)")).toBe(true);
  await page.keyboard.press("F12");
  await expect.poll(() => eguiHas(page, "DevTools")).toBe(false);
  await page.keyboard.press("F12");
  await expect.poll(() => eguiHas(page, "1 selected"), { timeout: 30000 }).toBe(true);
  await eguiClick(page, "Delete selection", "button", false);
  await expect.poll(() => eguiHas(page, "Renamed cube (Part)")).toBe(false);
  await expect.poll(() => eguiHas(page, "0 selected")).toBe(true);
  await expect(page.getByRole("alert")).toHaveCount(0);
});
test("the tree demo toggles selection with Ctrl-click and selects ranges", async ({ page }) => {
  await page.goto("/?widget=ltreeview_selection");
  const camera = page.getByRole("treeitem").filter({ hasText: "Camera" });
  const light = page.getByRole("treeitem").filter({ hasText: "Light" });
  await camera.click({ position: { x: 32, y: 15 }, modifiers: ["Control"] });
  await expect(page.getByText("Selected node IDs: [1]", { exact: true })).toBeVisible();
  await light.click({ modifiers: ["Shift"] });
  await expect(page.getByText("Selected node IDs: [1, 2]", { exact: true })).toBeVisible();
  await camera.click({ modifiers: ["Control"] });
  await expect(page.getByText("Selected node IDs: [2]", { exact: true })).toBeVisible();
});
test("the inspector keeps rendering responsive when opened and selected under CPU load", async ({
  page,
}) => {
  await page.setViewportSize({ width: 480, height: 480 });
  await page.addInitScript(() => {
    const info = Object.getOwnPropertyDescriptor(GPUAdapter.prototype, "info")!.get!;
    Object.defineProperty(GPUAdapter.prototype, "info", {
      get() {
        const value = info.call(this);
        Object.defineProperty(value, "isFallbackAdapter", { value: false });
        return value;
      },
    });
  });
  const browser = await page.context().newCDPSession(page);
  // Keep egui's CSS viewport intact while limiting software GPU pixel work.
  await browser.send("Emulation.setDeviceMetricsOverride", {
    width: 480,
    height: 480,
    deviceScaleFactor: 0.25,
    mobile: false,
  });
  await browser.send("Emulation.setCPUThrottlingRate", { rate: 4 });
  await page.goto("/simple/");
  await expect(page.getByRole("status")).toHaveCount(0, { timeout: 90_000 });
  await expect
    .poll(() => page.evaluate(() => window.__TERRARIUM__!.fps()), { timeout: 15_000 })
    .toBeGreaterThan(30);
  await page.keyboard.press("F12");
  const averageFps = async () => {
    const samples = [];
    for (let i = 0; i < 3; i++) {
      await page.waitForTimeout(1100);
      samples.push(await page.evaluate(() => window.__TERRARIUM__!.fps()));
    }
    return samples.reduce((sum, fps) => sum + fps, 0) / samples.length;
  };
  await expect.poll(averageFps, { timeout: 20_000 }).toBeGreaterThan(30);
  await page.mouse.click(145, 182);
  await expect.poll(averageFps, { timeout: 20_000 }).toBeGreaterThan(30);
  await page.evaluate(() => window.__TERRARIUM__!.enable_accessibility());
  await expect.poll(() => eguiHas(page, "1 selected"), { timeout: 30000 }).toBe(true);
  await expect(page.getByRole("alert")).toHaveCount(0);
});
test("hardware rendering stays responsive when GPU completion callbacks arrive late", async ({
  page,
}) => {
  await page.setViewportSize({ width: 320, height: 240 });
  await page.addInitScript(() => {
    const info = Object.getOwnPropertyDescriptor(GPUAdapter.prototype, "info")!.get!;
    Object.defineProperty(GPUAdapter.prototype, "info", {
      get() {
        const value = info.call(this);
        Object.defineProperty(value, "isFallbackAdapter", { value: false });
        return value;
      },
    });
    const completed = GPUQueue.prototype.onSubmittedWorkDone;
    GPUQueue.prototype.onSubmittedWorkDone = async function () {
      await completed.call(this);
      await new Promise((resolve) => setTimeout(resolve, 120));
    };
  });
  await page.goto("/simple/");
  await expect(page.getByRole("status")).toHaveCount(0, { timeout: 90_000 });
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect
    .poll(() => page.evaluate(() => window.__TERRARIUM__!.fps()), { timeout: 15_000 })
    .toBeGreaterThan(30);
});
for (const widget of widgetNames) {
  test(`${widget} mounts its package-backed components`, async ({ page }) => {
    await page.goto(`/?widget=${widget}`);
    await expect(page.getByRole("heading", { level: 2 })).toHaveText(widget.replaceAll("_", " "));
    await expect(page.getByRole("alert")).toHaveCount(0);
  });
}

test("font catalog filters, previews, applies, and downloads a family", async ({ page }) => {
  const fonts = [
    {
      id: "alpha",
      family: "Alpha",
      category: "sans-serif",
      variable: true,
      weights: [400],
      styles: ["normal"],
      subsets: ["latin"],
      defSubset: "latin",
      version: "v1",
    },
    {
      id: "beta",
      family: "Beta",
      category: "serif",
      variable: false,
      weights: [400],
      styles: ["normal"],
      subsets: ["latin", "cyrillic"],
      defSubset: "latin",
      version: "v1",
    },
  ];
  await page.route("https://api.fontsource.org/v1/fonts**", (route) => {
    const id = route.request().url().split("/").at(-1);
    const font = fonts.find((font) => font.id === id);
    return route.fulfill({
      json: font
        ? {
            ...font,
            variants: {
              [font.variable ? "100..900" : "400"]: {
                normal: { latin: { url: { ttf: "https://cdn.jsdelivr.net/terrarium-test.ttf" } } },
              },
            },
          }
        : fonts,
    });
  });
  await page.route("https://cdn.jsdelivr.net/terrarium-test.ttf", (route) =>
    route.fulfill({
      body: readFileSync(new URL("../dist/fonts/Outfit.ttf", import.meta.url)),
      contentType: "font/ttf",
    }),
  );
  await page.goto("/simple/");
  await expect(page.getByRole("status")).toHaveCount(0, { timeout: 90_000 });
  await page.evaluate(() => window.__TERRARIUM__!.enable_accessibility());
  await page.keyboard.press("F12");
  await expect.poll(() => eguiHas(page, "Font Market")).toBe(true);
  await page.waitForTimeout(1500);
  const window = await eguiNode(page, "DevTools");
  await page.mouse.move(window.bounds.x1 - 2, window.bounds.y1 - 2);
  await page.mouse.down();
  await page.waitForTimeout(500);
  await page.mouse.move(1100, 680, { steps: 10 });
  await expect.poll(async () => (await eguiNode(page, "DevTools")).bounds.x1).toBeGreaterThan(1000);
  await page.mouse.up();
  await eguiClick(page, "Font Market");
  await expect.poll(() => eguiHas(page, "2 font families")).toBe(true);
  await eguiClick(page, "Variable fonts", "checkBox");
  await expect.poll(() => eguiHas(page, "1 font families")).toBe(true);
  await eguiClick(page, "Clear filters", "button", false);
  await expect.poll(() => eguiHas(page, "2 font families")).toBe(true);
  await eguiClick(page, "Alpha", "button");
  await expect.poll(() => eguiHas(page, "Preview text"), { timeout: 30000 }).toBe(true);
  await eguiClick(page, "Glyphs");
  await expect.poll(() => eguiHas(page, "Latin character preview")).toBe(true);
  await eguiClick(page, "About");
  await expect.poll(() => eguiHas(page, "Subsets")).toBe(true);
  await eguiClick(page, "Get font");
  await expect.poll(() => eguiHas(page, "Use in app")).toBe(true);
  await eguiClick(page, "Use in app");
  await expect.poll(async () => (await eguiNode(page, "Use in app")).toggled).toBe("true");
  await eguiClick(page, "Preview");
  await expect.poll(() => eguiHas(page, "Applied to app text"), { timeout: 30000 }).toBe(true);
  await eguiClick(page, "Get font");
  const download = page.waitForEvent("download", { timeout: 30000 });
  await eguiClick(page, "Download 1 file", "button", false);
  await page.getByRole("link", { name: "click here to download your file" }).click();
  expect((await download).suggestedFilename()).toMatch(/alpha.*\.ttf$/);
  await expect(page.getByRole("alert")).toHaveCount(0);
});
