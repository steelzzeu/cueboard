//! Global shortcut registration for Windows.
//!
//! Keyboard shortcuts use `RegisterHotKey`, so Windows only delivers the combos the
//! user configured. Unrelated keystrokes are never seen, stored or logged.
//! Mouse-button shortcuts (middle / X1 / X2) use a low-level mouse hook that only
//! matches those configured buttons and is installed only while such bindings exist.

use std::{
    collections::HashMap,
    sync::OnceLock,
    thread,
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;
use serde::Serialize;

use crate::models::{Hotkey, HotkeyTarget};

const WM_APP_SYNC: u32 = 0x8000 + 1;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyIssue {
    pub target: String,
    pub label: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub issues: Vec<HotkeyIssue>,
}

pub struct Binding {
    pub hotkey: Hotkey,
    pub target: HotkeyTarget,
}

pub enum HotkeyFire {
    Target(HotkeyTarget),
}

enum Cmd {
    Sync(Vec<Binding>, Sender<SyncResult>),
    Shutdown,
}

#[derive(Clone)]
pub struct HotkeyHandle {
    tx: Sender<Cmd>,
    thread_id: u32,
}

impl HotkeyHandle {
    pub fn sync(&self, bindings: Vec<Binding>) -> SyncResult {
        let (tx, rx) = crossbeam_channel::bounded(1);
        if self.tx.send(Cmd::Sync(bindings, tx)).is_err() {
            return SyncResult::default();
        }
        wake(self.thread_id, WM_APP_SYNC);
        rx.recv_timeout(Duration::from_secs(2)).unwrap_or_default()
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(Cmd::Shutdown);
        wake(self.thread_id, 0x0012); // WM_QUIT
    }
}

fn wake(thread_id: u32, msg: u32) {
    #[cfg(windows)]
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
            thread_id,
            msg,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(0),
        );
    }
    #[cfg(not(windows))]
    let _ = (thread_id, msg);
}

pub fn spawn(on_fire: impl Fn(HotkeyFire) + Send + 'static) -> HotkeyHandle {
    let (tx, rx) = crossbeam_channel::unbounded::<Cmd>();
    let (id_tx, id_rx) = crossbeam_channel::bounded::<u32>(1);

    thread::Builder::new()
        .name("cueboard-hotkeys".into())
        .spawn(move || run(rx, id_tx, on_fire))
        .expect("failed to spawn hotkey thread");

    let thread_id = id_rx.recv_timeout(Duration::from_secs(2)).unwrap_or(0);
    HotkeyHandle { tx, thread_id }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct MouseSig {
    ctrl: bool,
    alt: bool,
    shift: bool,
    win: bool,
    button: u8,
}

struct MouseSlot {
    map: HashMap<MouseSig, HotkeyTarget>,
    fire: Sender<HotkeyTarget>,
}

static MOUSE: OnceLock<Mutex<Option<MouseSlot>>> = OnceLock::new();

fn mouse_store() -> &'static Mutex<Option<MouseSlot>> {
    MOUSE.get_or_init(|| Mutex::new(None))
}

fn run(
    rx: Receiver<Cmd>,
    id_tx: Sender<u32>,
    on_fire: impl Fn(HotkeyFire) + Send + 'static,
) {
    #[cfg(windows)]
    {
        run_windows(rx, id_tx, on_fire);
    }
    #[cfg(not(windows))]
    {
        let _ = id_tx.send(0);
        while let Ok(cmd) = rx.recv() {
            match cmd {
                Cmd::Sync(_, reply) => {
                    let _ = reply.send(SyncResult {
                        issues: vec![HotkeyIssue {
                            target: "system".into(),
                            label: String::new(),
                            message: "Global hotkeys are only available on Windows.".into(),
                        }],
                    });
                }
                Cmd::Shutdown => break,
            }
        }
        let _ = on_fire;
    }
}

