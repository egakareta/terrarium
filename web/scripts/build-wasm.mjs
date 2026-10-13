import { spawn } from "node:child_process";
import { mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const examples = process.argv.includes("--all")
  ? [
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
    ]
  : [process.argv[2] ?? "app"];
const run = (command, args, capture = false) =>
  new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: root,
      stdio: capture ? ["ignore", "pipe", "inherit"] : "inherit",
      env: { ...process.env, RUSTFLAGS: "" },
    });
    let output = "";
    if (capture)
      child.stdout.on("data", (data) => {
        output += data;
      });
    child.on("error", reject);
    child.on("close", (code) =>
      code === 0 ? resolve(output) : reject(new Error(`${command} exited with code ${code}`)),
    );
  });
const cargoLock = readFileSync(`${root}/Cargo.lock`, "utf8");
const bindgenVersion = cargoLock.match(/name = "wasm-bindgen"\nversion = "([^"]+)"/)[1];
const version = await run("wasm-bindgen", ["--version"], true);
if (!version.includes(bindgenVersion))
  throw new Error(`Install wasm-bindgen-cli ${bindgenVersion} to match Cargo.lock`);
for (const example of examples) {
  if (!/^[a-z_]+$/.test(example)) throw new Error("Invalid example name");
  await run("cargo", [
    "build",
    "--locked",
    "--release",
    "--all-features",
    "--target",
    "wasm32-unknown-unknown",
    "--example",
    example,
  ]);
  mkdirSync(`${root}/web/dist/wasm`, { recursive: true });
  await run("wasm-bindgen", [
    `${root}/target/wasm32-unknown-unknown/release/examples/${example}.wasm`,
    "--target",
    "web",
    "--out-dir",
    `${root}/web/dist/wasm`,
    "--out-name",
    example,
  ]);
}
