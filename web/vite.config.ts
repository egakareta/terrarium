import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { mkdirSync, copyFileSync, cpSync, rmSync, createReadStream, existsSync } from "node:fs";

export default defineConfig({
  base: "./",
  plugins: [
    react(),
    {
      name: "terrarium-fonts",
      configResolved(config) {
        if (config.command === "build") rmSync("dist/assets", { recursive: true, force: true });
      },
      configureServer(server) {
        server.middlewares.use((request, response, next) => {
          const url = (request.url ?? "").split("?")[0];
          if (!/^\/wasm\/[a-z_]+(?:_bg)?\.(?:js|wasm)$/.test(url)) return next();
          const path = `dist${url}`;
          if (!existsSync(path)) return next();
          response.setHeader(
            "Content-Type",
            url.endsWith(".wasm") ? "application/wasm" : "text/javascript",
          );
          createReadStream(path).pipe(response);
        });
      },
      closeBundle() {
        copyFileSync("dist/index.html", "dist/examples.html");
        mkdirSync("dist/fonts", { recursive: true });
        for (const font of ["Outfit", "SUSEMono"]) {
          copyFileSync(
            `../crates/trfont/src/bin/${font}-VariableFont_wght.ttf`,
            `dist/fonts/${font}.ttf`,
          );
        }
        rmSync("../crates/trengine/frontend/assets", { recursive: true, force: true });
        mkdirSync("../crates/trengine/frontend", { recursive: true });
        for (const path of ["index.html", "assets", "fonts"])
          cpSync(`dist/${path}`, `../crates/trengine/frontend/${path}`, { recursive: true });
      },
    },
  ],
  build: {
    emptyOutDir: false,
    sourcemap: true,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("opentype.js") || id.includes("fflate")) return "font-tools";
          if (id.includes("dockview")) return "dockview";
          if (id.includes("node_modules")) return "vendor";
        },
      },
    },
  },
});