#[cfg(windows)]
fn run_windows(
    rx: Receiver<Cmd>,
    id_tx: Sender<u32>,
    on_fire: impl Fn(HotkeyFire) + Send + 'static,
) {
    use windows::Win32::{
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, TranslateMessage, MSG, WM_HOTKEY,
        },
    };

    let thread_id = unsafe { GetCurrentThreadId() };
    let _ = id_tx.send(thread_id);

    let (fire_tx, fire_rx) = crossbeam_channel::unbounded::<HotkeyTarget>();
    *mouse_store().lock() = Some(MouseSlot {
        map: HashMap::new(),
        fire: fire_tx,
    });

    let mut ids: Vec<i32> = Vec::new();
    let mut by_id: HashMap<i32, HotkeyTarget> = HashMap::new();
    let mut hook = None;
    let mut next_id: i32 = 1;

    loop {
        let mut msg = MSG::default();
        let ok = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if !ok.as_bool() {
            break;
        }

        if msg.message == WM_HOTKEY {
            let id = msg.wParam.0 as i32;
            if let Some(target) = by_id.get(&id) {
                on_fire(HotkeyFire::Target(target.clone()));
            }
        } else {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        while let Ok(target) = fire_rx.try_recv() {
            on_fire(HotkeyFire::Target(target));
        }

        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                Cmd::Shutdown => {
                    clear_hotkeys(&mut ids);
                    uninstall_hook(&mut hook);
                    *mouse_store().lock() = None;
                    return;
                }
                Cmd::Sync(bindings, reply) => {
                    clear_hotkeys(&mut ids);
                    by_id.clear();
                    let mut mouse_map = HashMap::new();
                    let mut issues = Vec::new();

                    for binding in bindings {
                        let label = binding.hotkey.display();
                        let target_key = binding.target.key();
                        if let Some(button) = mouse_button(&binding.hotkey.code) {
                            let sig = MouseSig {
                                ctrl: binding.hotkey.ctrl,
                                alt: binding.hotkey.alt,
                                shift: binding.hotkey.shift,
                                win: binding.hotkey.win,
                                button,
                            };
                            mouse_map.insert(sig, binding.target);
                            continue;
                        }
                        match to_vk(&binding.hotkey.code) {
                            None => issues.push(HotkeyIssue {
                                target: target_key,
                                label,
                                message: format!(
                                    "{} cannot be registered as a Windows shortcut.",
                                    binding.hotkey.code
                                ),
                            }),
                            Some(vk) => {
                                let mods = modifiers(&binding.hotkey);
                                let id = next_id;
                                next_id = if next_id >= 0xBFFF { 1 } else { next_id + 1 };
                                let registered = unsafe {
                                    windows::Win32::UI::Input::KeyboardAndMouse::RegisterHotKey(
                                        None,
                                        id,
                                        mods,
                                        vk,
                                    )
                                };
                                match registered {
                                    Ok(()) => {
                                        ids.push(id);
                                        by_id.insert(id, binding.target);
                                    }
                                    Err(err) => {
                                        issues.push(HotkeyIssue {
                                            target: target_key,
                                            label: label.clone(),
                                            message: format!(
                                                "Windows refused {}. Another program may already own this shortcut, or the system reserved it. {}",
                                                label,
                                                err.message()
                                            ),
                                        });
                                    }
                                }
                            }
                        }
                    }

                    if mouse_map.is_empty() {
                        uninstall_hook(&mut hook);
                    } else if hook.is_none() {
                        hook = install_hook();
                        if hook.is_none() {
                            issues.push(HotkeyIssue {
                                target: "mouse".into(),
                                label: "Mouse".into(),
                                message: "Could not install the mouse hook used for mouse-button shortcuts.".into(),
                            });
                            mouse_map.clear();
                        }
                    }

                    if let Some(slot) = mouse_store().lock().as_mut() {
                        slot.map = mouse_map;
                    }

                    let _ = reply.send(SyncResult { issues });
                }
            }
        }
    }

    clear_hotkeys(&mut ids);
    uninstall_hook(&mut hook);
    *mouse_store().lock() = None;
}

#[cfg(windows)]
fn clear_hotkeys(ids: &mut Vec<i32>) {
    use windows::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey;
    for id in ids.drain(..) {
        unsafe {
            let _ = UnregisterHotKey(None, id);
        }
    }
}

