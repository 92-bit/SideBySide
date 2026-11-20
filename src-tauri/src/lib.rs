// SideBySide by 92-bit
// This file starts the app and connects the screen to the comparing engine (engine.rs).

pub mod engine;
pub mod tidy;
mod install;
mod update;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use engine::{Comparison, Control, Hit, Options, Row, Summary};
use tauri::{Emitter, Manager};

#[derive(Default)]
struct AppState {
    current: Mutex<Option<Arc<Comparison>>>,
    /// The progress bar and Cancel switch of the comparison that is running (or ran last).
    control: Mutex<Arc<Control>>,
    /// The stop switch of the search that is running (or ran last). It is separate from the
    /// comparison's switch, so a cancelled comparison can't break searching in the one on screen.
    search: Mutex<Arc<Control>>,
    /// Goes up on every new comparison, so an older, slower one can't overwrite a newer one.
    generation: AtomicU64,
    /// Files handed to the app (when it starts, or later from the right-click menu)
    /// that the screen hasn't picked up yet.
    pending_files: Mutex<Vec<String>>,
}

/// File names from a command line, made into full paths.
fn files_from_args(args: impl IntoIterator<Item = String>, cwd: &Path) -> Vec<String> {
    args.into_iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .map(|a| cwd.join(a).to_string_lossy().into_owned())
        .collect()
}

fn current(state: &AppState) -> Option<Arc<Comparison>> {
    state.current.lock().ok()?.clone()
}

fn control(state: &AppState) -> Arc<Control> {
    state.control.lock().map(|c| c.clone()).unwrap_or_default()
}

/// Stop the search that is running, if any, and put `next` in its place.
fn replace_search(state: &AppState, next: Arc<Control>) {
    if let Ok(mut s) = state.search.lock() {
        s.cancel.store(true, Ordering::SeqCst);
        *s = next;
    }
}

#[tauri::command]
async fn compare(
    state: tauri::State<'_, AppState>,
    left: String,
    right: String,
    options: Options,
) -> Result<Summary, String> {
    let my_turn = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let ctl = Arc::new(Control::default());
    if let Ok(mut c) = state.control.lock() {
        // A newer comparison replaces an older one that is still running.
        c.cancel.store(true, Ordering::SeqCst);
        *c = ctl.clone();
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        engine::compare(&PathBuf::from(left), &PathBuf::from(right), &options, &ctl)
    })
    .await
    .map_err(|e| e.to_string())??;
    if state.generation.load(Ordering::SeqCst) != my_turn {
        return Err("__stale__".into());
    }
    let summary = result.summary();
    // A search in the old result is no longer needed.
    replace_search(&state, Arc::default());
    *state.current.lock().map_err(|_| "busy")? = Some(Arc::new(result));
    Ok(summary)
}

/// How far the running comparison is, from 0 to 1000.
#[tauri::command]
fn progress(state: tauri::State<'_, AppState>) -> u32 {
    control(&state).progress.load(Ordering::Relaxed)
}

