import {
  Button,
  Checkbox,
  Divider,
  Group,
  Slider,
  Stack,
  Text,
  Title,
  Select,
  NumberInput,
} from "@mantine/core";
import { WidgetGallery, widgetNames } from "./Widgets";
import { useEngine } from "../context";
import { Window } from "../components/Window";
import { Viewport } from "../components/Viewport";

function WidgetsApp() {
  const { state, command } = useEngine<{ widget: string }>();
  return (
    <div className="native-widgets">
      <Select
        aria-label="Widget demo"
        data={widgetNames}
        value={state.widget}
        onChange={(value) => value && void command("widget", value)}
      />
      <WidgetGallery name={state.widget} />
    </div>
  );
}
function SceneApp() {
  const { fps, state, command } = useEngine<{ clock: number; time: string }>();
  return (
    <div className="top-panel">
      <Group gap={8}>
        <Text>FPS: {fps.toFixed(0)}</Text>
        <Divider orientation="vertical" />
        <Text ff="monospace">{state.time}</Text>
        <Slider
          aria-label="Clock time"
          thumbLabel="Clock time"
          value={state.clock}
          min={0}
          max={24}
          step={0.01}
          label={(v) => `${v.toFixed(2)}h`}
          onChange={(v) => void command("clock", v)}
          w={100}
        />
        <NumberInput
          aria-label="Clock time value"
          value={state.clock}
          min={0}
          max={24}
          step={0.01}
          suffix="h"
          decimalScale={2}
          hideControls
          size="xs"
          w={64}
          ff="monospace"
          onChange={(value) => typeof value === "number" && void command("clock", value)}
        />
      </Group>
    </div>
  );
}
function PhysicsApp() {
  const { fps, state, command } = useEngine<{ bodies: number; gravity: number[] }>();
  return (
    <aside className="side-panel" style={{ width: 220 }}>
      <Stack gap="xs">
        <Text>Dynamic bodies: {state.bodies}</Text>
        <Text>Gravity: ({state.gravity.map((v) => v.toFixed(1)).join(", ")})</Text>
        <Text>FPS: {fps.toFixed(0)}</Text>
        <Divider />
        <Button onClick={() => void command("reset")}>Reset stack</Button>
        <Button onClick={() => void command("kick")}>Kick stack</Button>
      </Stack>
    </aside>
  );
}
function OutlinesApp() {
  const { state, command } = useEngine<{ mode: string; width: number; focus: boolean }>();
  return (
    <div className="top-panel">
      <Group gap={8}>
        <Text>Outline:</Text>
        {["Toon", "Silhouette", "Stencil"].map((mode) => (
          <Button
            key={mode}
            aria-pressed={state.mode === mode}
            onClick={() => void command("mode", mode)}
          >
            {mode}
          </Button>
        ))}
        <Divider orientation="vertical" />
        <Text>Width</Text>
        <Slider
          aria-label="Width"
          thumbLabel="Width"
          min={0.5}
          max={10}
          step={0.1}
          value={state.width}
          w={100}
          onChange={(v) => void command("width", v)}
        />
        <NumberInput
          aria-label="Width value"
          value={state.width}
          min={0.5}
          max={10}
          step={0.1}
          decimalScale={1}
          hideControls
          size="xs"
          w={54}
          onChange={(value) => typeof value === "number" && void command("width", value)}
        />
        <Divider orientation="vertical" />
        <Checkbox
          label="Focus on duck"
          checked={state.focus}
          onChange={(e) => void command("focus", e.currentTarget.checked)}
        />
      </Group>
    </div>
  );
}
interface SoundState {
  volume: number;
  radius: number;
  speed: number;
  moving: boolean;
  occluded: boolean;
  playback: string;
  position: number | null;
  emitter: number[];
  distance: number;
  error: string | null;
}
function SoundApp() {
  const { state, command } = useEngine<SoundState>();
  return (
    <aside className="side-panel" style={{ width: 280 }}>
      <Stack gap="xs">
        <Title order={3}>Spatial sound</Title>
        <Text>Click Play to enable audio. Headphones recommended.</Text>
        <Text>WASD + left-click drag moves the camera/listener.</Text>
        <Divider />
        <Group gap="xs">
          {[
            ["play", "Play / resume"],
            ["pause", "Pause"],
            ["replay", "Replay"],
            ["stop", "Stop"],
          ].map(([action, label]) => (
            <Button key={action} onClick={() => void command(action)}>
              {label}
            </Button>
          ))}
        </Group>
        {(
          [
            ["volume", "Volume", 0, 1],
            ["radius", "Radius (m)", 1, 12],
            ["speed", "Speed (rad/s)", 0, 3],
          ] as const
        ).map(([key, label, min, max]) => (
          <div key={key}>
            <Text>{label}</Text>
            <Slider
              aria-label={label}
              thumbLabel={label}
              min={min}
              max={max}
              step={0.01}
              value={state[key]}
              onChange={(v) => void command(key, v)}
            />
          </div>
        ))}
        <Divider />
        <Checkbox
          label="Orbit the emitter"
          checked={state.moving}
          onChange={(e) => void command("moving", e.currentTarget.checked)}
        />
        <Checkbox
          label="Simulate wall occlusion"
          checked={state.occluded}
          onChange={(e) => void command("occluded", e.currentTarget.checked)}
        />
        <Text>Occlusion is supplied explicitly; no geometry raycasts are used.</Text>
        <Divider />
        <Text>Playback: {state.playback}</Text>
        {state.position !== null && <Text>Loop position: {state.position.toFixed(2)} s</Text>}
        <Text>Emitter: ({state.emitter.map((v) => v.toFixed(1)).join(", ")}) m</Text>
        <Text>Distance: {state.distance.toFixed(1)} m</Text>
        <Text>HRTF, distance attenuation, and Doppler are enabled.</Text>
        {state.error && (
          <Text c="red" role="alert">
            {state.error}
          </Text>
        )}
      </Stack>
    </aside>
  );
}
function ViewportApp() {
  const { state } = useEngine<{
    pixels: string;
    textures: { id: string; size: [number, number] }[];
  }>();
  return (
    <Window title="Viewport" width={340} height={600}>
      <Stack gap="xs">
        <Text>This is an image :)</Text>
        {state.textures.map((texture) => (
          <Viewport key={texture.id} {...texture} />
        ))}
        <Text>{state.pixels}</Text>
      </Stack>
    </Window>
  );
}
export const applications: Record<string, () => React.ReactNode> = {
  widgets: WidgetsApp,
  app: SceneApp,
  physics: PhysicsApp,
  outlines: OutlinesApp,
  sound: SoundApp,
  viewport: ViewportApp,
  bevy: () => (
    <Window title="Bevy" width={400} height={180}>
      <Text>Bevy systems animate ordinary Terrarium parts.</Text>
    </Window>
  ),
  headless: () => (
    <Text size="40px" className="headless-label">
      i love headless tests
    </Text>
  ),
  simple: () => null,
  gltf: () => null,
  render_benchmark: () => null,
};
