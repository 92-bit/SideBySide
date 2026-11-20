//! Installing the portable app for the current user (Windows only), and removing it again.
//!
//! Installing needs no administrator rights. It:
//! - copies SideBySide.exe to  %LOCALAPPDATA%\Programs\SideBySide\
//! - adds a Start Menu shortcut (and a desktop one, if asked)
//! - adds "Compare with SideBySide" to the right-click menu for files (if asked)
//! - adds an entry to Windows Settings → Apps, so it can be uninstalled like any app

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallInfo {
    /// Installing is possible on this system (Windows).
    pub supported: bool,
    /// This copy is the installed one.
    pub running_installed: bool,
    /// An installed copy exists.
    pub installed: bool,
    pub install_path: String,
}

#[cfg(windows)]
pub use win::{info, install, open_installed_later, refresh_registered_version, uninstall};

/// Start `exe` once this program has closed. It's started right away, but told to wait
/// for this program to exit first (see `wait_for_exit`). Starting it without waiting would
/// just hand over to this window, because only one SideBySide window is allowed.
pub fn launch_later(exe: &std::path::Path) -> Result<(), String> {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg(format!("{WAIT_FOR_ARG}{}", std::process::id()));
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("Could not restart SideBySide: {e}"))
}

/// "--wait-for=<process id>": wait until that process has closed before starting.
pub const WAIT_FOR_ARG: &str = "--wait-for=";

/// Is the process `pid` still running? (When it can't be told, the answer is yes.)
pub fn process_running(pid: u32) -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code) != 0;
        CloseHandle(h);
        !ok || code == STILL_ACTIVE as u32
    }
    #[cfg(not(windows))]
    {
        !std::path::Path::new("/proc/self").exists() || std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
}

