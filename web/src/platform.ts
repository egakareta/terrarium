import type { PlatformOutput } from "./bridge";
import { nativeLayout, type Bounds } from "./native-layout";

function subtract(regions: Bounds[], hole: Bounds): Bounds[] {
  return regions.flatMap(([x, y, w, h]) => {
    const left = Math.max(x, hole[0]),
      top = Math.max(y, hole[1]);
    const right = Math.min(x + w, hole[0] + hole[2]),
      bottom = Math.min(y + h, hole[1] + hole[3]);
    if (right <= left || bottom <= top) return [[x, y, w, h] as Bounds];
    return [
      [x, y, w, top - y],
      [x, bottom, w, y + h - bottom],
      [x, top, left - x, bottom - top],
      [right, top, x + w - right, bottom - top],
    ].filter((r) => r[2] > 0 && r[3] > 0) as Bounds[];
  });
}

const cursors: Record<string, string> = {
  Default: "default",
  None: "none",
  PointingHand: "pointer",
  ResizeHorizontal: "ew-resize",
  ResizeVertical: "ns-resize",
  ResizeNwSe: "nwse-resize",
  ResizeNeSw: "nesw-resize",
  Grab: "grab",
  Grabbing: "grabbing",
  Text: "text",
  VerticalText: "vertical-text",
  Crosshair: "crosshair",
  NotAllowed: "not-allowed",
  Wait: "wait",
  Progress: "progress",
  Help: "help",
  Move: "move",
  AllScroll: "all-scroll",
};
let nextClip = 0;
export function platformOutput(canvas: HTMLCanvasElement, overlay: HTMLCanvasElement) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  const clip = document.createElementNS(svg.namespaceURI, "clipPath");
  const sceneClip = document.createElementNS(svg.namespaceURI, "clipPath");
  const id = `terrarium-egui-${nextClip++}`;
  clip.id = id;
  sceneClip.id = `${id}-scene`;
  clip.setAttribute("clipPathUnits", "userSpaceOnUse");
  svg.append(clip, sceneClip);
  Object.assign(svg.style, { position: "absolute", width: "0", height: "0" });
  document.body.append(svg);
  overlay.style.clipPath = `url(#${id})`;
  const inputRegions = document.createElement("div");
  Object.assign(inputRegions.style, {
    position: "absolute",
    inset: "0",
    pointerEvents: "none",
    zIndex: "30",
  });
  canvas.parentElement!.append(inputRegions);
  let previousInput = "";
  let previousRegions = "";
  let previousScene = "";
  const rectangles = (regions: Bounds[]) =>
    regions.map(([x, y, width, height]) => {
      const rect = document.createElementNS(svg.namespaceURI, "rect");
      for (const [name, value] of Object.entries({ x, y, width, height }))
        rect.setAttribute(name, String(value));
      return rect;
    });
  const agent = document.createElement("textarea");
  agent.dataset.eguiInput = "true";
  agent.setAttribute("aria-label", "Egui text input");
  Object.assign(agent.style, {
    position: "fixed",
    width: "1px",
    height: "1px",
    opacity: "0",
    padding: "0",
    border: "0",
    pointerEvents: "none",
  });
  document.body.append(agent);
  return {
    apply(
      output: PlatformOutput,
      pixelsPerPoint: number,
      regions: [number, number, number, number][],
      interactive: Bounds[],
    ) {
      const inputGeometry = JSON.stringify(interactive);
      if (inputGeometry !== previousInput) {
        inputRegions.replaceChildren(
          ...interactive.map(([x, y, width, height]) => {
            const region = document.createElement("div");
            region.dataset.eguiRegion = "true";
            Object.assign(region.style, {
              position: "absolute",
              left: `${x}px`,
              top: `${y}px`,
              width: `${width}px`,
              height: `${height}px`,
              pointerEvents: "auto",
              touchAction: "none",
            });
            return region;
          }),
        );
        previousInput = inputGeometry;
      }
      const geometry = JSON.stringify(regions);
      if (geometry !== previousRegions) {
        clip.replaceChildren(...rectangles(regions));
        previousRegions = geometry;
      }
      if (canvas.classList.contains("combined-canvas")) {
        const layout = nativeLayout(canvas);
        let visible: Bounds[] = [[0, 0, layout.scene[2], layout.scene[3]]];
        for (const hole of layout.occlusions) visible = subtract(visible, hole);
        visible.push(...regions);
        const geometry = JSON.stringify(visible);
        if (geometry !== previousScene) {
          sceneClip.replaceChildren(...rectangles(visible));
          previousScene = geometry;
        }
        canvas.style.clipPath = `url(#${sceneClip.id})`;
      }
      const cursor = cursors[output.cursor_icon] ?? "default";
      if (inputRegions.style.cursor !== cursor) inputRegions.style.cursor = cursor;
      for (const command of output.commands) {
        if (command.CopyText !== undefined)
          void navigator.clipboard?.writeText(command.CopyText).catch(() => {});
        if (command.OpenUrl)
          window.open(
            command.OpenUrl.url,
            command.OpenUrl.new_tab ? "_blank" : "_self",
            "noopener,noreferrer",
          );
      }
      if (output.ime) {
        const rect = canvas.getBoundingClientRect();
        const scale = pixelsPerPoint / devicePixelRatio;
        agent.style.left = `${rect.left + output.ime.cursor_rect.min.x * scale}px`;
        agent.style.top = `${rect.top + output.ime.cursor_rect.max.y * scale}px`;
        if (document.activeElement !== agent) agent.focus({ preventScroll: true });
      } else if (document.activeElement === agent) agent.blur();
    },
    dispose() {
      agent.remove();
      svg.remove();
      inputRegions.remove();
    },
  };
}
