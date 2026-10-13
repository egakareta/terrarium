import { useEffect, useRef, useState } from "react";
import { Text } from "@mantine/core";
import { useEngine } from "../context";

export function Viewport({ id, size }: { id: string; size: [number, number] }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const { transport } = useEngine();
  const [error, setError] = useState<string>();
  useEffect(() => {
    if (transport.native) return;
    try {
      const detach = transport.mountViewport?.(id, ref.current!);
      if (detach) return detach;
    } catch (error) {
      setError(String(error));
      return;
    }
    let stopped = false,
      timer: ReturnType<typeof setTimeout>;
    const read = async () => {
      try {
        const bytes = await transport.viewport(id);
        if (stopped) return;
        if (!bytes) {
          timer = setTimeout(read, 16);
          return;
        }
        const pixels = new Uint8ClampedArray(bytes);
        if (pixels.length !== size[0] * size[1] * 4)
          throw new Error("Viewport dimensions do not match its pixels");
        ref.current?.getContext("2d")?.putImageData(new ImageData(pixels, size[0], size[1]), 0, 0);
        timer = setTimeout(read, 16);
      } catch (error) {
        if (!stopped) setError(String(error));
      }
    };
    void read();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [id, size[0], size[1], transport]);
  return (
    <>
      {error && (
        <Text c="red" role="alert">
          {error}
        </Text>
      )}
      <canvas
        ref={ref}
        data-terrarium-viewport={id}
        width={size[0]}
        height={size[1]}
        aria-label={`Viewport ${id}`}
        className="offscreen-viewport"
      />
    </>
  );
}
