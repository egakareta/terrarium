import type { FrameInput, TouchEvent } from "./bridge";

export function collectInput(canvas: HTMLCanvasElement) {
  const keys = new Set<string>(),
    buttons = new Set<number>();
  let pressed: string[] = [],
    touches: TouchEvent[] = [],
    delta: [number, number] = [0, 0];
  let previous = performance.now();
  let modifiers = { ctrl: false, command: false, shift: false };
  const touchCapture = new Map<number, boolean>();
  const editing = () =>
    document.activeElement?.matches("input,textarea,select,[contenteditable='true']") ?? false;
  const keyboard = (event: KeyboardEvent) => {
    modifiers = { ctrl: event.ctrlKey, command: event.metaKey, shift: event.shiftKey };
    if (event.type === "keydown") {
      keys.add(event.code);
      if (!event.repeat) pressed.push(event.code);
      if (!editing() && ["Space", "F12"].includes(event.code)) event.preventDefault();
    } else keys.delete(event.code);
  };
  const pointer = (event: PointerEvent) => {
    const rect = canvas.getBoundingClientRect();
    const onCanvas = event.target === canvas;
    if (event.pointerType === "touch") {
      if (event.type === "pointerdown") touchCapture.set(event.pointerId, !onCanvas);
      const captured = touchCapture.get(event.pointerId);
      if (captured === undefined) return;
      const phase = {
        pointerdown: "Start",
        pointermove: "Move",
        pointerup: "End",
        pointercancel: "Cancel",
      }[event.type] as TouchEvent["phase"];
      touches.push({
        device: 0,
        id: event.pointerId,
        phase,
        position: [event.clientX - rect.left, event.clientY - rect.top],
        captured,
      });
      if (event.type === "pointerup" || event.type === "pointercancel")
        touchCapture.delete(event.pointerId);
    } else {
      if (event.type === "pointerdown" && onCanvas) buttons.add(event.button);
      if (event.type === "pointerup" || event.type === "pointercancel")
        buttons.delete(event.button);
      if (event.type === "pointermove" && onCanvas && buttons.size) {
        delta[0] += event.movementX * devicePixelRatio;
        delta[1] += event.movementY * devicePixelRatio;
      }
    }
    if (event.type === "pointerdown" && onCanvas) canvas.setPointerCapture(event.pointerId);
  };
  const blur = () => {
    keys.clear();
    buttons.clear();
    touchCapture.clear();
    touches = [];
    delta = [0, 0];
  };
  const events = ["pointerdown", "pointermove", "pointerup", "pointercancel"] as const;
  events.forEach((name) => document.addEventListener(name, pointer));
  window.addEventListener("keydown", keyboard);
  window.addEventListener("keyup", keyboard);
  window.addEventListener("blur", blur);
  const visibility = () => {
    if (document.hidden) blur();
  };
  document.addEventListener("visibilitychange", visibility);
  return {
    next(now: number): FrameInput {
      const rect = canvas.getBoundingClientRect();
      const input: FrameInput = {
        delta: Math.min((now - previous) / 1000, 0.1),
        size: [rect.width, rect.height],
        scale: devicePixelRatio,
        focused: document.hasFocus() && !document.hidden,
        keys: [...keys],
        pressed,
        buttons: [...buttons],
        mouse_delta: delta,
        modifiers,
        keyboard_captured: editing(),
        pointer_captured: false,
        touches,
        touchscreen: navigator.maxTouchPoints > 0,
      };
      previous = now;
      pressed = [];
      touches = [];
      delta = [0, 0];
      return input;
    },
    dispose() {
      events.forEach((name) => document.removeEventListener(name, pointer));
      window.removeEventListener("keydown", keyboard);
      window.removeEventListener("keyup", keyboard);
      window.removeEventListener("blur", blur);
      document.removeEventListener("visibilitychange", visibility);
    },
  };
}
