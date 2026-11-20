// SideBySide by 92-bit
// This file starts the app and connects the screen to the comparing engine (engine.rs).

mod cli;
pub mod engine;
pub mod edit;
pub mod export;
pub mod fold;
pub mod folders;
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
    /// Files being edited (their working copies) and the undo / redo history, see edit.rs.
    work: Mutex<Work>,
    /// The last report saved, so "Open it" can open exactly that file (and nothing else).
    saved_report: Mutex<Option<PathBuf>>,
    /// The folded runs of the comparison on screen (see fold.rs). The screen's row numbers
    /// count each fold as one row; the commands below turn them into the engine's rows.
    folds: Mutex<Folds>,
}

/// Editing (see edit.rs): a working copy per side, and one history for both sides
/// (true = the left file).
#[derive(Default)]
struct Work {
    left: Option<edit::Working>,
    right: Option<edit::Working>,
    undo: Vec<(bool, edit::Change)>,
    redo: Vec<(bool, edit::Change)>,
}

impl Work {
    fn side(&mut self, left: bool) -> &mut Option<edit::Working> {
        if left { &mut self.left } else { &mut self.right }
    }
    /// Forget a side's working copy (its changes are dropped) and its history.
    fn drop_side(&mut self, left: bool) {
        *self.side(left) = None;
        self.undo.retain(|(l, _)| *l != left);
        self.redo.retain(|(l, _)| *l != left);
    }
    /// Keep the working copies of the files being compared now (also after Swap), drop the rest.
    fn keep_for(&mut self, left: &Path, right: &Path) {
        let swapped = self.left.as_ref().is_some_and(|w| w.real == right) || self.right.as_ref().is_some_and(|w| w.real == left);
        if swapped {
            std::mem::swap(&mut self.left, &mut self.right);
            for (l, _) in self.undo.iter_mut().chain(self.redo.iter_mut()) {
                *l = !*l;
            }
        }
        if self.left.as_ref().is_some_and(|w| w.real != left) {
            self.drop_side(true);
        }
        if self.right.as_ref().is_some_and(|w| w.real != right) {
            self.drop_side(false);
        }
    }
    fn state(&self) -> EditState {
        EditState {
            left: self.left.is_some(),
            right: self.right.is_some(),
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
        }
    }
}

/// Which files have unsaved changes, and whether Undo / Redo can do anything.
#[derive(serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct EditState {
    left: bool,
    right: bool,
    can_undo: bool,
    can_redo: bool,
}

fn work(state: &AppState) -> std::sync::MutexGuard<'_, Work> {
    state.work.lock().unwrap_or_else(|e| e.into_inner())
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
    // A file being edited is compared through its working copy.
    let (lreal, rreal) = (PathBuf::from(&left), PathBuf::from(&right));
    let (lsrc, rsrc) = {
        let mut w = work(&state);
        w.keep_for(&lreal, &rreal);
        let src = |o: &Option<edit::Working>, real: &PathBuf| o.as_ref().map_or(real.clone(), |w| w.copy.clone());
        (src(&w.left, &lreal), src(&w.right, &rreal))
    };
    let result = tauri::async_runtime::spawn_blocking(move || engine::compare(&lsrc, &rsrc, &options, &ctl))
        .await
        .map_err(|e| e.to_string())??;
    if state.generation.load(Ordering::SeqCst) != my_turn {
        return Err("__stale__".into());
    }
    let mut summary = result.summary();
    // (show the user's files, not the working copies)
    for (info, real) in [(&mut summary.left, &lreal), (&mut summary.right, &rreal)] {
        info.path = real.to_string_lossy().into_owned();
        info.name = real.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    }
    // A search in the old result is no longer needed.
    replace_search(&state, Arc::default());
    let f = make_folds(&state, &result);
    summary.total_rows = f.total();
    *state.folds.lock().map_err(|_| "busy")? = f;
    *state.current.lock().map_err(|_| "busy")? = Some(Arc::new(result));
    Ok(summary)
}

