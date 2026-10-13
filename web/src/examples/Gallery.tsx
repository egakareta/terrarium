import { Anchor, SimpleGrid, Stack, Title } from "@mantine/core";
import { widgetNames } from "./Widgets";
export function Gallery() {
  return (
    <div className="widget-gallery">
      <Stack>
        <Title>Terrarium examples</Title>
        <SimpleGrid cols={{ base: 1, sm: 3 }}>
          {[
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
          ].map((name) => (
            <Anchor key={name} href={`?example=${name}`}>
              {name}
            </Anchor>
          ))}
        </SimpleGrid>
        <Title order={2}>Widgets</Title>
        <SimpleGrid cols={{ base: 1, sm: 3 }}>
          {widgetNames.map((name) => (
            <Anchor key={name} href={`?widget=${name}`}>
              {name.replaceAll("_", " ")}
            </Anchor>
          ))}
        </SimpleGrid>
      </Stack>
    </div>
  );
}