#[cfg(windows)]
fn modifiers(hotkey: &Hotkey) -> windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    };
    let mut mods = HOT_KEY_MODIFIERS(MOD_NOREPEAT.0);
    if hotkey.ctrl {
        mods |= MOD_CONTROL;
    }
    if hotkey.alt {
        mods |= MOD_ALT;
    }
    if hotkey.shift {
        mods |= MOD_SHIFT;
    }
    if hotkey.win {
        mods |= MOD_WIN;
    }
    mods
}

fn mouse_button(code: &str) -> Option<u8> {
    match code {
        "MouseMiddle" => Some(2),
        "MouseX1" => Some(4),
        "MouseX2" => Some(5),
        _ => None,
    }
}

#[cfg(windows)]
fn install_hook() -> Option<windows::Win32::UI::WindowsAndMessaging::HHOOK> {
    use windows::Win32::{
        Foundation::HINSTANCE,
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{SetWindowsHookExW, WH_MOUSE_LL},
    };
    unsafe {
        let module = GetModuleHandleW(None).ok()?;
        SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(mouse_proc),
            Some(HINSTANCE(module.0)),
            0,
        )
        .ok()
    }
}

#[cfg(windows)]
fn uninstall_hook(hook: &mut Option<windows::Win32::UI::WindowsAndMessaging::HHOOK>) {
    use windows::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx;
    if let Some(h) = hook.take() {
        unsafe {
            let _ = UnhookWindowsHookEx(h);
        }
    }
}

