import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "e2e",
  timeout: 120_000,
  workers: 1,
  use: {
    baseURL: "http://127.0.0.1:4173",
    headless: true,
    launchOptions: {
      executablePath: process.env.TERRARIUM_CHROMIUM,
      args: [
        "--no-sandbox",
        "--enable-unsafe-webgpu",
        "--use-angle=vulkan",
        "--enable-features=Vulkan",
        "--disable-vulkan-surface",
      ],
    },
  },
  projects: [
    { name: "webgpu" },
    {
      name: "webgl",
      grep: /viewport starts/,
      use: {
        launchOptions: {
          executablePath: process.env.TERRARIUM_CHROMIUM,
          args: [
            "--no-sandbox",
            "--use-angle=swiftshader",
            "--enable-unsafe-swiftshader",
            "--disable-blink-features=WebGPU",
            "--disable-features=Vulkan,WebGPU",
          ],
        },
      },
    },
  ],
  webServer: {
    command: "npm run preview -- --host 127.0.0.1",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: false,
  },
});
