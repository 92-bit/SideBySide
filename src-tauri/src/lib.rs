// SideBySide by 92-bit
// This file starts the app and connects the screen to the comparing engine (engine.rs).

pub mod engine;
pub mod fold;
pub mod tidy;
mod install;
mod update;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use engine::{Comparison, Control, Hit, Options, Row, Summary};
use fold::{At, Folds};
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
    /// "Hide unchanged" is switched on.
    fold_on: AtomicBool,
    /// The folded runs of the comparison on screen (see fold.rs). The screen's row numbers
    /// count each fold as one row; the commands below turn them into the engine's rows.
    folds: Mutex<Folds>,
}

fn folds(state: &AppState) -> std::sync::MutexGuard<'_, Folds> {
    state.folds.lock().unwrap_or_else(|e| e.into_inner())
}

/// Folds for a comparison, if "Hide unchanged" is on.
fn make_folds(state: &AppState, c: &Comparison) -> Folds {
    if state.fold_on.load(Ordering::SeqCst) { Folds::new(&c.same_runs(), c.total_rows()) } else { Folds::none(c.total_rows()) }
}

/// A place found by Go to line or Find. If it was inside a fold, that fold was opened:
/// `opened` is (the fold's screen row, how many rows the screen gained).
#[derive(serde::Serialize)]
struct Spot {
    row: u64,
    total: u64,
    opened: Option<(u64, u64)>,
}

/// The screen row of engine row `r`, opening the fold that hides it, if any.
fn spot(state: &AppState, r: u64) -> Spot {
    let Ok(mut f) = state.folds.lock() else { return Spot { row: r, total: 0, opened: None } };
    let mut opened = None;
    if let (_, Some(i)) = f.screen(r) {
        opened = Some(f.open(i));
    }
    Spot { row: f.screen(r).0, total: f.total(), opened }
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
    let mut summary = result.summary();
    // A search in the old result is no longer needed.
    replace_search(&state, Arc::default());
    let f = make_folds(&state, &result);
    summary.total_rows = f.total();
    *state.folds.lock().map_err(|_| "busy")? = f;
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
    let f = folds(&state);
    if f.is_empty() {
        return c.rows(start, count.min(500));
    }
    // Screen rows: runs of real rows, with a fold row in place of each folded run.
    let end = start.saturating_add(count.min(500) as u64).min(f.total());
    let mut out = Vec::new();
    let mut s = start;
    while s < end {
        match f.at(s) {
            At::Fold(i) => {
                out.push(Row { k: 4, l: None, r: None, f: Some(f.len(i)), m: None });
                s += 1;
            }
            At::Real(r) => {
                let n = (f.next_fold_after(s).min(end) - s) as u32;
                out.extend(c.rows(r, n)?);
                s += n as u64;
            }
        }
    }
    Ok(out)
}

#[tauri::command]
fn nav(state: tauri::State<'_, AppState>, row: u64, dir: i32) -> Option<Hit> {
    let f = folds(&state);
    let hit = current(&state)?.nav(f.real(row), dir)?;
    // Differences are never folded, so a difference's rows stay together on screen.
    Some(Hit { row: f.screen(hit.row).0, ..hit })
}

#[tauri::command]
fn overview(state: tauri::State<'_, AppState>, buckets: u32) -> Vec<u8> {
    let f = folds(&state);
    current(&state).map(|c| c.overview_mapped(buckets, f.total(), &|r| f.screen(r).0)).unwrap_or_default()
}

/// Screen row of a line number (1-based) in the original (left) or new (right) file.
#[tauri::command]
fn row_for_line(state: tauri::State<'_, AppState>, left: bool, line: u32) -> Option<Spot> {
    let r = current(&state)?.row_for_line(left, line.checked_sub(1)?)?;
    Some(spot(&state, r))
}

/// "Hide unchanged" on or off. Returns how many rows the screen has now.
#[tauri::command]
fn set_fold(state: tauri::State<'_, AppState>, on: bool) -> u64 {
    state.fold_on.store(on, Ordering::SeqCst);
    let Some(c) = current(&state) else { return 0 };
    let f = make_folds(&state, &c);
    let total = f.total();
    if let Ok(mut s) = state.folds.lock() {
        *s = f;
    }
    total
}

/// Open the fold shown at screen row `row`. Returns (rows gained, rows on the screen now).
#[tauri::command]
fn open_fold(state: tauri::State<'_, AppState>, row: u64) -> Option<(u64, u64)> {
    let mut f = state.folds.lock().ok()?;
    let At::Fold(i) = f.at(row) else { return None };
    let (_, added) = f.open(i);
    Some((added, f.total()))
}

#[tauri::command]
async fn find(
    state: tauri::State<'_, AppState>,
    query: String,
    from: Option<u64>,
    forward: bool,
    match_case: bool,
) -> Result<Option<Spot>, String> {
    let c = current(&state).ok_or("Nothing to search")?;
    let from = from.map(|s| folds(&state).real(s));
    // (the lock above is let go at the end of that line, before the search runs)
    // Each search gets its own fresh stop switch (a new search stops the one before).
    let ctl = Arc::new(Control::default());
    replace_search(&state, ctl.clone());
    let found = tauri::async_runtime::spawn_blocking(move || c.find(&query, from, forward, match_case, &ctl))
        .await
        .map_err(|e| e.to_string())??;
    Ok(found.map(|r| spot(&state, r)))
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
    if let Ok(mut f) = state.folds.lock() {
        *f = Folds::default();
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
            set_fold,
            open_fold,
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
