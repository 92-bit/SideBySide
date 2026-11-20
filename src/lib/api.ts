// Talking to the comparing engine (src-tauri/src/engine.rs).
import { invoke } from '@tauri-apps/api/core';

export interface SideInfo {
  name: string;
  path: string;
  size: number;
  lines: number;
  /** Only part of the file is compared: its first and last line. */
  part: [number, number] | null;
  /** The text shown is the file itself, so it can be edited. */
  editable: boolean;
}

export interface Summary {
  left: SideInfo;
  right: SideInfo;
  totalRows: number;
  blocks: number;
  added: number;
  removed: number;
  changed: number;
  /** Lines that moved to another place unchanged. */
  moved: number;
  millis: number;
}

/** A piece of a line. h = highlighted (changed). */
export interface Seg {
  t: string;
  h: boolean;
}

export interface Cell {
  n: number;
  s: Seg[];
}

/** k: 0 = same, 1 = changed, 2 = added, 3 = removed */
export interface Row {
  /** 0 = same, 1 = changed, 2 = added, 3 = removed, 4 = folded unchanged rows,
   *  5 = moved away from here (left), 6 = moved here (right) */
  k: 0 | 1 | 2 | 3 | 4 | 5 | 6;
  /** A moved row: its line number on the other side. */
  m?: number;
  l: Cell | null;
  r: Cell | null;
  /** A folded row: how many unchanged rows it hides. */
  f?: number;
}

/** A place found by Go to line or Find. If it was inside a fold, that fold was opened:
 *  opened = [the fold's screen row, how many rows the screen gained]. */
export interface Spot {
  row: number;
  total: number;
  opened: [number, number] | null;
}

export interface Hit {
  index: number;
  row: number;
  rows: number;
}

export interface Options {
  ignoreSpaces: boolean;
  ignoreCase: boolean;
  ignoreBlankLines: boolean;
  /** Parts of lines that don't count, as regular expressions (see rules.ts). */
  ignoreRules: string[];
  /** Tidy up JSON and XML files first (one item per line). */
  tidy: boolean;
  /** Compare only some lines: [first, last], counting from 1. */
  leftLines: [number, number] | null;
  rightLines: [number, number] | null;
}

/** Returned as the error when Cancel was pressed, or a newer comparison replaced this one. */
export const CANCELLED = '__cancelled__';
export const STALE = '__stale__';
/** Returned as the error when a file was changed on disk after it was compared. */
export const CHANGED = '__changed__';

export const compare = (left: string, right: string, options: Options) =>
  invoke<Summary>('compare', { left, right, options });
/** 0 to 1000 */
export const progress = () => invoke<number>('progress');
export const cancel = () => invoke<void>('cancel');
export const fetchRows = (start: number, count: number) => invoke<Row[]>('rows', { start, count });
/** dir: 1 = next, -1 = previous, 0 = first at or after row */
export const nav = (row: number, dir: number) => invoke<Hit | null>('nav', { row, dir });
export const overview = (buckets: number) => invoke<number[]>('overview', { buckets });
/** line starts at 1 */
export const rowForLine = (left: boolean, line: number) => invoke<Spot | null>('row_for_line', { left, line });
export const find = (query: string, from: number | null, forward: boolean, matchCase: boolean) =>
  invoke<Spot | null>('find', { query, from, forward, matchCase });
/** Save the comparison as an HTML report; `when` is printed in it. */
export const saveReport = (path: string, when: string) => invoke<void>('save_report', { path, when });
/** Open the report that was just saved, in the browser. */
export const openReport = () => invoke<void>('open_report');
/** Using SideBySide as Git's compare tool. */
export interface GitSetup {
  commands: string[];
  gitFound: boolean;
  /** What Git's compare tool is set to now ('' if nothing). */
  current: string;
  /** Why Git couldn't be used ('' if it could). */
  problem: string;
}
/** apply = false: only look; true: change Git's settings (git config --global). */
export const gitSetup = (apply: boolean) => invoke<GitSetup>('git_setup', { apply });
// ---- Folders ----
/** One file in a folder comparison. kind: 0 = the same, 1 = different, 2 = only in the first folder, 3 = only in the second. */
export interface FolderEntry {
  path: string;
  kind: 0 | 1 | 2 | 3;
  leftSize: number | null;
  rightSize: number | null;
}
export interface FolderResult {
  left: string;
  right: string;
  entries: FolderEntry[];
  same: number;
  different: number;
  onlyLeft: number;
  onlyRight: number;
  millis: number;
}
export const compareFolders = (left: string, right: string) => invoke<FolderResult>('compare_folders', { left, right });
export const isFolder = (path: string) => invoke<boolean>('is_folder', { path });

