export type Bounds = [number, number, number, number];
export interface NativeLayout {
  scene: Bounds;
  occlusions: Bounds[];
  viewports: { key: string; id: string; bounds: Bounds; clip: Bounds; occlusions: Bounds[] }[];
}
const viewportKeys = new WeakMap<HTMLCanvasElement, string>();
let nextViewport = 0;
function intersection(a: Bounds, b: Bounds): Bounds {
  const x = Math.max(a[0], b[0]),
    y = Math.max(a[1], b[1]);
  return [
    x,
    y,
    Math.max(0, Math.min(a[0] + a[2], b[0] + b[2]) - x),
    Math.max(0, Math.min(a[1] + a[3], b[1] + b[3]) - y),
  ];
}
export function nativeLayout(canvas: HTMLCanvasElement): NativeLayout {
  const origin = canvas.getBoundingClientRect();
  const bounds = (element: Element): Bounds => {
    const rect = element.getBoundingClientRect();
    return [rect.left - origin.left, rect.top - origin.top, rect.width, rect.height];
  };
  const panels = [
    ...canvas.parentElement!.querySelectorAll<HTMLElement>(".ui-layer > *, .error-message"),
    ...document.querySelectorAll<HTMLElement>(
      "[data-portal] [data-position], .mantine-Modal-overlay, .mantine-Modal-content, .mantine-Drawer-overlay, .mantine-Drawer-content",
    ),
  ].filter((element) => getComputedStyle(element).display !== "none");
  const viewports = [
    ...canvas.parentElement!.querySelectorAll<HTMLCanvasElement>("canvas[data-terrarium-viewport]"),
  ].map((element) => {
    const full = bounds(element);
    let clip = intersection(full, [0, 0, origin.width, origin.height]);
    for (
      let parent = element.parentElement;
      parent && parent !== canvas.parentElement;
      parent = parent.parentElement
    ) {
      const style = getComputedStyle(parent);
      if ([style.overflowX, style.overflowY].some((value) => value !== "" && value !== "visible")) {
        const rect = parent.getBoundingClientRect();
        clip = intersection(clip, [
          rect.left + parent.clientLeft - origin.left,
          rect.top + parent.clientTop - origin.top,
          parent.clientWidth,
          parent.clientHeight,
        ]);
      }
    }
    const owner = panels.find((panel) => panel.contains(element));
    const level = owner ? Number.parseInt(getComputedStyle(owner).zIndex) || 0 : 0;
    const occlusions = panels
      .filter((panel) => {
        if (panel.contains(element)) return false;
        const other = Number.parseInt(getComputedStyle(panel).zIndex) || 0;
        return (
          other > level ||
          (other === level &&
            !!owner &&
            !!(owner.compareDocumentPosition(panel) & Node.DOCUMENT_POSITION_FOLLOWING))
        );
      })
      .map(bounds);
    let key = viewportKeys.get(element);
    if (!key) {
      key = String(++nextViewport);
      viewportKeys.set(element, key);
    }
    return { key, id: element.dataset.terrariumViewport!, bounds: full, clip, occlusions };
  });
  return {
    scene: [origin.left, origin.top, origin.width, origin.height],
    occlusions: panels.map(bounds),
    viewports,
  };
}
