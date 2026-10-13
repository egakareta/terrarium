import { readFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import { widgetNames } from "../src/examples/Widgets";
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
      `/?example=${example}${example === "render_benchmark" ? "&parts=32&warmup=1&frames=2&width=128&height=128" : ""}`,
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