/// Compare two folders (see folders.rs). Uses the same progress bar and Cancel as comparing files.
#[tauri::command]
async fn compare_folders(state: tauri::State<'_, AppState>, left: String, right: String) -> Result<folders::FolderResult, String> {
    let ctl = Arc::new(Control::default());
    if let Ok(mut c) = state.control.lock() {
        c.cancel.store(true, Ordering::SeqCst);
        *c = ctl.clone();
    }
    tauri::async_runtime::spawn_blocking(move || folders::compare(Path::new(&left), Path::new(&right), &ctl))
        .await
        .map_err(|e| e.to_string())?
}

/// Is this a folder? (For dropped or picked paths.)
#[tauri::command]
fn is_folder(path: String) -> bool {
    Path::new(&path).is_dir()
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

/// Save the comparison as an HTML report (see export.rs). `when` is the date and time to print in it.
#[tauri::command]
async fn save_report(state: tauri::State<'_, AppState>, path: String, when: String) -> Result<(), String> {
    let c = current(&state).ok_or("Nothing to save")?;
    let p = PathBuf::from(&path);
    let target = p.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let html = export::report(&c, &when, 64 << 20)?;
        std::fs::write(&target, html).map_err(|e| format!("Could not save the report: {e}"))
    })
    .await
    .map_err(|e| e.to_string())??;
    if let Ok(mut s) = state.saved_report.lock() {
        *s = Some(p);
    }
    Ok(())
}

/// Open the report that was just saved, in the browser.
#[tauri::command]
fn open_report(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let p = state.saved_report.lock().ok().and_then(|s| s.clone()).ok_or("No report was saved")?;
    tauri_plugin_opener::open_path(p, None::<&str>).map_err(|e| e.to_string())
}

/// The differences as a patch (unified diff text), for the clipboard.
#[tauri::command]
async fn make_patch(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let c = current(&state).ok_or("Nothing to copy")?;
    tauri::async_runtime::spawn_blocking(move || export::patch(&c, 32 << 20))
        .await
        .map_err(|e| e.to_string())?
}

/// Using SideBySide as Git's compare tool ("git difftool").
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GitSetup {
    /// The Git commands that set it up (to show, or to run).
    commands: Vec<String>,
    /// Git was found on this computer.
    git_found: bool,
    /// What Git's compare tool is set to now ("" if nothing).
    current: String,
    /// Why Git couldn't be used, if it couldn't ("" if it could).
    problem: String,
}

fn git_run(args: &[&str]) -> std::io::Result<std::process::Output> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(args).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // no console window flashing up
    }
    cmd.output()
}
fn git(args: &[&str]) -> Option<std::process::Output> {
    git_run(args).ok()
}

/// Shows (apply = false) or runs (apply = true) the Git settings that make "git difftool" use
/// SideBySide. Only changes Git's own settings for this user (git config --global).
#[tauri::command]
fn git_setup(apply: bool) -> Result<GitSetup, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_string_lossy().replace('\\', "/");
    let tool_cmd = format!("\"{exe}\" --wait \"$LOCAL\" \"$REMOTE\"");
    let settings: [(&str, &str); 3] =
        [("difftool.sidebyside.cmd", &tool_cmd), ("diff.tool", "sidebyside"), ("difftool.prompt", "false")];
    let commands = settings.iter().map(|(k, v)| format!("git config --global {k} '{v}'")).collect();
    let (git_found, problem) = match git_run(&["--version"]) {
        Ok(o) if o.status.success() => (true, String::new()),
        Ok(o) => (false, String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => (false, e.to_string()),
    };
    if apply {
        if !git_found {
            return Err("Git isn't installed (or can't be found), so there is nothing to set up.".into());
        }
        for (k, v) in settings {
            let out = git(&["config", "--global", k, v]).ok_or("Git could not be started.")?;
            if !out.status.success() {
                return Err(format!("Git said: {}", String::from_utf8_lossy(&out.stderr).trim()));
            }
        }
    }
    let current = git(&["config", "--global", "diff.tool"])
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    Ok(GitSetup { commands, git_found, current, problem })
}

