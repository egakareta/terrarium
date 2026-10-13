import { useEffect, useRef, useState } from "react";
import { Alert, MantineProvider, createTheme } from "@mantine/core";
import "@mantine/core/styles.css";
import "dockview-react/dist/styles/dockview.css";
import "./style.css";
import { connect, type Snapshot, type Transport } from "./bridge";
import { collectInput } from "./input";
import { EngineContext } from "./context";
import { applications } from "./examples/Applications";
import { Devtools } from "./components/Devtools";
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

export function TerrariumHost({
  components = applications,
  canvasId = "app",
  gallery = new URLSearchParams(location.search).has("gallery") ||
    location.pathname.endsWith("/examples.html"),
}: {
  components?: Record<string, () => React.ReactNode>;
  canvasId?: string;
  gallery?: boolean;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [transport, setTransport] = useState<Transport>();
  const [snapshot, setSnapshot] = useState<Snapshot>();
  const [error, setError] = useState<string>();
  const widget = new URLSearchParams(location.search).get("widget");
  useEffect(() => {
    if (widget || gallery) return;
    let stopped = false,
      animation = 0;
    let bridge: Transport | undefined;
    const input = collectInput(canvas.current!);
    void connect(canvas.current!)
      .then(async (connection) => {
        bridge = connection;
        if (stopped) {
          connection.dispose();
          return;
        }
        setTransport(connection);
        setSnapshot(await connection.state());
        let lastSnapshot = performance.now();
        const render = async (now: number) => {
          try {
            const events = input.next(now);
            const close = await connection.advance(events);
            if (stopped) return;
            if (close || events.pressed.length || now - lastSnapshot >= 1000 / 30) {
              const next = await connection.state();
              if (stopped) return;
              setSnapshot(next);
              lastSnapshot = now;
            }
            if (!close) animation = requestAnimationFrame(render);
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
      bridge?.dispose();
    };
  }, [widget, gallery]);
  const command = async (name: string, payload?: unknown) => {
    try {
      await transport!.command(name, payload);
      setSnapshot(await transport!.state());
    } catch (error) {
      setError(String(error));
    }
  };
  const joystick = snapshot?.touch_geometry;
  const Application = snapshot ? components[snapshot.ui] : undefined;
  return (
    <MantineProvider theme={theme} defaultColorScheme="dark">
      <div
        className="terrarium-host"
        style={
          snapshot?.bundled_fonts === false
            ? ({ "--terrarium-default-font": "system-ui" } as React.CSSProperties)
            : undefined
        }
      >
        {!widget && !gallery && (
          <canvas ref={canvas} id={canvasId} className="scene-canvas" aria-label="3D scene" />
        )}
        {gallery && <Gallery />}
        {widget && <WidgetGallery name={widget} />}
        {snapshot && transport && (
          <EngineContext.Provider value={{ snapshot, command, transport }}>
            <div className="ui-layer">
              {Application ? (
                <Application />
              ) : (
                <Alert color="red">Unknown UI component: {snapshot.ui}</Alert>
              )}
              <Devtools />
              {snapshot.touch_enabled && joystick && (
                <div
                  className="joystick"
                  aria-hidden="true"
                  style={{
                    left: joystick[0] - joystick[2],
                    top: joystick[1] - joystick[2],
                    bottom: "auto",
                    width: joystick[2] * 2,
                    height: joystick[2] * 2,
                  }}
                >
                  <span
                    style={{
                      transform: `translate(${snapshot.touch_movement[0] * joystick[2] * 0.65}px, ${snapshot.touch_movement[1] * joystick[2] * 0.65}px)`,
                      width: joystick[2] * 0.6,
                      height: joystick[2] * 0.6,
                      left: joystick[2] * 0.7,
                      top: joystick[2] * 0.7,
                    }}
                  />
                </div>
              )}
            </div>
          </EngineContext.Provider>
        )}
        {!snapshot && !error && !widget && !gallery && (
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
    </MantineProvider>
  );
}
