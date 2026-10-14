export const exampleNames = [
  "app",
  "gltf",
  "outlines",
  "physics",
  "simple",
  "viewport",
  "bevy",
  "sound",
  "headless",
  "render_benchmark",
  "widgets",
] as const;

export function exampleFromLocation(): string | undefined {
  const query = new URLSearchParams(location.search).get("example");
  if (query) return query;
  const name = location.pathname
    .replace(/\/index\.html$/, "/")
    .split("/")
    .filter(Boolean)
    .at(-1);
  return exampleNames.find((example) => example === name);
}

export function assetBase(): URL {
  const moduleUrl = import.meta.url;
  return new URL("../", moduleUrl);
}