// ---- Editing and merging (see edit.rs) ----

/// The working copy of one side, made now if that file isn't being edited yet.
fn working<'a>(w: &'a mut Work, c: &Comparison, left: bool) -> Result<&'a edit::Working, String> {
    let s = c.summary();
    let info = if left { s.left } else { s.right };
    if w.side(left).is_none() {
        if !info.editable {
            return Err(
                "This file can't be changed here: it was converted, tidied, or only part of it is shown. Turn that off to edit it."
                    .into(),
            );
        }
        *w.side(left) = Some(edit::Working::start(c.source(left)).map_err(|e| format!("Could not start editing: {e}"))?);
    }
    Ok(w.side(left).as_ref().expect("made above"))
}

fn record(w: &mut Work, left: bool, change: edit::Change) -> EditState {
    w.undo.push((left, change));
    w.redo.clear();
    w.state()
}

/// Copy the difference at screen row `row` to the other side (to_left: from the right file
/// into the left one).
#[tauri::command]
fn copy_block(state: tauri::State<'_, AppState>, row: u64, to_left: bool) -> Result<EditState, String> {
    let c = current(&state).ok_or("Nothing to copy")?;
    let real_row = folds(&state).real(row);
    let (a, alen, b, blen) = c.block_at(real_row).ok_or("There is no difference there.")?;
    let ((from, count), (at, replace)) = if to_left { ((b, blen), (a, alen)) } else { ((a, alen), (b, blen)) };
    let raw = edit::read_raw(c.source(!to_left), from, count).map_err(|e| e.to_string())?;
    let mut w = work(&state);
    let target = working(&mut w, &c, to_left)?;
    let change = target.replace(at, replace, target.adapt(&raw)).map_err(|e| e.to_string())?;
    Ok(record(&mut w, to_left, change))
}

/// Change line `line` (counted from 1) of one file to `text`.
#[tauri::command]
fn edit_line(state: tauri::State<'_, AppState>, left: bool, line: u32, text: String) -> Result<EditState, String> {
    let c = current(&state).ok_or("Nothing to edit")?;
    let mut w = work(&state);
    let target = working(&mut w, &c, left)?;
    let change = target.replace(line.saturating_sub(1), 1, target.lines_of(&text)).map_err(|e| e.to_string())?;
    Ok(record(&mut w, left, change))
}

/// Undo (redo = false) or redo the last change.
#[tauri::command]
fn undo_redo(state: tauri::State<'_, AppState>, redo: bool) -> Result<EditState, String> {
    let mut w = work(&state);
    let Some((left, change)) = (if redo { w.redo.pop() } else { w.undo.pop() }) else { return Ok(w.state()) };
    let target = w.side(left).as_ref().ok_or("That file isn't being edited any more.")?;
    let back = target.apply_inverse(&change).map_err(|e| e.to_string())?;
    if redo {
        w.undo.push((left, back));
    } else {
        w.redo.push((left, back));
    }
    Ok(w.state())
}

/// Save the changes to the user's file(s): one side, or both (left = None).
#[tauri::command]
fn save_edits(state: tauri::State<'_, AppState>, left: Option<bool>) -> Result<EditState, String> {
    let mut w = work(&state);
    for side in [true, false] {
        if left.is_some_and(|l| l != side) {
            continue;
        }
        if let Some(wk) = w.side(side).as_ref() {
            wk.save().map_err(|e| format!("Could not save {}: {e}", wk.real.display()))?;
            w.drop_side(side);
        }
    }
    Ok(w.state())
}

/// Throw the changes away: one side, or both (left = None).
#[tauri::command]
fn discard_edits(state: tauri::State<'_, AppState>, left: Option<bool>) -> EditState {
    let mut w = work(&state);
    for side in [true, false] {
        if left.is_none_or(|l| l == side) {
            w.drop_side(side);
        }
    }
    w.state()
}

