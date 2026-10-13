import { useEffect, useMemo, useState } from "react";
import {
  Button,
  Card,
  Progress,
  Checkbox,
  Group,
  Loader,
  NumberInput,
  Select,
  Slider,
  Stack,
  Tabs,
  Text,
  TextInput,
  Title,
} from "@mantine/core";
import { useResizeObserver } from "@mantine/hooks";
import { useVirtualizer } from "@tanstack/react-virtual";
import { zipSync, strToU8 } from "fflate";
import { parse } from "opentype.js";

interface FontRecord {
  id: string;
  family: string;
  category: string;
  variable: boolean;
  weights: number[];
  subsets: string[];
  version: string;
  defSubset: string;
  styles: string[];
  type?: string;
}
interface FontDetails extends FontRecord {
  variants: Record<string, Record<string, Record<string, { url: Record<string, string> }>>>;
}
const API = "https://api.fontsource.org/v1/fonts";
const SENTENCE = "Sphinx of black quartz, judge my vow.";
function checkUrl(url: string) {
  const parsed = new URL(url);
  if (parsed.protocol !== "https:" || parsed.hostname !== "cdn.jsdelivr.net")
    throw new Error("Unsupported font download URL");
  return parsed.href;
}
async function fetchChecked(url: string, signal?: AbortSignal) {
  const response = await fetch(url, { signal });
  if (!response.ok) throw new Error(`Font request failed (${response.status})`);
  return response;
}
function fontUrl(details: FontDetails, weight: number) {
  const variant =
    Object.entries(details.variants).find(
      ([key]) => details.variable && (key === "variable" || key.includes("..")),
    )?.[1] ??
    details.variants[String(weight)] ??
    Object.entries(details.variants)
      .filter(([key]) => /^\d+$/.test(key))
      .sort(([a], [b]) => Math.abs(Number(a) - weight) - Math.abs(Number(b) - weight))[0]?.[1] ??
    details.variants["400"] ??
    Object.values(details.variants)[0];
  const style = variant.normal ?? Object.values(variant)[0];
  const subset = style[details.defSubset] ?? style.latin ?? Object.values(style)[0];
  return checkUrl(subset.url.ttf ?? subset.url.woff2);
}
const previews = new Map<string, Promise<string>>();
function loadPreview(details: FontDetails, weight = 400) {
  const key = `${details.id}-${weight}`;
  if (!previews.has(key)) {
    previews.set(
      key,
      (async () => {
        const bytes = await (await fetchChecked(fontUrl(details, weight))).arrayBuffer();
        const name = `terrarium-${key}`;
        const face = await new FontFace(name, bytes, {
          weight: details.variable ? "100 900" : String(weight),
        }).load();
        document.fonts.add(face);
        return name;
      })().catch((error) => {
        previews.delete(key);
        throw error;
      }),
    );
  }
  return previews.get(key)!;
}
function FontCard({
  font,
  sentence,
  size,
  open,
}: {
  font: FontRecord;
  sentence: string;
  size: number;
  open: () => void;
}) {
  const [family, setFamily] = useState<string>();
  useEffect(() => {
    let active = true;
    void fetchChecked(`${API}/${encodeURIComponent(font.id)}`)
      .then((r) => r.json())
      .then(loadPreview)
      .then((name) => {
        if (active) setFamily(name);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [font.id]);
  return (
    <Card
      component="button"
      onClick={open}
      withBorder
      aria-label={`Preview ${font.family}`}
      style={{ height: 174, textAlign: "left", width: "100%" }}
    >
      <Text
        style={{ fontFamily: family, fontSize: Math.min(size, 34), flex: 1, overflow: "hidden" }}
      >
        {sentence || SENTENCE}
      </Text>
      <Text fw={600}>{font.family}</Text>
      {font.variable && (
        <Text size="xs" c="dimmed">
          Variable
        </Text>
      )}
    </Card>
  );
}
export function FontMarket() {
  const [catalog, setCatalog] = useState<FontRecord[]>([]),
    [details, setDetails] = useState<FontDetails>();
  const [query, setQuery] = useState(""),
    [category, setCategory] = useState<string | null>(null),
    [language, setLanguage] = useState<string | null>(null),
    [sort, setSort] = useState<string | null>("name"),
    [retry, setRetry] = useState(0),
    [variable, setVariable] = useState(false);
  const [weight, setWeight] = useState(400),
    [size, setSize] = useState(32),
    [sentence, setSentence] = useState(SENTENCE),
    [allSubsets, setAllSubsets] = useState(false);
  const [error, setError] = useState<string>(),
    [busy, setBusy] = useState(false),
    [loaded, setLoaded] = useState<string>(),
    [glyphs, setGlyphs] = useState<string[]>([]);
  const [progress, setProgress] = useState(0);
  const [applied, setApplied] = useState<string>();
  const [scroll, bounds] = useResizeObserver<HTMLDivElement>();
  const columns = Math.max(1, Math.min(5, Math.floor((bounds.width + 8) / 233)));
  useEffect(() => {
    const controller = new AbortController();
    setError(undefined);
    void fetchChecked(API, controller.signal)
      .then((r) => r.json())
      .then(setCatalog)
      .catch((e) => {
        if (!controller.signal.aborted) setError(String(e));
      });
    return () => controller.abort();
  }, [retry]);
  const filtered = useMemo(
    () =>
      catalog
        .filter(
          (font) =>
            font.family.toLowerCase().includes(query.toLowerCase()) &&
            (!category || font.category === category) &&
            (!language || font.subsets.includes(language)) &&
            (!variable || font.variable),
        )
        .sort(
          (a, b) =>
            (sort === "variable" ? Number(b.variable) - Number(a.variable) : 0) ||
            a.family.localeCompare(b.family),
        ),
    [catalog, query, category, language, variable, sort],
  );
  const virtual = useVirtualizer({
    count: Math.ceil(filtered.length / columns),
    getScrollElement: () => scroll.current,
    estimateSize: () => 182,
    overscan: 1,
  });
  useEffect(() => {
    if (!details) return;
    const controller = new AbortController();
    void fetchChecked(fontUrl(details, weight), controller.signal)
      .then((r) => r.arrayBuffer())
      .then(async (bytes) => {
        const name = `terrarium-${details.id}-${weight}`;
        const face = new FontFace(name, bytes, {
          weight: details.variable ? "100 900" : String(weight),
        });
        await face.load();
        if (controller.signal.aborted) return;
        document.fonts.add(face);
        setLoaded(name);
        try {
          const font = parse(bytes);
          setGlyphs(
            Array.from({ length: font.numGlyphs }, (_, i) => font.glyphs.get(i).unicode)
              .filter((v): v is number => v !== undefined)
              .map((v) => String.fromCodePoint(v)),
          );
        } catch {
          setGlyphs([]);
        }
      })
      .catch((e) => {
        if (!controller.signal.aborted) setError(String(e));
      });
    return () => controller.abort();
  }, [details, weight]);
  const open = async (font: FontRecord) => {
    setBusy(true);
    setError(undefined);
    try {
      setDetails(await (await fetchChecked(`${API}/${encodeURIComponent(font.id)}`)).json());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const download = async () => {
    if (!details) return;
    setBusy(true);
    setProgress(0);
    setError(undefined);
    try {
      const files: Record<string, Uint8Array> = {};
      const assets: { name: string; url: string }[] = [];
      for (const [weight, styles] of Object.entries(details.variants))
        for (const [style, subsets] of Object.entries(styles))
          for (const [subset, asset] of Object.entries(subsets)) {
            if (!allSubsets && subset !== details.defSubset) continue;
            if (!asset.url.ttf) continue;
            const name = `${details.id}-${weight}-${style}-${subset}.ttf`.replace(
              /[^a-zA-Z0-9_.-]/g,
              "_",
            );
            assets.push({ name, url: checkUrl(asset.url.ttf) });
          }
      if (!assets.length) throw new Error("No TrueType files are available for this font");
      for (let index = 0; index < assets.length; index++) {
        const asset = assets[index];
        files[asset.name] = new Uint8Array(await (await fetchChecked(asset.url)).arrayBuffer());
        setProgress(((index + 1) / assets.length) * 100);
      }
      files["metadata.json"] = strToU8(JSON.stringify(details, null, 2));
      const bytes = zipSync(files);
      const url = URL.createObjectURL(
        new Blob([new Uint8Array(bytes).buffer], { type: "application/zip" }),
      );
      const link = document.createElement("a");
      link.href = url;
      link.download = `${details.id}.zip`;
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="inspector-panel">
      <Stack gap="xs">
        {error && (
          <Text c="red" role="alert">
            {error}
            {!catalog.length && <Button onClick={() => setRetry((v) => v + 1)}>Retry</Button>}
          </Text>
        )}
        {busy && <Loader size="sm" />}
        {!details ? (
          <>
            <TextInput
              aria-label="Search fonts"
              placeholder="Search fonts"
              value={query}
              onChange={(e) => setQuery(e.currentTarget.value)}
            />
            <TextInput
              label="Preview text"
              value={sentence}
              onChange={(e) => setSentence(e.currentTarget.value)}
            />
            <NumberInput
              label="Preview size"
              min={16}
              max={48}
              value={size}
              onChange={(v) => {
                if (typeof v === "number") setSize(v);
              }}
            />
            <Group>
              <Select
                aria-label="Font category"
                placeholder="All categories"
                clearable
                data={[...new Set(catalog.map((f) => f.category))]}
                value={category}
                onChange={setCategory}
              />
              <Select
                aria-label="Font language"
                placeholder="All languages"
                clearable
                value={language}
                onChange={setLanguage}
                data={[...new Set(catalog.flatMap((font) => font.subsets))].sort()}
              />
              <Checkbox
                label="Variable only"
                checked={variable}
                onChange={(e) => setVariable(e.currentTarget.checked)}
              />
            </Group>
            <Button
              variant="subtle"
              onClick={() => {
                setQuery("");
                setCategory(null);
                setLanguage(null);
                setVariable(false);
              }}
            >
              Clear filters
            </Button>
            <Group justify="space-between">
              <Text>{filtered.length} font families</Text>
              <Select
                aria-label="Sort fonts"
                value={sort}
                onChange={setSort}
                data={[
                  { value: "name", label: "Name A–Z" },
                  { value: "variable", label: "Variable first" },
                ]}
              />
            </Group>
            {!catalog.length && !error && <Text>Loading fonts from Fontsource…</Text>}
            <div ref={scroll} style={{ height: 360, overflow: "auto" }}>
              <div style={{ height: virtual.getTotalSize(), position: "relative" }}>
                {virtual.getVirtualItems().map((item) => (
                  <div
                    key={item.key}
                    style={{
                      position: "absolute",
                      top: item.start,
                      width: "100%",
                      display: "grid",
                      gap: 8,
                      gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
                    }}
                  >
                    {filtered
                      .slice(item.index * columns, (item.index + 1) * columns)
                      .map((font) => (
                        <FontCard
                          key={font.id}
                          font={font}
                          sentence={sentence}
                          size={size}
                          open={() => void open(font)}
                        />
                      ))}
                  </div>
                ))}
              </div>
            </div>
          </>
        ) : (
          <>
            <Group>
              <Button onClick={() => setDetails(undefined)}>Back</Button>
              <Title order={4}>{details.family}</Title>
            </Group>
            <Tabs defaultValue="preview">
              <Tabs.List>
                <Tabs.Tab value="preview">Preview</Tabs.Tab>
                <Tabs.Tab value="glyphs">Glyphs</Tabs.Tab>
                <Tabs.Tab value="about">About</Tabs.Tab>
                <Tabs.Tab value="get">Get font</Tabs.Tab>
              </Tabs.List>
              <Tabs.Panel value="preview" pt="sm">
                <Stack>
                  <TextInput
                    aria-label="Preview sentence"
                    value={sentence}
                    onChange={(e) => setSentence(e.currentTarget.value)}
                  />
                  <NumberInput
                    label="Size"
                    value={size}
                    min={8}
                    max={160}
                    onChange={(v) => {
                      if (typeof v === "number") setSize(v);
                    }}
                  />
                  <Slider
                    aria-label="Weight"
                    value={weight}
                    min={100}
                    max={900}
                    step={100}
                    onChange={setWeight}
                  />
                  <p style={{ fontFamily: loaded, fontSize: size, fontWeight: weight }}>
                    {sentence}
                  </p>
                </Stack>
              </Tabs.Panel>
              <Tabs.Panel value="glyphs" pt="sm">
                <div
                  style={{
                    display: "grid",
                    gridTemplateColumns: "repeat(auto-fill, 40px)",
                    fontFamily: loaded,
                    fontSize: 24,
                  }}
                >
                  {glyphs.slice(0, 2000).map((glyph, i) => (
                    <span key={i} title={`U+${glyph.codePointAt(0)!.toString(16).toUpperCase()}`}>
                      {glyph}
                    </span>
                  ))}
                </div>
              </Tabs.Panel>
              <Tabs.Panel value="about" pt="sm">
                <Text>Category: {details.category}</Text>
                <Text>Source: {details.type}</Text>
                <Text>Styles: {details.styles.join(", ")}</Text>
                <Text>Variable: {details.variable ? "Yes" : "No"}</Text>
                <Text>Version: {details.version}</Text>
                <Text>Weights: {details.weights.join(", ")}</Text>
                <Text>Subsets: {details.subsets.join(", ")}</Text>
              </Tabs.Panel>
              <Tabs.Panel value="get" pt="sm">
                <Stack>
                  <Checkbox
                    label="Use in app"
                    disabled={!loaded}
                    checked={!!loaded && applied === loaded}
                    onChange={(e) => {
                      setApplied(e.currentTarget.checked ? loaded : undefined);
                      if (e.currentTarget.checked)
                        document.documentElement.style.setProperty(
                          "--terrarium-font",
                          `'${loaded}'`,
                        );
                      else document.documentElement.style.removeProperty("--terrarium-font");
                    }}
                  />
                  <Checkbox
                    label="Include all subsets"
                    checked={allSubsets}
                    onChange={(e) => setAllSubsets(e.currentTarget.checked)}
                  />
                  {busy && <Progress value={progress} aria-label="Font download progress" />}
                  <Button loading={busy} onClick={() => void download()}>
                    Download font files
                  </Button>
                </Stack>
              </Tabs.Panel>
            </Tabs>
          </>
        )}
      </Stack>
    </div>
  );
}
