import type { Hotkey } from "@/lib/types";

const CODE_LABELS: Record<string, string> = {
  Space: "Space",
  Enter: "Enter",
  Tab: "Tab",
  Escape: "Esc",
  Backspace: "Backspace",
  Delete: "Del",
  Insert: "Ins",
  Home: "Home",
  End: "End",
  PageUp: "PgUp",
  PageDown: "PgDn",
  ArrowLeft: "Left",
  ArrowUp: "Up",
  ArrowRight: "Right",
  ArrowDown: "Down",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Comma: ",",
  Period: ".",
  Slash: "/",
  IntlBackslash: "\\",
  MouseMiddle: "Mouse 3",
  MouseX1: "Mouse 4",
  MouseX2: "Mouse 5",
  ContextMenu: "Menu",
  CapsLock: "Caps Lock",
  NumLock: "Num Lock",
  ScrollLock: "Scroll Lock",
  PrintScreen: "PrtSc",
  Pause: "Pause",
};

export function emptyHotkey(): Hotkey {
  return { ctrl: false, alt: false, shift: false, win: false, code: "", label: "" };
}

export function isModifierCode(code: string): boolean {
  return (
    code === "ControlLeft" ||
    code === "ControlRight" ||
    code === "ShiftLeft" ||
    code === "ShiftRight" ||
    code === "AltLeft" ||
    code === "AltRight" ||
    code === "MetaLeft" ||
    code === "MetaRight" ||
    code === "OSLeft" ||
    code === "OSRight"
  );
}

export function labelForCode(code: string): string {
  if (CODE_LABELS[code]) return CODE_LABELS[code];
  if (code.startsWith("Key") && code.length === 4) return code.slice(3);
  if (code.startsWith("Digit") && code.length === 6) return code.slice(5);
  if (code.startsWith("Numpad")) return `Num ${code.slice(6)}`;
  if (/^F\d{1,2}$/.test(code)) return code;
  return code;
}

export function fromKeyboardEvent(event: KeyboardEvent): Hotkey | null {
  if (isModifierCode(event.code)) return null;
  const label = labelForCode(event.code);
  return {
    ctrl: event.ctrlKey,
    alt: event.altKey,
    shift: event.shiftKey,
    win: event.metaKey,
    code: event.code,
    label,
  };
}

export function fromMouseEvent(event: MouseEvent, mods: Pick<Hotkey, "ctrl" | "alt" | "shift" | "win">): Hotkey | null {
  let code = "";
  let label = "";
  if (event.button === 1) {
    code = "MouseMiddle";
    label = "Mouse 3";
  } else if (event.button === 3) {
    code = "MouseX1";
    label = "Mouse 4";
  } else if (event.button === 4) {
    code = "MouseX2";
    label = "Mouse 5";
  } else {
    return null;
  }
  return { ...mods, code, label };
}

export function formatHotkey(hotkey: Hotkey | null | undefined): string {
  if (!hotkey || !hotkey.code) return "";
  const parts: string[] = [];
  if (hotkey.ctrl) parts.push("Ctrl");
  if (hotkey.alt) parts.push("Alt");
  if (hotkey.shift) parts.push("Shift");
  if (hotkey.win) parts.push("Win");
  parts.push(hotkey.label || labelForCode(hotkey.code));
  return parts.join(" + ");
}

export function hotkeyEqual(a: Hotkey | null | undefined, b: Hotkey | null | undefined): boolean {
  if (!a || !b) return false;
  return (
    a.ctrl === b.ctrl &&
    a.alt === b.alt &&
    a.shift === b.shift &&
    a.win === b.win &&
    a.code === b.code
  );
}