#[tauri::command]
fn cancel(state: tauri::State<'_, AppState>) {
    control(&state).cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
async fn rows(state: tauri::State<'_, AppState>, start: u64, count: u32) -> Result<Vec<Row>, String> {
    let c = current(&state).ok_or("Nothing to show")?;
    c.rows(start, count.min(500))
}

#[tauri::command]
fn nav(state: tauri::State<'_, AppState>, row: u64, dir: i32) -> Option<Hit> {
    current(&state)?.nav(row, dir)
}

#[tauri::command]
fn overview(state: tauri::State<'_, AppState>, buckets: u32) -> Vec<u8> {
    current(&state).map(|c| c.overview(buckets)).unwrap_or_default()
}

/// Screen row of a line number (1-based) in the original (left) or new (right) file.
#[tauri::command]
fn row_for_line(state: tauri::State<'_, AppState>, left: bool, line: u32) -> Option<u64> {
    current(&state)?.row_for_line(left, line.checked_sub(1)?)
}

#[tauri::command]
async fn find(
    state: tauri::State<'_, AppState>,
    query: String,
    from: Option<u64>,
    forward: bool,
    match_case: bool,
) -> Result<Option<u64>, String> {
    let c = current(&state).ok_or("Nothing to search")?;
    // Each search gets its own fresh stop switch (a new search stops the one before).
    let ctl = Arc::new(Control::default());
    replace_search(&state, ctl.clone());
    tauri::async_runtime::spawn_blocking(move || c.find(&query, from, forward, match_case, &ctl))
        .await
        .map_err(|e| e.to_string())?
}

/// Live reload: has a compared file changed since? False when nothing is compared.
#[tauri::command]
fn files_changed(state: tauri::State<'_, AppState>) -> bool {
    current(&state).is_some_and(|c| c.changed_on_disk())
}

#[tauri::command]
fn close_compare(state: tauri::State<'_, AppState>) {
    state.generation.fetch_add(1, Ordering::SeqCst);
    control(&state).cancel.store(true, Ordering::SeqCst);
    if let Ok(mut c) = state.current.lock() {
        *c = None;
    }
}

/// Save pasted text to two files in the temporary folder, so they can be compared like any file.
#[tauri::command]
fn save_pasted(original: String, changed: String) -> Result<(String, String), String> {
    let dir = std::env::temp_dir().join("SideBySide");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let a = dir.join("Pasted text (original).txt");
    let b = dir.join("Pasted text (new).txt");
    std::fs::write(&a, original).map_err(|e| e.to_string())?;
    std::fs::write(&b, changed).map_err(|e| e.to_string())?;
    Ok((a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned()))
}

/// Files given to the app (e.g. `sidebyside.exe old.txt new.txt`, or from the right-click menu)
/// that the screen hasn't picked up yet.
#[tauri::command]
fn take_files(state: tauri::State<'_, AppState>) -> Vec<String> {
    state.pending_files.lock().map(|mut f| std::mem::take(&mut *f)).unwrap_or_default()
}

#[tauri::command]
fn install_info() -> install::InstallInfo {
    install::info()
}

#[tauri::command]
fn install_app(app: tauri::AppHandle, context_menu: bool, desktop_shortcut: bool) -> Result<(), String> {
    let desktop = if desktop_shortcut {
        Some(app.path().desktop_dir().map_err(|e| e.to_string())?.join("SideBySide.lnk"))
    } else {
        None
    };
    install::install(&app.package_info().version.to_string(), context_menu, desktop)
}

/// Close this (portable) copy and open the installed one.
#[tauri::command]
fn open_installed(app: tauri::AppHandle) -> Result<(), String> {
    install::open_installed_later()?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
async fn check_update(app: tauri::AppHandle) -> Result<update::UpdateInfo, String> {
    let current = app.package_info().version.to_string();
    tauri::async_runtime::spawn_blocking(move || update::check(&current))
        .await
        .map_err(|e| e.to_string())?
}

/// Download and install the newest version, then restart into it.
#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let current = app.package_info().version.to_string();
    let exe = tauri::async_runtime::spawn_blocking(move || update::download_and_replace(&current))
        .await
        .map_err(|e| e.to_string())??;
    install::launch_later(&exe)?;
    app.exit(0);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // "SideBySide.exe --uninstall" comes from Windows Settings → Apps → Uninstall.
    // (args_os: a file name that is not valid Unicode, possible on Linux, must not crash the app)
    let args: Vec<String> = std::env::args_os().map(|a| a.to_string_lossy().into_owned()).collect();
    if args.iter().any(|a| a == "--uninstall") {
        install::uninstall();
        return;
    }

    // Started by an update or by "open the installed copy": wait for the old window to close
    // first, or the single-window rule would hand this start over to it.
    if let Some(pid) = args.iter().find_map(|a| a.strip_prefix(install::WAIT_FOR_ARG)).and_then(|p| p.parse().ok()) {
        install::wait_for_exit(pid);
    }

    update::clean_up_old();
    engine::remove_left_over_copies(install::process_running);
    install::refresh_registered_version(env!("CARGO_PKG_VERSION"));

    let cwd = std::env::current_dir().unwrap_or_default();
    let state = AppState {
        pending_files: Mutex::new(files_from_args(args, &cwd)),
        ..Default::default()
    };

    tauri::Builder::default()
        // Only one SideBySide window: opening it again (or right-clicking a file)
        // hands the files to the window that is already open.
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
            let files = files_from_args(args, Path::new(&cwd));
            if !files.is_empty() {
                if let Ok(mut pending) = app.state::<AppState>().pending_files.lock() {
                    pending.extend(files);
                }
                let _ = app.emit("files-arrived", ());
            }
        }))
        .manage(state)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            compare,
            progress,
            cancel,
            rows,
            nav,
            overview,
            files_changed,
            row_for_line,
            find,
            close_compare,
            save_pasted,
            take_files,
            install_info,
            install_app,
            open_installed,
            check_update,
            install_update
        ])
        .build(tauri::generate_context!())
        .expect("error while running SideBySide")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // Let go of the comparison, so converted copies in the Temp folder are removed.
                if let Ok(mut c) = app.state::<AppState>().current.lock() {
                    *c = None;
                }
            }
        });
}