#[cfg(windows)]
unsafe extern "system" fn mouse_proc(
    code: i32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
        },
        WindowsAndMessaging::{
            CallNextHookEx, MSLLHOOKSTRUCT, WM_MBUTTONDOWN, WM_XBUTTONDOWN, XBUTTON1, XBUTTON2,
        },
    };

    if code >= 0 {
        let button = match wparam.0 as u32 {
            v if v == WM_MBUTTONDOWN => Some(2u8),
            v if v == WM_XBUTTONDOWN => {
                let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
                let x = ((info.mouseData >> 16) & 0xffff) as u16;
                if x == XBUTTON1 {
                    Some(4)
                } else if x == XBUTTON2 {
                    Some(5)
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(button) = button {
            let down = |vk: u16| unsafe { GetAsyncKeyState(vk as i32) } < 0;
            let sig = MouseSig {
                ctrl: down(VK_CONTROL.0),
                alt: down(VK_MENU.0),
                shift: down(VK_SHIFT.0),
                win: down(VK_LWIN.0) || down(VK_RWIN.0),
                button,
            };
            if let Some(slot) = mouse_store().lock().as_ref() {
                if let Some(target) = slot.map.get(&sig) {
                    let _ = slot.fire.try_send(target.clone());
                }
            }
        }
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Map a `KeyboardEvent.code` value to a Windows virtual-key code.
pub fn to_vk(code: &str) -> Option<u32> {
    if let Some(rest) = code.strip_prefix("Key") {
        if rest.len() == 1 {
            let c = rest.as_bytes()[0];
            if c.is_ascii_uppercase() {
                return Some(c as u32);
            }
        }
    }
    if let Some(rest) = code.strip_prefix("Digit") {
        if rest.len() == 1 && rest.as_bytes()[0].is_ascii_digit() {
            return Some(rest.as_bytes()[0] as u32);
        }
    }
    if let Some(rest) = code.strip_prefix('F') {
        if let Ok(n) = rest.parse::<u32>() {
            if (1..=24).contains(&n) {
                return Some(0x6F + n); // F1 = 0x70
            }
        }
    }
    if let Some(rest) = code.strip_prefix("Numpad") {
        return match rest {
            "0" => Some(0x60),
            "1" => Some(0x61),
            "2" => Some(0x62),
            "3" => Some(0x63),
            "4" => Some(0x64),
            "5" => Some(0x65),
            "6" => Some(0x66),
            "7" => Some(0x67),
            "8" => Some(0x68),
            "9" => Some(0x69),
            "Multiply" => Some(0x6A),
            "Add" => Some(0x6B),
            "Subtract" => Some(0x6D),
            "Decimal" => Some(0x6E),
            "Divide" => Some(0x6F),
            "Enter" => Some(0x0D),
            "Equal" => Some(0x0D),
            _ => None,
        };
    }
    Some(match code {
        "Space" => 0x20,
        "Enter" => 0x0D,
        "Tab" => 0x09,
        "Escape" | "Esc" => 0x1B,
        "Backspace" => 0x08,
        "Delete" => 0x2E,
        "Insert" => 0x2D,
        "Home" => 0x24,
        "End" => 0x23,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "ArrowLeft" => 0x25,
        "ArrowUp" => 0x26,
        "ArrowRight" => 0x27,
        "ArrowDown" => 0x28,
        "Pause" => 0x13,
        "CapsLock" => 0x14,
        "NumLock" => 0x90,
        "ScrollLock" => 0x91,
        "PrintScreen" => 0x2C,
        "ContextMenu" => 0x5D,
        "Minus" => 0xBD,
        "Equal" => 0xBB,
        "BracketLeft" => 0xDB,
        "BracketRight" => 0xDD,
        "Backslash" => 0xDC,
        "Semicolon" => 0xBA,
        "Quote" => 0xDE,
        "Backquote" => 0xC0,
        "Comma" => 0xBC,
        "Period" => 0xBE,
        "Slash" => 0xBF,
        "IntlBackslash" => 0xE2,
        _ => return None,
    })
}

pub fn is_modifier_code(code: &str) -> bool {
    matches!(
        code,
        "ControlLeft"
            | "ControlRight"
            | "ShiftLeft"
            | "ShiftRight"
            | "AltLeft"
            | "AltRight"
            | "MetaLeft"
            | "MetaRight"
            | "OSLeft"
            | "OSRight"
    )
}

pub fn reserved_reason(hotkey: &Hotkey) -> Option<&'static str> {
    if hotkey.code.is_empty() || is_modifier_code(&hotkey.code) {
        return Some("A shortcut needs a non-modifier key.");
    }
    if hotkey.alt && !hotkey.ctrl && !hotkey.shift && !hotkey.win && hotkey.code == "F4" {
        return Some("Alt + F4 is reserved by Windows for closing the active window.");
    }
    if hotkey.ctrl && hotkey.alt && hotkey.code == "Delete" {
        return Some("Ctrl + Alt + Delete is reserved by Windows.");
    }
    if hotkey.win && !hotkey.ctrl && !hotkey.alt && !hotkey.shift && hotkey.code == "KeyL" {
        return Some("Win + L is reserved by Windows for locking the session.");
    }
    if hotkey.ctrl && hotkey.shift && hotkey.code == "Escape" {
        return Some("Ctrl + Shift + Escape is reserved for Task Manager.");
    }
    None
}

pub fn risky_reason(hotkey: &Hotkey) -> Option<&'static str> {
    let bare = !hotkey.ctrl && !hotkey.alt && !hotkey.shift && !hotkey.win;
    if bare
        && (hotkey.code.starts_with("Key")
            || hotkey.code.starts_with("Digit")
            || matches!(hotkey.code.as_str(), "Space" | "Enter" | "Tab" | "Backspace"))
    {
        return Some(
            "This key will fire while you type in other applications. A modifier or function key is safer.",
        );
    }
    if hotkey.ctrl
        && !hotkey.alt
        && !hotkey.shift
        && !hotkey.win
        && matches!(
            hotkey.code.as_str(),
            "KeyC" | "KeyV" | "KeyX" | "KeyA" | "KeyZ" | "KeyY" | "KeyS" | "KeyF" | "KeyP"
        )
    {
        return Some("This combination is used for common editing commands in most applications.");
    }
    if hotkey.alt && !hotkey.ctrl && !hotkey.shift && hotkey.code == "Tab" {
        return Some("Alt + Tab is used by Windows to switch windows.");
    }
    None
}

pub fn same_signature(a: &Hotkey, b: &Hotkey) -> bool {
    a.signature() == b.signature()
}