/// Wait (up to 30 seconds) until the process `pid` has closed.
pub fn wait_for_exit(pid: u32) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
        let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if !h.is_null() {
            WaitForSingleObject(h, 30_000);
            CloseHandle(h);
        }
    }
    #[cfg(not(windows))]
    {
        let proc_dir = std::path::PathBuf::from(format!("/proc/{pid}"));
        if !std::path::Path::new("/proc/self").exists() {
            std::thread::sleep(std::time::Duration::from_secs(1)); // no /proc (Mac): just wait a moment
            return;
        }
        for _ in 0..300 {
            if !proc_dir.exists() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

#[cfg(windows)]
mod win {
    use super::InstallInfo;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\SideBySide";
    const MENU_KEY: &str = r"Software\Classes\*\shell\SideBySide";
    /// Where the window's saved settings live (named after the app identifier).
    const DATA_FOLDER: &str = "com.bit92.sidebyside";

    fn env_dir(var: &str) -> Result<PathBuf, String> {
        std::env::var_os(var).map(PathBuf::from).ok_or_else(|| format!("{var} is not set"))
    }
    fn install_dir() -> Result<PathBuf, String> {
        Ok(env_dir("LOCALAPPDATA")?.join("Programs").join("SideBySide"))
    }
    fn installed_exe() -> Result<PathBuf, String> {
        Ok(install_dir()?.join("SideBySide.exe"))
    }
    fn start_menu_link() -> Result<PathBuf, String> {
        Ok(env_dir("APPDATA")?.join(r"Microsoft\Windows\Start Menu\Programs\SideBySide.lnk"))
    }
    fn same_file(a: &Path, b: &Path) -> bool {
        matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
    }
    fn err<E: std::fmt::Display>(what: &str) -> impl Fn(E) -> String + '_ {
        move |e| format!("{what}: {e}")
    }

    pub fn info() -> InstallInfo {
        let target = installed_exe().ok();
        let running_installed = match (std::env::current_exe().ok(), &target) {
            (Some(exe), Some(t)) => same_file(&exe, t),
            _ => false,
        };
        InstallInfo {
            supported: true,
            running_installed,
            installed: target.as_ref().is_some_and(|t| t.exists()),
            install_path: target.map(|t| t.display().to_string()).unwrap_or_default(),
        }
    }

    fn make_shortcut(target: &Path, link: &Path) -> Result<(), String> {
        if let Some(parent) = link.parent() {
            std::fs::create_dir_all(parent).map_err(err("Could not create the shortcut folder"))?;
        }
        let mut sl = mslnk::ShellLink::new(target).map_err(err("Could not make a shortcut"))?;
        sl.set_name(Some("Compare text files side by side".into()));
        sl.set_working_dir(target.parent().map(|p| p.display().to_string()));
        sl.set_icon_location(Some(target.display().to_string()));
        sl.create_lnk(link).map_err(err("Could not save the shortcut"))
    }

    pub fn install(version: &str, context_menu: bool, desktop_link: Option<PathBuf>) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(err("Could not find SideBySide"))?;
        let dir = install_dir()?;
        let target = installed_exe()?;
        std::fs::create_dir_all(&dir).map_err(err("Could not create the install folder"))?;
        if !same_file(&exe, &target) {
            std::fs::copy(&exe, &target).map_err(|e| {
                format!("Could not copy SideBySide ({e}). If the installed SideBySide is open, close it and try again.")
            })?;
        }

        make_shortcut(&target, &start_menu_link()?)?;
        if let Some(link) = &desktop_link {
            make_shortcut(&target, link)?;
        }

        let t = target.display().to_string();
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let reg = err::<std::io::Error>("Could not update Windows settings");

        // Entry in Windows Settings → Apps
        let (k, _) = hkcu.create_subkey(UNINSTALL_KEY).map_err(&reg)?;
        let size_kb = std::fs::metadata(&target).map(|m| (m.len() / 1024) as u32).unwrap_or(0);
        k.set_value("DisplayName", &"SideBySide").map_err(&reg)?;
        k.set_value("DisplayVersion", &version).map_err(&reg)?;
        k.set_value("Publisher", &"92-bit").map_err(&reg)?;
        k.set_value("DisplayIcon", &t).map_err(&reg)?;
        k.set_value("InstallLocation", &dir.display().to_string()).map_err(&reg)?;
        k.set_value("UninstallString", &format!("\"{t}\" --uninstall")).map_err(&reg)?;
        k.set_value("QuietUninstallString", &format!("\"{t}\" --uninstall")).map_err(&reg)?;
        k.set_value("URLInfoAbout", &"https://github.com/92-bit/SideBySide").map_err(&reg)?;
        k.set_value("NoModify", &1u32).map_err(&reg)?;
        k.set_value("NoRepair", &1u32).map_err(&reg)?;
        k.set_value("EstimatedSize", &size_kb).map_err(&reg)?;
        match &desktop_link {
            Some(l) => k.set_value("DesktopShortcut", &l.display().to_string()).map_err(&reg)?,
            None => {
                let _ = k.delete_value("DesktopShortcut");
            }
        }

        // Right-click → "Compare with SideBySide"
        if context_menu {
            let (m, _) = hkcu.create_subkey(MENU_KEY).map_err(&reg)?;
            m.set_value("", &"Compare with SideBySide").map_err(&reg)?;
            m.set_value("Icon", &format!("\"{t}\",0")).map_err(&reg)?;
            let (c, _) = m.create_subkey("command").map_err(&reg)?;
            c.set_value("", &format!("\"{t}\" \"%1\"")).map_err(&reg)?;
        } else {
            let _ = hkcu.delete_subkey_all(MENU_KEY);
        }
        Ok(())
    }

    /// Start the installed copy a moment after this one has closed.
    pub fn open_installed_later() -> Result<(), String> {
        super::launch_later(&installed_exe()?)
    }

    /// After an update, show the new version number in Settings → Apps.
    pub fn refresh_registered_version(version: &str) {
        if !info().running_installed {
            return;
        }
        if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(UNINSTALL_KEY, winreg::enums::KEY_SET_VALUE) {
            let _ = k.set_value("DisplayVersion", &version);
        }
    }

    /// Is another SideBySide running from the install folder (not this uninstaller)?
    fn other_copy_running() -> bool {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
        };
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
        };
        let Ok(target) = installed_exe() else { return false };
        let me = std::process::id();
        let mut found = false;
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return false;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(snap, &mut entry) != 0;
            while ok && !found {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                if entry.th32ProcessID != me && name.eq_ignore_ascii_case("SideBySide.exe") {
                    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID);
                    if !h.is_null() {
                        let mut buf = [0u16; 1024];
                        let mut size = buf.len() as u32;
                        if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut size) != 0 {
                            let path = PathBuf::from(String::from_utf16_lossy(&buf[..size as usize]));
                            found = same_file(&path, &target);
                        }
                        CloseHandle(h);
                    }
                }
                ok = Process32NextW(snap, &mut entry) != 0;
            }
            CloseHandle(snap);
        }
        found
    }

    /// Show a Retry / Cancel message. Returns true for Retry.
    fn ask_retry(text: &str) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            IDRETRY, MB_ICONWARNING, MB_RETRYCANCEL, MB_SETFOREGROUND, MessageBoxW,
        };
        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let (text, title) = (wide(text), wide("Uninstall SideBySide"));
        unsafe {
            MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_RETRYCANCEL | MB_ICONWARNING | MB_SETFOREGROUND)
                == IDRETRY
        }
    }

    /// Remove everything the installer added. Runs without opening a window
    /// (from Windows Settings → Apps → Uninstall).
    pub fn uninstall() {
        // An open SideBySide window would keep its program file locked, so the folder
        // couldn't be removed. Ask the user to close it first.
        while other_copy_running() {
            if !ask_retry("SideBySide is still open.

Please close it, then click Retry.") {
                return; // Cancel: change nothing
            }
        }
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let desktop: Option<String> = hkcu.open_subkey(UNINSTALL_KEY).ok().and_then(|k| k.get_value("DesktopShortcut").ok());
        let _ = hkcu.delete_subkey_all(MENU_KEY);
        let _ = hkcu.delete_subkey_all(UNINSTALL_KEY);
        if let Ok(link) = start_menu_link() {
            let _ = std::fs::remove_file(link);
        }
        if let Some(link) = desktop {
            if link.to_ascii_lowercase().ends_with("sidebyside.lnk") {
                let _ = std::fs::remove_file(link);
            }
        }
        // A running program can't delete itself, so a tiny helper removes the folders after
        // SideBySide has closed. It keeps trying for up to 20 seconds, because Windows'
        // web viewer can take a few seconds to let go of the settings folder.
        let mut folders = Vec::new();
        if let Ok(dir) = install_dir() {
            folders.push(dir);
        }
        if let Ok(local) = env_dir("LOCALAPPDATA") {
            folders.push(local.join(DATA_FOLDER));
        }
        // The folder paths are handed over as environment variables, so a "%" or "&" in a
        // folder name can't be mistaken for part of the command.
        let mut cmd = Command::new("cmd");
        let mut steps = Vec::new();
        for (i, d) in folders.iter().enumerate() {
            let var = format!("SIDEBYSIDE_REMOVE_{i}");
            cmd.env(&var, d);
            steps.push(format!(
                "for /l %i in (1,1,10) do @(if exist \"%{var}%\" (ping -n 3 127.0.0.1 >nul & rmdir /s /q \"%{var}%\"))"
            ));
        }
        // Run the helper from the Temp folder: a program can't delete the folder it runs in,
        // which matters when the uninstall was started from inside the install folder.
        let _ = cmd
            .raw_arg(format!("/C {}", steps.join(" & ")))
            .current_dir(std::env::temp_dir())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}

// On Mac and Linux, the app is simply run from where it is (installing comes later).
#[cfg(not(windows))]
pub fn info() -> InstallInfo {
    InstallInfo { supported: false, running_installed: false, installed: false, install_path: String::new() }
}
#[cfg(not(windows))]
pub fn install(_version: &str, _context_menu: bool, _desktop_link: Option<std::path::PathBuf>) -> Result<(), String> {
    Err("Installing is only available on Windows for now.".into())
}
#[cfg(not(windows))]
pub fn open_installed_later() -> Result<(), String> {
    Err("Installing is only available on Windows for now.".into())
}
#[cfg(not(windows))]
pub fn uninstall() {}
#[cfg(not(windows))]
pub fn refresh_registered_version(_version: &str) {}
