use crate::models::ActiveAppContext;

pub fn current() -> ActiveAppContext {
    let process = platform_process_name().unwrap_or_else(|| "active-app".into());
    app_context(&process)
}

fn app_context(process: &str) -> ActiveAppContext {
    let normalized = process.to_ascii_lowercase();
    let (name, accent, symbol) = if normalized.contains("chrome") {
        ("Google Chrome", "#4d8df7", "C")
    } else if normalized.contains("msedge") || normalized == "edge" {
        ("Microsoft Edge", "#18b6a4", "E")
    } else if normalized.contains("firefox") {
        ("Firefox", "#ff7139", "F")
    } else if normalized.contains("code") {
        ("Visual Studio Code", "#24a7e8", "⌁")
    } else if normalized.contains("windowsterminal") || normalized.contains("terminal") {
        ("Terminale", "#8b7cf6", ">_")
    } else if normalized.contains("powershell") {
        ("PowerShell", "#4f7bd9", ">_")
    } else if normalized.contains("notepad") {
        ("Blocco note", "#56a7da", "N")
    } else if normalized.contains("word") {
        ("Microsoft Word", "#2b579a", "W")
    } else if normalized.contains("outlook") {
        ("Microsoft Outlook", "#0a64ad", "O")
    } else if normalized.contains("slack") {
        ("Slack", "#6f4a7e", "S")
    } else if normalized.contains("discord") {
        ("Discord", "#5865f2", "D")
    } else if normalized.contains("finder") {
        ("Finder", "#3b9df5", "F")
    } else if normalized.contains("safari") {
        ("Safari", "#1f9bf0", "S")
    } else {
        ("App attiva", "#8f9bab", "•")
    };
    ActiveAppContext {
        name: name.into(),
        process: process.into(),
        accent: accent.into(),
        symbol: symbol.into(),
    }
}

#[cfg(target_os = "windows")]
fn platform_process_name() -> Option<String> {
    use std::path::Path;

    type Hwnd = isize;
    type Handle = isize;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetForegroundWindow() -> Hwnd;
        fn GetWindowThreadProcessId(window: Hwnd, process_id: *mut u32) -> u32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> Handle;
        fn QueryFullProcessImageNameW(
            process: Handle,
            flags: u32,
            path: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }

    // SAFETY: handles are checked before use and the path buffer length is passed to Win32.
    unsafe {
        let window = GetForegroundWindow();
        if window == 0 {
            return None;
        }
        let mut process_id = 0;
        GetWindowThreadProcessId(window, &mut process_id);
        if process_id == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
        if handle == 0 {
            return None;
        }
        let mut buffer = vec![0_u16; 2048];
        let mut size = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size);
        CloseHandle(handle);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buffer[..size as usize]);
        Path::new(&path)
            .file_stem()
            .and_then(|value| value.to_str())
            .map(str::to_owned)
    }
}

#[cfg(target_os = "macos")]
fn platform_process_name() -> Option<String> {
    // The visual badge remains useful on macOS even when app metadata permission is unavailable.
    Some("active-app".into())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_process_name() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_apps_have_stable_accents() {
        let chrome = app_context("chrome.exe");
        assert_eq!(chrome.name, "Google Chrome");
        assert_eq!(chrome.accent, "#4d8df7");
    }
}
