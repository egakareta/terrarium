import type { FrameInput, TouchEvent } from "./bridge";

export function collectInput(canvas: HTMLCanvasElement, overlay: HTMLCanvasElement) {
  const keys = new Set<string>(),
    buttons = new Set<number>();
  let pressed: string[] = [],
    gui: unknown[] = [],
    touches: TouchEvent[] = [],
    delta: [number, number] = [0, 0];
  let previous = performance.now();
  let modifiers = { ctrl: false, command: false, shift: false };
  const touchCapture = new Map<number, boolean>();
  const editing = () =>
    document.activeElement?.matches(
      "input:not([data-egui-input]),textarea:not([data-egui-input]),select,[contenteditable='true']",
    ) ?? false;
  const eguiModifiers = (event: KeyboardEvent | MouseEvent) => ({
    alt: event.altKey,
    ctrl: event.ctrlKey,
    shift: event.shiftKey,
    mac_cmd: event.metaKey,
    command: event.metaKey || event.ctrlKey,
  });
  const eguiKey = (key: string) =>
    ({ " ": "Space", Control: "", Shift: "", Alt: "", Meta: "", Esc: "Escape" })[key] ??
    (key.length === 1 && /[a-z]/i.test(key)
      ? key.toUpperCase()
      : /^\d$/.test(key)
        ? `Num${key}`
        : key);
  const keyboard = (event: KeyboardEvent) => {
    modifiers = { ctrl: event.ctrlKey, command: event.metaKey, shift: event.shiftKey };
    if (event.type === "keydown") {
      keys.add(event.code);
      if (!event.repeat) pressed.push(event.code);
      if (
        event.code === "F12" ||
        (!editing() &&
          event.code === "Space" &&
          !document.activeElement?.matches("[data-egui-input]"))
      )
        event.preventDefault();
    } else keys.delete(event.code);
    if ((!editing() || event.code === "F12") && !event.isComposing) {
      gui.push({ ModifiersChanged: eguiModifiers(event) });
      const key = eguiKey(event.key);
      if (
        /^(Arrow(Up|Down|Left|Right)|Escape|Tab|Backspace|Enter|Space|Insert|Delete|Home|End|PageUp|PageDown|F\d{1,2}|[A-Z]|Num\d)$/.test(
          key,
        )
      )
        gui.push({
          Key: {
            key,
            physical_key: null,
            pressed: event.type === "keydown",
            repeat: event.repeat,
            modifiers: eguiModifiers(event),
          },
        });
      if (event.type === "keydown") {
        if ((event.ctrlKey || event.metaKey) && ["c", "x"].includes(event.key.toLowerCase())) {
          gui.push(event.key.toLowerCase() === "c" ? "Copy" : "Cut");
          event.preventDefault();
        } else if (
          !event.ctrlKey &&
          !event.metaKey &&
          Array.from(event.key).length === 1 &&
          !document.activeElement?.matches("[data-egui-input]")
        )
          gui.push({ Text: event.key });
      }
    }
  };
  const pointer = (event: PointerEvent) => {
    const rect = canvas.getBoundingClientRect();
    const onCanvas =
      event.target === canvas ||
      event.target === overlay ||
      (event.target instanceof Element && event.target.matches("[data-egui-region]"));
    if (onCanvas && event.type === "pointerdown" && editing())
      (document.activeElement as HTMLElement)?.blur();
    const pos = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    if (onCanvas && (event.pointerType !== "touch" || event.isPrimary)) {
      gui.push({ PointerMoved: pos });
      if (event.type === "pointerdown" || event.type === "pointerup") {
        const button = ["Primary", "Middle", "Secondary", "Extra1", "Extra2"][event.button];
        if (button)
          gui.push({
            PointerButton: {
              pos,
              button,
              pressed: event.type === "pointerdown",
              modifiers: eguiModifiers(event),
            },
          });
      }
      if (event.type === "pointercancel") gui.push("PointerGone");
    } else if (event.type === "pointermove") gui.push("PointerGone");
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
    if (event.type === "pointerdown" && onCanvas)
      (event.target as HTMLCanvasElement).setPointerCapture(event.pointerId);
  };
  const blur = () => {
    keys.clear();
    buttons.clear();
    touchCapture.clear();
    touches = [];
    delta = [0, 0];
    gui = ["PointerGone", { WindowFocused: false }];
  };
  const events = ["pointerdown", "pointermove", "pointerup", "pointercancel"] as const;
  const wheel = (event: WheelEvent) => {
    if (
      event.target === canvas ||
      event.target === overlay ||
      (event.target instanceof Element && event.target.matches("[data-egui-region]"))
    ) {
      gui.push({
        MouseWheel: {
          unit: ["Point", "Line", "Page"][event.deltaMode],
          delta: { x: -event.deltaX, y: -event.deltaY },
          modifiers: eguiModifiers(event),
        },
      });
      event.preventDefault();
    }
  };
  let composing = false;
  const text = (event: Event) => {
    const target = event.target as HTMLTextAreaElement;
    if (!target.matches("[data-egui-input]")) return;
    if (event.type === "compositionstart") {
      composing = true;
      gui.push({ Ime: "Enabled" });
    }
    if (event.type === "compositionupdate")
      gui.push({ Ime: { Preedit: (event as CompositionEvent).data } });
    if (event.type === "compositionend") {
      composing = false;
      gui.push({ Ime: { Commit: (event as CompositionEvent).data } }, { Ime: "Disabled" });
      target.value = "";
    }
    if (
      event.type === "input" &&
      !composing &&
      (event as InputEvent).inputType !== "insertCompositionText" &&
      target.value
    ) {
      gui.push({ Text: target.value });
      target.value = "";
    }
  };
  const paste = (event: ClipboardEvent) => {
    if (!editing()) {
      const text = event.clipboardData?.getData("text/plain");
      if (text) {
        gui.push({ Paste: text });
        event.preventDefault();
      }
    }
  };
  document.addEventListener("wheel", wheel, { passive: false });
  document.addEventListener("paste", paste);
  const textEvents = ["input", "compositionstart", "compositionupdate", "compositionend"];
  textEvents.forEach((name) => document.addEventListener(name, text));
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
        gui_events: gui,
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
      gui = [];
      touches = [];
      delta = [0, 0];
      return input;
    },
    dispose() {
      events.forEach((name) => document.removeEventListener(name, pointer));
      document.removeEventListener("wheel", wheel);
      document.removeEventListener("paste", paste);
      textEvents.forEach((name) => document.removeEventListener(name, text));
      window.removeEventListener("keydown", keyboard);
      window.removeEventListener("keyup", keyboard);
      window.removeEventListener("blur", blur);
      document.removeEventListener("visibilitychange", visibility);
    },
  };
}
