import { useCallback, useEffect, useRef, useState } from "react";
import { Alert, MantineProvider, createTheme } from "@mantine/core";
import "@mantine/core/styles.css";
import "./style.css";
import { connect, type Transport } from "./bridge";
import { collectInput } from "./input";
import { EngineContext } from "./context";
import { applications } from "./examples/Applications";
import { platformOutput } from "./platform";
import { Gallery } from "./examples/Gallery";
import { WidgetGallery } from "./examples/Widgets";

const theme = createTheme({
  primaryColor: "teal",
  defaultRadius: 4,
  fontFamily: "var(--terrarium-font, var(--terrarium-default-font, Outfit)), sans-serif",
  fontFamilyMonospace: "SUSEMono, monospace",
  colors: {
    dark: [
      "#e6eef5",
      "#94a7b7",
      "#304554",
      "#233846",
      "#1a2735",
      "#131d2a",
      "#0d141e",
      "#080c12",
      "#080c12",
      "#080c12",
    ],
  },
  components: {
    Button: { defaultProps: { size: "compact-sm", variant: "default" } },
    Text: { defaultProps: { size: "sm" } },
  },
});

interface HostProps {
  components?: Record<string, () => React.ReactNode>;
  canvasId?: string;
  gallery?: boolean;
}
export function TerrariumHost(props: HostProps) {
  return (
    <MantineProvider theme={theme} defaultColorScheme="dark">
      <HostContent {...props} />
    </MantineProvider>
  );
}
function HostContent({
  components = applications,
  canvasId = "app",
  gallery = new URLSearchParams(location.search).has("gallery") ||
    location.pathname.endsWith("/examples.html"),
}: HostProps) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const overlay = useRef<HTMLCanvasElement>(null);
  const [transport, setTransport] = useState<Transport>();
  const [state, setState] = useState<unknown>();
  const [fps, setFps] = useState(0);
  const [error, setError] = useState<string>();
  const widget = new URLSearchParams(location.search).get("widget");
  useEffect(() => {
    if (widget || gallery) return;
    let stopped = false,
      animation = 0;
    let bridge: Transport | undefined;
    const input = collectInput(canvas.current!, overlay.current!);
    const platform = platformOutput(canvas.current!, overlay.current!);
    void connect(canvas.current!, overlay.current!)
      .then(async (connection) => {
        bridge = connection;
        if (stopped) {
          connection.dispose();
          return;
        }
        const initial = await connection.state();
        if (stopped) return;
        setState(initial);
        setTransport(connection);
        let lastState = performance.now();
        const render = async (now: number) => {
          try {
            const events = input.next(now);
            const output = await connection.advance(events);
            if (stopped) return;
            platform.apply(
              output.platform,
              output.pixels_per_point,
              output.gui_regions,
              output.gui_input_regions,
            );
            if (output.close || events.pressed.length || now - lastState >= 1000 / 30) {
              const next = await connection.state();
              if (stopped) return;
              setState(next);
              setFps(output.fps);
              lastState = now;
            }
            if (!output.close) animation = requestAnimationFrame(render);
          } catch (error) {
            if (!stopped) setError(String(error));
          }
        };
        animation = requestAnimationFrame(render);
      })
      .catch((error) => {
        if (!stopped) setError(String(error));
      });
    return () => {
      stopped = true;
      cancelAnimationFrame(animation);
      input.dispose();
      platform.dispose();
      bridge?.dispose();
    };
  }, [widget, gallery]);
  const command = useCallback(
    async (name: string, payload?: unknown) => {
      try {
        await transport!.command(name, payload);
        setState(await transport!.state());
      } catch (error) {
        setError(String(error));
      }
    },
    [transport],
  );
  const Application = transport ? components[transport.info.ui] : undefined;
  return (
    <div
      className="terrarium-host"
      style={
        transport?.info.bundled_fonts === false
          ? ({ "--terrarium-default-font": "system-ui" } as React.CSSProperties)
          : undefined
      }
    >
      {!widget && !gallery && (
        <>
          <canvas ref={canvas} id={canvasId} className="scene-canvas" aria-label="3D scene" />
          <canvas ref={overlay} className="egui-canvas" aria-label="Internal tools" />
        </>
      )}
      {gallery && <Gallery />}
      {widget && <WidgetGallery name={widget} />}
      {transport && (
        <EngineContext.Provider value={{ state, fps, command, transport }}>
          <div className="ui-layer">
            {Application ? (
              <Application />
            ) : (
              <Alert color="red">Unknown UI component: {transport.info.ui}</Alert>
            )}
          </div>
        </EngineContext.Provider>
      )}
      {!transport && !error && !widget && !gallery && (
        <div className="loading" role="status">
          Starting renderer…
        </div>
      )}
      {error && (
        <Alert
          role="alert"
          color="red"
          title="Terrarium"
          className="error-message"
          withCloseButton
          onClose={() => setError(undefined)}
        >
          {error}
        </Alert>
      )}
    </div>
  );
}
