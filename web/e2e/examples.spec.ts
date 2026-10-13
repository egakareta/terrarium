import { readFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import { widgetNames } from "../src/examples/Widgets";

test.afterEach(async ({ page }, testInfo) => {
  if (testInfo.status === testInfo.expectedStatus || page.isClosed()) return;
  console.log(
    JSON.stringify({
      example: testInfo.title,
      alerts: await page.getByRole("alert").allTextContents(),
      state: await page.evaluate(() => window.__TERRARIUM__?.state().app),
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
    const state = await page.evaluate(() => window.__TERRARIUM__!.state());
    expect(state.ui).toBe(example);
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
              const textures = (state.app as { textures: { id: string; size: [number, number] }[] })
                .textures;
              return textures.map((texture) => {
                const bytes = window.__TERRARIUM__!.viewport(texture.id);
                return bytes?.length ?? -1;
              });
            }),
          { timeout: 30_000 },
        )
        .toEqual(
          (state.app as { textures: { size: [number, number] }[] }).textures.map(
            (texture) => texture.size[0] * texture.size[1] * 4,
          ),
        );
      await expect(page.getByRole("alert")).toHaveCount(0);
    }
    await page.keyboard.press("F12");
    if (example !== "headless" && example !== "render_benchmark") {
      await expect(page.getByText("DevTools", { exact: true })).toBeVisible();
      await expect(page.getByText("Explorer", { exact: true })).toBeVisible();
      const inspector = await page.evaluate(() => window.__TERRARIUM__!.state().devtools);
      expect(inspector?.visible).toBe(true);
      expect(inspector?.tree?.class).toBe("Workspace");
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
test("the development notice and default inspector layout retain their original presentation", async ({
  page,
}) => {
  await page.goto("/simple/");
  const notice = page.getByText("Development build: Press F12 to enable DevTools", { exact: true });
  await expect(notice).toBeVisible({ timeout: 90_000 });
  const style = await notice.evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      color: style.color,
      outline: style.webkitTextStrokeWidth,
      bounds: element.getBoundingClientRect().toJSON(),
    };
  });
  expect(style.color).toBe("rgb(255, 0, 0)");
  expect(style.outline).toBe("3px");
  expect(style.bounds.x + style.bounds.width / 2).toBeCloseTo(page.viewportSize()!.width / 2, 0);
  await page.keyboard.press("F12");
  const explorer = page.getByText("Explorer", { exact: true });
  const fonts = page.getByText("Font Market", { exact: true });
  const properties = page.getByText("Properties", { exact: true });
  await expect(explorer).toBeVisible();
  await expect(properties).toBeVisible();
  const top = await explorer.boundingBox();
  const bottom = await properties.boundingBox();
  const font = await fonts.boundingBox();
  expect(bottom!.y).toBeGreaterThan(top!.y + top!.height);
  expect(bottom!.x).toBeCloseTo(top!.x, 0);
  expect(font!.y).toBeCloseTo(top!.y, 0);
  await expect(notice).toBeVisible();
});
test("outline controls edit the running scene", async ({ page }) => {
  await page.goto("/?example=outlines");
  await expect(page.getByRole("checkbox", { name: "Focus on duck" })).toBeVisible({
    timeout: 90_000,
  });
  await page.getByText("Toon", { exact: true }).click();
  await page.getByRole("checkbox", { name: "Focus on duck" }).uncheck();
  await expect
    .poll(() => page.evaluate(() => window.__TERRARIUM__!.state().app))
    .toMatchObject({ mode: "Toon", focus: false });
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
    .poll(() => page.evaluate(() => window.__TERRARIUM__!.state().fps), { timeout: 15_000 })
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
  await page.goto("/?example=app");
  await expect(page.getByRole("status")).toHaveCount(0, { timeout: 90_000 });
  await page.keyboard.press("F12");
  await page.getByText("Font Market", { exact: true }).click();
  await expect(page.getByText("2 font families", { exact: true })).toBeVisible();
  await page.getByRole("checkbox", { name: "Variable only" }).check();
  await expect(page.getByRole("button", { name: "Preview Beta", exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Clear filters", exact: true }).click();
  await page.getByRole("textbox", { name: "Font language" }).click();
  await page.getByRole("option", { name: "cyrillic", exact: true }).click();
  await expect(page.getByRole("button", { name: "Preview Alpha", exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Clear filters", exact: true }).click();
  await page.getByRole("button", { name: "Preview Alpha", exact: true }).click();
  await page.getByRole("tab", { name: "Get font", exact: true }).click();
  const apply = page.getByRole("checkbox", { name: "Use in app", exact: true });
  await expect(apply).toBeEnabled();
  await apply.check();
  await expect
    .poll(() =>
      page.evaluate(() => document.documentElement.style.getPropertyValue("--terrarium-font")),
    )
    .toContain("terrarium-alpha");
  await apply.uncheck();
  await expect
    .poll(() =>
      page.evaluate(() => document.documentElement.style.getPropertyValue("--terrarium-font")),
    )
    .toBe("");
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Download font files", exact: true }).click();
  expect((await download).suggestedFilename()).toBe("alpha.zip");
  await expect(page.getByRole("alert")).toHaveCount(0);
});