#[tauri::command]
fn edit_state(state: tauri::State<'_, AppState>) -> EditState {
    work(&state).state()
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

/// Save copied text to a file in the temporary folder, to compare it like any file.
#[tauri::command]
fn save_clipboard_text(text: String) -> Result<String, String> {
    let dir = std::env::temp_dir().join("SideBySide");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = dir.join("Pasted text (clipboard).txt");
    std::fs::write(&p, text).map_err(|e| e.to_string())?;
    Ok(p.to_string_lossy().into_owned())
}

/// Download a file from a web link (http or https only) to the temporary folder, to compare it.
/// Each download gets its own folder, so two versions of "app.ts" don't overwrite each other.
#[tauri::command]
async fn fetch_url(url: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || download(url.trim())).await.map_err(|e| e.to_string())?
}

static DOWNLOADS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Biggest file that is downloaded.
const DOWNLOAD_MAX: u64 = 512 << 20;

fn download(url: &str) -> Result<String, String> {
    use std::io::Read;
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("Only web links that start with http:// or https:// can be opened.".into());
    }
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(60)).user_agent("SideBySide").build();
    let resp = agent.get(url).call().map_err(|e| match e {
        ureq::Error::Status(code, _) => format!("The website answered with an error ({code}). Is the link right?"),
        e => format!("Couldn't download it. Are you online? ({e})"),
    })?;
    // The file name: the last part of the link (without ?… or #…), made safe for Windows.
    let path_part = url.split(['?', '#']).next().unwrap_or(url);
    let name: String = path_part
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .chars()
        .map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '_' } else { c })
        .collect();
    let name = if name.is_empty() || name.contains("://") { "page.txt".to_string() } else { name };
    let n = DOWNLOADS.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join("SideBySide").join(format!("web-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(name);
    let mut body = Vec::new();
    resp.into_reader().take(DOWNLOAD_MAX + 1).read_to_end(&mut body).map_err(|e| format!("The download stopped: {e}"))?;
    if body.len() as u64 > DOWNLOAD_MAX {
        return Err("That file is bigger than 512 MB, too big to download here. Save it first, then open it.".into());
    }
    std::fs::write(&target, body).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().into_owned())
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
    // "sidebyside old.txt new.txt --summary / --patch / --report out.html": no window (see cli.rs).
    if cli::wanted(&args) {
        std::process::exit(cli::run(&args));
    }

    // Started by an update or by "open the installed copy": wait for the old window to close
    // first, or the single-window rule would hand this start over to it.
    if let Some(pid) = args.iter().find_map(|a| a.strip_prefix(install::WAIT_FOR_ARG)).and_then(|p| p.parse().ok()) {
        install::wait_for_exit(pid);
    }

    update::clean_up_old();
    engine::remove_left_over_copies(install::process_running);
    install::refresh_registered_version(env!("CARGO_PKG_VERSION"));

    let own_window = args.iter().any(|a| a == "--wait");
    let cwd = std::env::current_dir().unwrap_or_default();
    let state = AppState {
        pending_files: Mutex::new(files_from_args(args, &cwd)),
        ..Default::default()
    };

    // "--wait" (used by Git, see git_setup): this start gets its own window and only ends when
    // that window is closed, so Git waits before it removes its temporary copies of the files.
    let mut builder = tauri::Builder::default();
    if !own_window {
        // Only one SideBySide window: opening it again (or right-clicking a file)
        // hands the files to the window that is already open.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
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
        }));
    }
    builder
        .manage(state)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
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
            save_report,
            open_report,
            make_patch,
            git_setup,
            save_clipboard_text,
            fetch_url,
            copy_block,
            compare_folders,
            is_folder,
            edit_line,
            undo_redo,
            save_edits,
            discard_edits,
            edit_state,
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
