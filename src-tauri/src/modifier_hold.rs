#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::time::{Duration, Instant};

use tauri::AppHandle;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use tauri::Emitter;

#[cfg(any(target_os = "windows", target_os = "macos"))]
use crate::{model::HoldPayload, windowing};

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeVoicePermissions {
    pub input_monitoring: bool,
    pub accessibility: bool,
}

pub fn permission_status() -> NativeVoicePermissions {
    #[cfg(target_os = "macos")]
    {
        return macos::permission_status();
    }
    #[cfg(not(target_os = "macos"))]
    NativeVoicePermissions {
        input_monitoring: true,
        accessibility: true,
    }
}

pub fn request_permissions() -> NativeVoicePermissions {
    #[cfg(target_os = "macos")]
    {
        return macos::request_permissions();
    }
    #[cfg(not(target_os = "macos"))]
    permission_status()
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
const ACTIVATION_DELAY: Duration = Duration::from_millis(180);

#[cfg(any(target_os = "windows", target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Dictation,
    Agent,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
#[derive(Default)]
struct HoldMachine {
    left_control: bool,
    left_shift: bool,
    left_alt: bool,
    blocked: bool,
    candidate: Option<(Mode, Instant)>,
    active: Option<Mode>,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl HoldMachine {
    fn modifier(&mut self, key: ModifierKey, down: bool, app: &AppHandle) {
        match key {
            ModifierKey::Control => self.left_control = down,
            ModifierKey::Shift => self.left_shift = down,
            ModifierKey::Alt => self.left_alt = down,
        }
        self.evaluate(app);
    }

    fn non_modifier_down(&mut self, app: &AppHandle) {
        self.blocked = true;
        self.candidate = None;
        self.end_active(app);
    }

    fn tick(&mut self, app: &AppHandle) {
        self.evaluate(app);
        if self.active.is_none()
            && let Some((mode, started)) = self.candidate
            && started.elapsed() >= ACTIVATION_DELAY
        {
            self.active = Some(mode);
            self.candidate = None;
            match mode {
                Mode::Dictation => {
                    let _ = windowing::show_hud(app);
                }
                Mode::Agent => {
                    let _ = windowing::show_agent(app);
                }
            }
            emit(app, mode, "pressed");
        }
    }

    fn evaluate(&mut self, app: &AppHandle) {
        let exact = if self.left_control && self.left_shift && !self.left_alt {
            Some(Mode::Dictation)
        } else if self.left_control && self.left_alt && !self.left_shift {
            Some(Mode::Agent)
        } else {
            None
        };
        if exact.is_none() {
            self.blocked = false;
            self.candidate = None;
            self.end_active(app);
            return;
        }
        if self.active.is_some() && self.active != exact {
            self.end_active(app);
        }
        if !self.blocked
            && self.active.is_none()
            && self.candidate.map(|candidate| candidate.0) != exact
        {
            self.candidate = exact.map(|mode| (mode, Instant::now()));
        }
    }

    fn end_active(&mut self, app: &AppHandle) {
        if let Some(mode) = self.active.take() {
            emit(app, mode, "released");
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
#[derive(Clone, Copy)]
enum ModifierKey {
    Control,
    Shift,
    Alt,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn emit(app: &AppHandle, mode: Mode, phase: &str) {
    let _ = app.emit(
        "onyx://hold",
        HoldPayload {
            mode: match mode {
                Mode::Dictation => "dictation",
                Mode::Agent => "agent",
            }
            .into(),
            phase: phase.into(),
        },
    );
}

pub fn start(app: AppHandle) {
    #[cfg(target_os = "windows")]
    windows::start(app);
    #[cfg(target_os = "macos")]
    macos::start(app);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = app;
}

#[cfg(target_os = "windows")]
mod windows {
    use std::{
        ffi::c_void,
        sync::{
            Mutex, OnceLock,
            mpsc::{self, Sender},
        },
        thread,
    };

    use super::*;

    const WH_KEYBOARD_LL: i32 = 13;
    const WM_KEYDOWN: u32 = 0x0100;
    const WM_KEYUP: u32 = 0x0101;
    const WM_SYSKEYDOWN: u32 = 0x0104;
    const WM_SYSKEYUP: u32 = 0x0105;
    const VK_LSHIFT: u32 = 0xA0;
    const VK_LCONTROL: u32 = 0xA2;
    const VK_LMENU: u32 = 0xA4;

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[repr(C)]
    struct Msg {
        hwnd: isize,
        message: u32,
        w_param: usize,
        l_param: isize,
        time: u32,
        point: Point,
        private: u32,
    }
    #[repr(C)]
    struct KeyboardData {
        vk_code: u32,
        scan_code: u32,
        flags: u32,
        time: u32,
        extra_info: usize,
    }

    type HookProc = unsafe extern "system" fn(i32, usize, isize) -> isize;
    static SENDER: OnceLock<Mutex<Sender<(u32, bool)>>> = OnceLock::new();

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetWindowsHookExW(
            id: i32,
            callback: Option<HookProc>,
            module: isize,
            thread_id: u32,
        ) -> isize;
        fn CallNextHookEx(hook: isize, code: i32, w_param: usize, l_param: isize) -> isize;
        fn UnhookWindowsHookEx(hook: isize) -> i32;
        fn GetMessageW(message: *mut Msg, window: isize, min: u32, max: u32) -> i32;
        fn TranslateMessage(message: *const Msg) -> i32;
        fn DispatchMessageW(message: *const Msg) -> isize;
        fn SetTimer(window: isize, id: usize, milliseconds: u32, callback: *const c_void) -> usize;
        fn KillTimer(window: isize, id: usize) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleW(module_name: *const u16) -> isize;
        fn GetLastError() -> u32;
    }

    unsafe extern "system" fn keyboard_hook(code: i32, w_param: usize, l_param: isize) -> isize {
        if code >= 0 && l_param != 0 {
            // SAFETY: Windows supplies a valid KBDLLHOOKSTRUCT for WH_KEYBOARD_LL callbacks.
            let event = unsafe { &*(l_param as *const KeyboardData) };
            let message = w_param as u32;
            let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
            let up = message == WM_KEYUP || message == WM_SYSKEYUP;
            if (down || up)
                && let Some(sender) = SENDER.get()
                && let Ok(sender) = sender.lock()
            {
                let _ = sender.send((event.vk_code, down));
            }
        }
        // SAFETY: forwarding the original callback parameters is required by Win32.
        unsafe { CallNextHookEx(0, code, w_param, l_param) }
    }

    pub fn start(app: AppHandle) {
        let _ = thread::Builder::new()
            .name("onyx-modifier-listener".into())
            .spawn(move || {
                let (sender, receiver) = mpsc::channel();
                let _ = SENDER.set(Mutex::new(sender));
                // SAFETY: the callback has static lifetime and this thread pumps messages until exit.
                let module = unsafe { GetModuleHandleW(std::ptr::null()) };
                let hook =
                    unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), module, 0) };
                if hook == 0 {
                    let code = unsafe { GetLastError() };
                    eprintln!(
                        "Onyx: impossibile avviare il listener Ctrl+Shift/Ctrl+Alt (Win32 {code})."
                    );
                    return;
                }
                // A timer keeps the message queue moving so the 180 ms hold threshold is deterministic.
                let timer = unsafe { SetTimer(0, 1, 16, std::ptr::null()) };
                let mut machine = HoldMachine::default();
                let mut message = Msg {
                    hwnd: 0,
                    message: 0,
                    w_param: 0,
                    l_param: 0,
                    time: 0,
                    point: Point { x: 0, y: 0 },
                    private: 0,
                };
                while unsafe { GetMessageW(&mut message, 0, 0, 0) } > 0 {
                    while let Ok((key, down)) = receiver.try_recv() {
                        match key {
                            VK_LCONTROL => machine.modifier(ModifierKey::Control, down, &app),
                            VK_LSHIFT => machine.modifier(ModifierKey::Shift, down, &app),
                            VK_LMENU => machine.modifier(ModifierKey::Alt, down, &app),
                            _ if down => machine.non_modifier_down(&app),
                            _ => {}
                        }
                    }
                    machine.tick(&app);
                    unsafe {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                if timer != 0 {
                    unsafe {
                        KillTimer(0, timer);
                    }
                }
                unsafe {
                    UnhookWindowsHookEx(hook);
                }
            });
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::thread;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceKeyState(state_id: i32, key: u16) -> bool;
        fn CGPreflightListenEventAccess() -> bool;
        fn CGRequestListenEventAccess() -> bool;
        fn CGPreflightPostEventAccess() -> bool;
        fn CGRequestPostEventAccess() -> bool;
    }

    pub fn start(app: AppHandle) {
        let _ = thread::Builder::new()
            .name("onyx-modifier-listener".into())
            .spawn(move || {
                let mut machine = HoldMachine::default();
                loop {
                    // Treat either side of each modifier as the same shortcut.
                    let shift =
                        unsafe { CGEventSourceKeyState(0, 56) || CGEventSourceKeyState(0, 60) };
                    let control =
                        unsafe { CGEventSourceKeyState(0, 59) || CGEventSourceKeyState(0, 62) };
                    let option =
                        unsafe { CGEventSourceKeyState(0, 58) || CGEventSourceKeyState(0, 61) };
                    machine.modifier(ModifierKey::Shift, shift, &app);
                    machine.modifier(ModifierKey::Control, control, &app);
                    machine.modifier(ModifierKey::Alt, option, &app);
                    if control && (shift || option) && other_key_is_down() {
                        machine.non_modifier_down(&app);
                    }
                    machine.tick(&app);
                    thread::sleep(Duration::from_millis(12));
                }
            });
    }

    pub(super) fn permission_status() -> NativeVoicePermissions {
        NativeVoicePermissions {
            input_monitoring: unsafe { CGPreflightListenEventAccess() },
            accessibility: unsafe { CGPreflightPostEventAccess() },
        }
    }

    pub(super) fn request_permissions() -> NativeVoicePermissions {
        let current = permission_status();
        unsafe {
            if !current.input_monitoring {
                let _ = CGRequestListenEventAccess();
            }
            if !current.accessibility {
                let _ = CGRequestPostEventAccess();
            }
        }
        permission_status()
    }

    fn other_key_is_down() -> bool {
        const MODIFIERS: [u16; 9] = [54, 55, 56, 57, 58, 59, 60, 61, 62];
        (0_u16..=127)
            .any(|key| !MODIFIERS.contains(&key) && unsafe { CGEventSourceKeyState(0, key) })
    }
}

#[cfg(all(test, any(target_os = "windows", target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn activation_delay_is_not_a_toggle() {
        assert_eq!(ACTIVATION_DELAY, Duration::from_millis(180));
        assert_ne!(Mode::Dictation, Mode::Agent);
    }
}