// ---- Three files ----
export interface Summary3 {
  original: SideInfo;
  a: SideInfo;
  b: SideInfo;
  totalRows: number;
  changes: number;
  conflicts: number;
  millis: number;
}
/** k: 0 = unchanged, 1 = changed in A, 2 = changed in B, 3 = the same change in both, 4 = conflict. */
export interface Row3 {
  k: 0 | 1 | 2 | 3 | 4;
  a: { n: number; t: string } | null;
  o: { n: number; t: string } | null;
  b: { n: number; t: string } | null;
}
export const compareThree = (original: string, a: string, b: string, options: Options) =>
  invoke<Summary3>('compare_three', { original, a, b, options });
export const threeRows = (start: number, count: number) => invoke<Row3[]>('three_rows', { start, count });
export const threeNav = (row: number, dir: number) => invoke<Hit | null>('three_nav', { row, dir });

// ---- Editing and merging ----
/** Which files have unsaved changes, and whether Undo / Redo can do anything. */
export interface EditState {
  left: boolean;
  right: boolean;
  canUndo: boolean;
  canRedo: boolean;
}
/** Copy the difference at screen row `row` to the other side (toLeft: from the right file into the left). */
export const copyBlock = (row: number, toLeft: boolean) => invoke<EditState>('copy_block', { row, toLeft });
/** Change line `line` (from 1) of one file to `text`. */
export const editLine = (left: boolean, line: number, text: string) => invoke<EditState>('edit_line', { left, line, text });
export const undoRedo = (redo: boolean) => invoke<EditState>('undo_redo', { redo });
/** Save one side (left = true / false) or both (null). */
export const saveEdits = (left: boolean | null) => invoke<EditState>('save_edits', { left });
export const discardEdits = (left: boolean | null) => invoke<EditState>('discard_edits', { left });
export const editState = () => invoke<EditState>('edit_state');

/** Download a file from a web link (http/https) to a temporary file; returns its path. */
export const fetchUrl = (url: string) => invoke<string>('fetch_url', { url });
/** Save copied text to a temporary file (to compare it); returns its path. */
export const saveClipboardText = (text: string) => invoke<string>('save_clipboard_text', { text });
/** The differences as a patch (unified diff text). */
export const makePatch = () => invoke<string>('make_patch');
/** "Hide unchanged" on or off; returns how many rows the screen has now. */
export const setFold = (on: boolean) => invoke<number>('set_fold', { on });
/** Opens the fold at screen row `row`; returns [rows gained, rows on the screen now]. */
export const openFold = (row: number) => invoke<[number, number] | null>('open_fold', { row });
export const closeCompare = () => invoke<void>('close_compare');
/** Live reload: true if a compared file changed on disk since it was compared. */
export const filesChanged = () => invoke<boolean>('files_changed');
export const savePasted = (original: string, changed: string) =>
  invoke<[string, string]>('save_pasted', { original, changed });
/** Files handed to the app (at start, or later from the right-click menu) not yet picked up. */
export const takeFiles = () => invoke<string[]>('take_files');

export interface InstallInfo {
  supported: boolean;
  runningInstalled: boolean;
  installed: boolean;
  installPath: string;
}
export const installInfo = () => invoke<InstallInfo>('install_info');
export const installApp = (contextMenu: boolean, desktopShortcut: boolean) =>
  invoke<void>('install_app', { contextMenu, desktopShortcut });
export const openInstalled = () => invoke<void>('open_installed');

export interface UpdateInfo {
  available: boolean;
  current: string;
  latest: string;
  notes: string;
  date: string;
  canInstall: boolean;
}
export const checkUpdate = () => invoke<UpdateInfo>('check_update');
/** Downloads, checks the signature, replaces the app and restarts it. */
export const installUpdate = () => invoke<void>('install_update');

export const PROJECT_URL = 'https://github.com/92-bit/SideBySide';
export const RELEASES_URL = 'https://github.com/92-bit/SideBySide/releases/latest';

export const fmtNum = (n: number) => n.toLocaleString('en-US');

export function fmtSize(bytes: number): string {
  const units = ['bytes', 'KB', 'MB', 'GB', 'TB'];
  let v = bytes;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  return u === 0 ? `${v} bytes` : `${v.toFixed(v < 10 ? 1 : 0)} ${units[u]}`;
}
