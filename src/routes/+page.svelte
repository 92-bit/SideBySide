<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { readText } from '@tauri-apps/plugin-clipboard-manager';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import TitleBar from '#lib/components/TitleBar.svelte';
  import RollingNumber from '#lib/components/RollingNumber.svelte';
  import DiffView from '#lib/components/DiffView.svelte';
  import FindBar from '#lib/components/FindBar.svelte';
  import RecentList from '#lib/components/RecentList.svelte';
  import PastePanel from '#lib/components/PastePanel.svelte';
  import InstallDialog from '#lib/components/InstallDialog.svelte';
  import UpdateDialog from '#lib/components/UpdateDialog.svelte';
  import ShortcutsDialog from '#lib/components/ShortcutsDialog.svelte';
  import RulesPanel from '#lib/components/RulesPanel.svelte';
  import LinesPanel from '#lib/components/LinesPanel.svelte';
  import GitDialog from '#lib/components/GitDialog.svelte';
  import { activeRules, loadRules, saveRules, type RulesConfig } from '#lib/rules.ts';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { getVersion } from '@tauri-apps/api/app';
  import { listen } from '@tauri-apps/api/event';
  import {
    CANCELLED,
    STALE,
    cancel,
    closeCompare,
    compare,
    filesChanged as filesChangedOnDisk,
    find,
    openFold,
    setFold,
    saveReport,
    openReport,
    makePatch,
    saveClipboardText,
    type Spot,
    fmtNum,
    fmtSize,
    progress,
    rowForLine,
    savePasted,
    takeFiles,
    installInfo,
    checkUpdate,
    PROJECT_URL,
    type UpdateInfo,
    type InstallInfo,
    type Hit,
    type Summary,
  } from '#lib/api.ts';
  import { addRecent, clearRecent, fileName, loadRecent, removeRecent, type Recent } from '#lib/recent.ts';
  import {
    FONT_DEFAULT,
    loadAutoUpdate,
    loadLive,
    saveLive,
    loadReopen,
    saveReopen,
    loadSession,
    saveSession,
    type Session,
    markUpdateChecked,
    updateCheckDue,
  } from '#lib/settings.ts';
  import { applyCodeFont, prefs, setFoldPref, setInline, setTidy, tidyKind, zoomStep } from '#lib/prefs.svelte.ts';
  import { changeLook, followSystem, loadLook, setLook, type Look } from '#lib/theme.ts';

  type SideName = 'left' | 'right';

  let look = $state<Look>(loadLook());
  let paths = $state<{ left: string | null; right: string | null }>({ left: null, right: null });
  let summary = $state<Summary | null>(null);
  let busy = $state(false);
  let showBusy = $state(false);
  let pct = $state(0);
  let error = $state<string | null>(null);
  let current = $state<Hit | null>(null);
  let dropSide = $state<SideName | null>(null);
  let stageEl = $state<HTMLDivElement>();
  let ignore = $state({ spaces: false, blank: false, caseLetters: false, rules: false });
  /** The ignore options of the result on screen, so Cancel can put the buttons back. */
  let shownIgnore = { spaces: false, blank: false, caseLetters: false, rules: false };
  let findBar = $state<ReturnType<typeof FindBar>>();
  /** A compared file was changed on disk since. */
  let filesChanged = $state(false);
  let view = $state<ReturnType<typeof DiffView>>();
  let recent = $state<Recent[]>([]);
  let recentOpen = $state(false);
  let pasteMode = $state(false);
  let findMode = $state<'find' | 'goto' | null>(null);
  let query = $state('');
  let matchCase = $state(false);
  let lastFound: number | null = null;
  let version = $state('');
  let install = $state<InstallInfo | null>(null);
  let installOpen = $state(false);
  let aboutOpen = $state(false);
  /** Live reload: compare again by itself when a file changes on disk (e.g. a growing log). */
  let live = $state(false);
  /** A live update is running (quietly, the old result stays on screen). */
  let updating = $state(false);
  /** When the last live update finished, shown in the status bar. */
  let updatedAt = $state<Date | null>(null);
  /** A live update couldn't read a file (e.g. it is being saved right now); tried again next time. */
  let liveProblem = $state<string | null>(null);
  /** Where the view was before a live update, so it stays there afterwards. */
  let keepPlace = $state<{ top: number; x: number; atEnd: boolean } | null>(null);
  /** Bookmarked rows of the comparison on screen (F2 / Shift+F2 jump between them). */
  let bookmarks = $state<number[]>([]);
  /** Set when the daily check found a newer version. */
  let newVersion = $state<UpdateInfo | null>(null);

  let keysOpen = $state(false);
  let gitOpen = $state(false);
  function onHelp(action: 'check' | 'about' | 'site' | 'keys' | 'git') {
    if (action === 'site') openUrl(PROJECT_URL);
    else if (action === 'keys') keysOpen = true;
    else if (action === 'git') gitOpen = true;
    else aboutOpen = true;
  }
  // Show the Install button only for the portable copy on Windows.
  const canInstall = $derived(!!install?.supported && !install.runningInstalled);

  // The seam between the two file cards: wide enough for the current difference's number ("12,345").
  const seamW = $derived(summary ? Math.max(40, fmtNum(summary.blocks).length * 7 + 22) : 40);

  /** The folder part of a path, shortened to its last two folders. */
  function folderOf(p: string): string {
    const parts = p.split(/[\\/]/).slice(0, -1);
    return parts.length > 3 ? `…\\${parts.slice(-2).join('\\')}` : parts.join('\\');
  }
  // The swap button turns half a circle each time it is used.
  let swapped = $state(false);
  function swapCards() {
    swapped = !swapped;
    swap();
  }

  const isPasted = (p: string | null) => !!p && /[\\/]SideBySide[\\/]Pasted text \(/.test(p);

  // The counter rolls only for clicks on the arrows; F7/F8 change it instantly (keys are used a lot).
  let roll = $state(0);
  function stepDiff(d: 1 | -1) {
    roll = d;
    Promise.resolve(d > 0 ? view?.next(true) : view?.prev(true)).finally(() => setTimeout(() => (roll = 0)));
  }

  function pickLook(next: Look, from: { x: number; y: number }) {
    if (next.theme === look.theme && next.mode === look.mode) return;
    look = next;
    changeLook(next, from);
  }

  // ---- Progress bar ----
  let timer: ReturnType<typeof setInterval> | undefined;
  function startProgress() {
    const started = performance.now();
    pct = 0;
    showBusy = false;
    clearInterval(timer);
    timer = setInterval(async () => {
      // Only show the progress box if it takes a moment, so small files don't flicker.
      if (performance.now() - started > 250) showBusy = true;
      pct = await progress();
    }, 120);
  }
  function stopProgress() {
    clearInterval(timer);
    showBusy = false;
  }

  // ---- Comparing ----
  // If another comparison starts meanwhile, the older result is thrown away.
  let runId = 0;
  /** quiet: a live update. The old result stays on screen (no progress box) and the view keeps its place. */
  /** restore: reopening the last comparison at start (its place, and its bookmarks if the files are unchanged).
   *  Returns true if the comparison is now on screen. */
  async function run(quiet = false, restore: Session | null = null): Promise<boolean> {
    const { left, right } = paths;
    if (!left || !right) return false;
    const id = ++runId;
    const used = { ...ignore };
    if (quiet) {
      updating = true;
    } else {
      busy = true;
      error = null;
      startProgress();
    }
    try {
      const s = await compare(left, right, {
        ignoreSpaces: used.spaces,
        ignoreCase: used.caseLetters,
        ignoreBlankLines: used.blank,
        ignoreRules: used.rules ? activeRules(rulesCfg) : [],
        tidy: prefs.tidy,
        leftLines: ranges.left,
        rightLines: ranges.right,
      });
      if (id === runId) {
        keepPlace = quiet ? (view?.getPlace() ?? null) : (restore?.place ?? null);
        summary = s;
        shownIgnore = used;
        shownTidy = prefs.tidy;
        filesChanged = false;
        current = null;
        lastFound = null;
        liveProblem = null;
        if (quiet) {
          updatedAt = new Date();
          bookmarks = bookmarks.filter((b) => b < s.totalRows); // (kept: a growing log keeps its rows)
        } else {
          updatedAt = null;
          const same = restore && s.left.size === restore.sizes[0] && s.right.size === restore.sizes[1];
          bookmarks = same ? restore.bookmarks.filter((b) => b < s.totalRows) : [];
          pasteMode = false;
          if (!isPasted(left)) recent = addRecent(left, right);
        }
        return true;
      }
    } catch (e) {
      const msg = String(e);
      if (quiet) {
        // Keep showing the last result; the next check tries again.
        if (id === runId && msg !== STALE && msg !== CANCELLED) liveProblem = msg;
      } else if (id === runId && msg !== STALE) {
        if (msg === CANCELLED) {
          // Go back to what was showing before.
          if (summary) {
            paths = { left: summary.left.path, right: summary.right.path };
            ignore = { ...shownIgnore };
          }
        } else {
          error = msg;
          summary = null;
        }
      }
    } finally {
      if (id === runId) {
        busy = false;
        updating = false;
        stopProgress();
      }
    }
    return false;
  }

  // ---- Reopen the last comparison at start ----
  let reopen = $state(false);
  function toggleReopen() {
    reopen = !reopen;
    saveReopen(reopen);
  }
  // Remember the comparison on screen (files, options, place, bookmarks) every couple of seconds.
  $effect(() => {
    const t = setInterval(() => {
      if (!summary || busy || isPasted(summary.left.path)) return;
      saveSession({
        left: summary.left.path,
        right: summary.right.path,
        ignore: { ...shownIgnore },
        place: view?.getPlace() ?? null,
        bookmarks: [...bookmarks],
        sizes: [summary.left.size, summary.right.size],
      });
    }, 2000);
    return () => clearInterval(t);
  });
  async function reopenLast() {
    const last = loadSession();
    if (!last) return;
    paths = { left: last.left, right: last.right };
    ignore = { rules: false, ...last.ignore };
    if (!(await run(false, last)) && error) {
      const gone = /^Could not open "(.+?)":.*(cannot find|not find|No such file)/i.exec(error);
      error = gone
        ? `The last comparison couldn't be reopened: "${gone[1]}" was moved or deleted.`
        : `The last comparison couldn't be reopened. ${error}`;
      paths = { left: null, right: null };
      ignore = { spaces: false, blank: false, caseLetters: false, rules: false };
    }
  }

  // ---- Live reload ----
  // Once a second, look at the two files' size and time (cheap, even for huge files). When one
  // has changed, compare again quietly. A new check waits until the last update has finished.
  $effect(() => {
    if (!live || !summary || busy) return;
    const t = setInterval(async () => {
      if (updating || busy) return;
      if (await filesChangedOnDisk().catch(() => false)) run(true);
    }, 1000);
    return () => clearInterval(t);
  });
  function toggleLive() {
    live = !live;
    saveLive(live);
    if (!live) liveProblem = null;
    else if (filesChanged) run(true);
  }
  /** "12:04:31" */
  const clock = (d: Date) => d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });

  // ---- Compare only some lines ----
  let ranges = $state<{ left: [number, number] | null; right: [number, number] | null }>({ left: null, right: null });
  let linesOpen = $state<SideName | null>(null);
  /** How many lines each whole file has (from the last time it was compared whole). */
  let fullLines = $state({ left: 0, right: 0 });
  $effect(() => {
    if (summary && !summary.left.part) fullLines.left = summary.left.lines;
    if (summary && !summary.right.part) fullLines.right = summary.right.lines;
  });
  function setRange(side: SideName, r: [number, number] | null) {
    ranges[side] = r;
    linesOpen = null;
    run();
  }
  /** The last line number in a file as compared (its part, if only part of it). */
  const lastLine = (info: { lines: number; part: [number, number] | null }) => (info.part ? info.part[1] : info.lines);

  function setPath(side: SideName, p: string) {
    paths[side] = p;
    ranges[side] = null;
    run();
  }

  async function choose(side: SideName) {
    const p = await open({
      multiple: false,
      directory: false,
      title: side === 'left' ? 'Choose the original file' : 'Choose the new file',
    });
    if (typeof p === 'string') setPath(side, p);
  }

  function swap() {
    paths = { left: paths.right, right: paths.left };
    ranges = { left: ranges.right, right: ranges.left };
    run();
  }

  function closeAll() {
    runId++;
    ranges = { left: null, right: null };
    busy = false;
    stopProgress();
    summary = null;
    current = null;
    error = null;
    findMode = null;
    bookmarks = [];
    paths = { left: null, right: null };
    closeCompare();
  }
  /** "New" in the top bar: back to the start screen. */
  function newCompare() {
    closeAll();
    saveSession(null); // the start screen is where you left off now
    pasteMode = false;
    recentOpen = false;
  }

  // ---- Ignore rules (times, IDs...) ----
  let rulesCfg = $state<RulesConfig>(loadRules());
  let rulesOpen = $state(false);
  let rulesWrap = $state<HTMLSpanElement>();
  function applyRules(c: RulesConfig, on: boolean) {
    rulesCfg = c;
    saveRules(c);
    rulesOpen = false;
    ignore.rules = on;
    run();
  }
  // ---- Tidy up JSON / XML ----
  const tidyable = $derived(summary ? (tidyKind(summary.left.name) ?? tidyKind(summary.right.name)) : null);
  /** The tidy result on screen (the setting may have changed since). */
  let shownTidy = $state(false);
  let tidyTipHidden = $state(false);
  function changeTidy(on: boolean) {
    setTidy(on);
    run();
  }

  function onWindowClick(e: MouseEvent) {
    if (shareOpen && !(e.target as Element).closest?.('.share')) shareOpen = false;
    if (rulesOpen && rulesWrap && !rulesWrap.contains(e.target as Node)) rulesOpen = false;
    if (linesOpen && !(e.target as Element).closest?.('.lw')) linesOpen = null;
  }

  function toggle(k: 'spaces' | 'blank' | 'caseLetters') {
    ignore[k] = !ignore[k];
    run();
  }

  function openRecent(r: Recent) {
    recentOpen = false;
    pasteMode = false;
    paths = { left: r.left, right: r.right };
    ranges = { left: null, right: null };
    run();
  }

  async function comparePasted(a: string, b: string) {
    try {
      const [l, r] = await savePasted(a, b);
      paths = { left: l, right: r };
      ranges = { left: null, right: null };
      run();
    } catch (e) {
      error = String(e);
    }
  }

  // ---- Find and go to line ----
  $effect(() => {
    void query;
    void matchCase;
    lastFound = null;
  });

  async function doFind(forward: boolean): Promise<boolean> {
    if (!view || !query) return false;
    const t = view.getTop();
    const from = lastFound ?? (forward ? (t > 0 ? t - 1 : null) : t > 0 ? t : null);
    try {
      const spot = await find(query, from, forward, matchCase);
      if (spot === null) return false;
      lastFound = spot.row;
      await showSpot(spot);
      return true;
    } catch {
      return false;
    }
  }

  async function doGoto(left: boolean, line: number): Promise<boolean> {
    const spot = await rowForLine(left, line);
    if (spot === null || !view) return false;
    await showSpot(spot);
    return true;
  }

  // ---- Share: report and patch ----
  let shareOpen = $state(false);
  /** A short message at the bottom (e.g. "Copied"), with an optional button; it goes away by itself. */
  let notice = $state<{ text: string; action?: { label: string; run: () => void } } | null>(null);
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  function tell(text: string, action?: { label: string; run: () => void }) {
    notice = { text, action };
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = null), action ? 8000 : 3500);
  }
  /** "old.txt vs new.txt", safe to use as a file name. */
  const pairName = () => (summary ? `${summary.left.name} vs ${summary.right.name}`.replace(/[\/:*?"<>|]+/g, '_') : 'comparison');
  async function exportReport() {
    shareOpen = false;
    if (!summary) return;
    const path = await save({
      title: 'Save the comparison as a report',
      defaultPath: `${pairName()}.html`,
      filters: [{ name: 'Web page (opens in any browser, can be printed to PDF)', extensions: ['html'] }],
    });
    if (!path) return;
    try {
      await saveReport(path, new Date().toLocaleString());
      tell('Report saved.', { label: 'Open it', run: () => openReport() });
    } catch (e) {
      tell(String(e));
    }
  }
  async function copyPatch() {
    shareOpen = false;
    if (!summary) return;
    try {
      await navigator.clipboard.writeText(await makePatch());
      tell('Copied as a patch. Paste it into an e-mail, a chat or a code review.');
    } catch (e) {
      tell(`Couldn't copy: ${e}`);
    }
  }

  // ---- Compare with the clipboard (Ctrl+Shift+V) ----
  /** The original file (left) against whatever text was copied last. */
  async function compareClipboard() {
    const left = summary?.left.path ?? paths.left;
    if (!left || isPasted(left)) {
      tell('Choose the original file first; then press Ctrl+Shift+V to compare it with what you copied.');
      return;
    }
    let text = '';
    try {
      text = (await readText()) ?? '';
    } catch {
      text = '';
    }
    if (!text) {
      tell('There is no text in the clipboard. Copy some text first.');
      return;
    }
    try {
      const p = await saveClipboardText(text);
      pasteMode = false;
      recentOpen = false;
      paths = { left, right: p };
      ranges = { left: ranges.left, right: null };
      run();
    } catch (e) {
      tell(String(e));
    }
  }

  // ---- Hide unchanged (folds) ----
  /** The screen gained `added` rows below screen row `at` (a fold was opened): move bookmarks along. */
  function foldOpened(at: number, added: number, total: number, place: { top: number; x: number; atEnd: boolean } | null) {
    bookmarks = bookmarks.map((b) => (b > at ? b + added : b));
    keepPlace = place;
    current = null; // the view starts again; F7 / F8 carry on from where it is
    if (summary) summary = { ...summary, totalRows: total };
  }
  /** Show a found place; if it was hidden in a fold, that fold was opened first. */
  async function showSpot(spot: Spot) {
    if (spot.opened) {
      foldOpened(spot.opened[0], spot.opened[1], spot.total, { top: Math.max(0, spot.row - 4), x: 0, atEnd: false });
      await tick();
    }
    view?.reveal(spot.row);
  }
  async function openFoldAt(row: number) {
    const res = await openFold(row);
    if (res) foldOpened(row, res[0], res[1], view?.getPlace() ?? null);
  }
  async function toggleFold() {
    setFoldPref(!prefs.fold);
    const total = await setFold(prefs.fold);
    if (!summary) return;
    bookmarks = []; // the rows are numbered differently now
    keepPlace = null;
    current = null;
    summary = { ...summary, totalRows: total };
  }

  // ---- Files sent to the app ----
  // Two files: compare them. One file: it becomes the original, then the next one is the new file.
  function receiveFiles(files: string[]) {
    if (!files.length) return;
    pasteMode = false;
    recentOpen = false;
    if (files.length >= 2) {
      paths = { left: files[0], right: files[1] };
      run();
    } else if (paths.left && !paths.right) {
      setPath('right', files[0]);
    } else {
      // Also stop a comparison that is still running, so it can't show up later.
      if (summary || busy) closeAll();
      paths = { left: files[0], right: null };
    }
  }

  // ---- Zoom ----
  function zoom(step: number) {
    zoomStep(step);
  }

  // ---- Keyboard shortcuts ----
  function onKey(e: KeyboardEvent) {
    if (e.key === 'F1') {
      e.preventDefault(); // (otherwise WebView2 opens its own help)
      keysOpen = !keysOpen;
      return;
    }
    if (!e.ctrlKey || e.altKey) {
      if (e.key === 'Escape' && findMode) findMode = null;
      return;
    }
    const k = e.key.toLowerCase();
    if ((k === 'f' || k === 'g') && summary) {
      const mode = k === 'f' ? 'find' : 'goto';
      // Already open in this mode: just put the cursor back in the box.
      if (findMode === mode) findBar?.focus();
      else findMode = mode;
    }
    else if (k === '=' || k === '+') zoom(1);
    else if (k === '-') zoom(-1);
    else if (k === '0') zoom(0);
    else if (k === 'o') choose(paths.left ? 'right' : 'left');
    else if (k === 'v' && e.shiftKey) compareClipboard();
    else return;
    e.preventDefault();
  }

  onMount(() => {
    setLook(look); // also saves the colours for app.html, so the next start shows them at once
    const stopFollowing = followSystem(() => look);
    recent = loadRecent();
    applyCodeFont();
    live = loadLive();

    getVersion().then((v) => (version = v));
    installInfo().then((i) => (install = i));

    // Quietly look for a new version, at most once a day (can be switched off in Help → About).
    if (loadAutoUpdate() && updateCheckDue()) {
      setTimeout(() => {
        checkUpdate()
          .then((u) => {
            markUpdateChecked();
            if (u.available) newVersion = u;
          })
          .catch(() => {}); // offline: try again next time
      }, 3000);
    }

    // Files given when starting (sidebyside.exe old.txt new.txt), or sent later from the right-click menu.
    // Start listening first, then pick up what is already waiting; otherwise a file that
    // arrives in between (e.g. the 2nd of two right-clicked files) would be missed.
    const unlistenFiles = listen('files-arrived', () => takeFiles().then(receiveFiles));
    reopen = loadReopen();
    unlistenFiles
      .then(() => setFold(prefs.fold)) // before the first comparison
      .then(() => takeFiles())
      .then((files) => {
        // Files given at start always win; otherwise reopen the last comparison if that is switched on.
        if (files.length) receiveFiles(files);
        else if (reopen) reopenLast();
      });

    // Drag and drop: drop on the left or right half, or drop two files at once.
    // (The middle of the two panes, not of the window: the icon bar on the left moves them over.)
    const sideAt = (x: number): SideName => {
      const r = stageEl?.getBoundingClientRect();
      const middle = r && r.width ? r.left + r.width / 2 : window.innerWidth / 2;
      return x / window.devicePixelRatio < middle ? 'left' : 'right';
    };
    const unlisten = getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === 'enter' || p.type === 'over') {
        dropSide = sideAt(p.position.x);
      } else if (p.type === 'drop') {
        dropSide = null;
        pasteMode = false;
        if (p.paths.length >= 2) {
          paths = { left: p.paths[0], right: p.paths[1] };
          run();
        } else if (p.paths.length === 1) {
          setPath(sideAt(p.position.x), p.paths[0]);
        }
      } else {
        dropSide = null;
      }
    });
    return () => {
      clearInterval(timer);
      unlisten.then((f) => f());
      unlistenFiles.then((f) => f());
      stopFollowing();
    };
  });
</script>

<svelte:window onkeydown={onKey} onclick={onWindowClick} />

<div class="app">
  <TitleBar {look} onLook={pickLook} {onHelp}>
    {#snippet lead()}
      <button type="button" class="lb" disabled={!summary && !paths.left && !paths.right && !pasteMode} onclick={newCompare} title="New comparison (closes this one)">
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg><span class="lbl">New</span>
      </button>
      <button type="button" class="lb" class:on={recentOpen} aria-pressed={recentOpen} disabled={!recent.length && !recentOpen} onclick={() => (recentOpen = !recentOpen)} title="Recent comparisons">
        <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="8.5" /><path d="M12 7.5V12l3 2" /></svg><span class="lbl">Recent</span>
      </button>
    {/snippet}
    {#snippet island()}
      {@const none = !summary || summary.blocks === 0}
      <button type="button" class="ib sq" disabled={none} onclick={() => stepDiff(-1)} title="Previous difference (F7)" aria-label="Previous difference">
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 15l6-6 6 6" /></svg>
      </button>
      {#if summary && summary.blocks === 0}
        <span class="counter same">✓ No differences</span>
      {:else}
        <span class="counter" aria-live="polite">
          <b><RollingNumber value={summary && current ? fmtNum(current.index + 1) : '–'} dir={roll} /></b>
          <span>of {summary ? fmtNum(summary.blocks) : '–'}</span>
        </span>
      {/if}
      <button type="button" class="ib sq" disabled={none} onclick={() => stepDiff(1)} title="Next difference (F8)" aria-label="Next difference">
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 9l6 6 6-6" /></svg>
      </button>
      <i class="div"></i>
      <button type="button" class="ib" disabled={!summary} onclick={() => (findMode = 'find')} title="Find text (Ctrl+F)">
        <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="6" /><path d="M20 20l-4.5-4.5" /></svg><span class="lbl">Find</span>
      </button>
      <button type="button" class="ib" disabled={!summary} onclick={() => (findMode = 'goto')} title="Go to a line number (Ctrl+G)">
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 4L7 20M17 4l-2 16M4 9h16M3 15h16" /></svg><span class="lbl">Go to line</span>
      </button>
      <i class="div"></i>
      <span class="ig" role="group" aria-label="Ignore">
        <span class="iglbl">Ignore</span>
        <button type="button" class="chip" class:on={ignore.spaces} aria-pressed={ignore.spaces} onclick={() => toggle('spaces')} title="Ignore spaces: treat lines as equal if they only differ in spaces or tabs">
          <span class="dot"></span>Spaces
        </button>
        <button type="button" class="chip" class:on={ignore.blank} aria-pressed={ignore.blank} onclick={() => toggle('blank')} title="Ignore blank lines: don't count empty lines as differences">
          <span class="dot"></span><span class="long">Blank lines</span><span class="short">Blank</span>
        </button>
        <button type="button" class="chip" class:on={ignore.caseLetters} aria-pressed={ignore.caseLetters} onclick={() => toggle('caseLetters')} title="Ignore case: treat UPPER and lower case letters as the same">
          <span class="dot"></span>Case
        </button>
        <span class="rw" bind:this={rulesWrap}>
          <button type="button" class="chip" class:on={ignore.rules} aria-pressed={ignore.rules} aria-expanded={rulesOpen} onclick={() => (rulesOpen = !rulesOpen)} title="Ignore rules: leave out parts of lines, like times or IDs">
            <span class="dot"></span>Rules
          </button>
          {#if rulesOpen}
            <RulesPanel config={rulesCfg} on={ignore.rules} onApply={applyRules} onClose={() => (rulesOpen = false)} />
          {/if}
        </span>
      </span>
    {/snippet}
    {#snippet actions()}
      <span class="share" class:open={shareOpen}>
        <button type="button" class="sharebtn" disabled={!summary} aria-expanded={shareOpen} onclick={() => (shareOpen = !shareOpen)} title="Share: save a report or copy a patch" aria-label="Share">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 15V4M8 8l4-4 4 4M6 12v6a2 2 0 002 2h8a2 2 0 002-2v-6" /></svg>
        </button>
        {#if shareOpen}
          <div class="sharemenu" role="menu">
            <button type="button" role="menuitem" onclick={exportReport}>
              <b>Save as a report…</b><small>A web page with every difference. Opens in any browser; print it to make a PDF.</small>
            </button>
            <button type="button" role="menuitem" onclick={copyPatch}>
              <b>Copy as a patch</b><small>The differences as text, in the "diff" format that Git and code tools read.</small>
            </button>
          </div>
        {/if}
      </span>
      {#if canInstall}
        <button type="button" class="install" onclick={() => (installOpen = true)} title="Add SideBySide to your Start Menu" aria-label={install?.installed ? 'Update install' : 'Install'}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 4v11M7 10l5 5 5-5M5 20h14" /></svg>
          <span class="lbl">{install?.installed ? 'Update install' : 'Install'}</span>
        </button>
      {/if}
    {/snippet}
  </TitleBar>

  <div class="body">
    {#if recentOpen}
      <aside class="panel" aria-label="Recent comparisons">
        <h2>Recent comparisons</h2>
        <RecentList items={recent} onOpen={openRecent} onRemove={(r) => (recent = removeRecent(r))} onClear={() => (recent = clearRecent())} />
      </aside>
    {/if}

    <main class="main">
      {#if findMode && summary}
        <FindBar
          bind:this={findBar}
          mode={findMode}
          bind:query
          bind:matchCase
          maxLeft={lastLine(summary.left)}
          maxRight={lastLine(summary.right)}
          onFind={doFind}
          onGoto={doGoto}
          onClose={() => (findMode = null)}
        />
      {/if}

      {#if summary && tidyable && shownTidy}
        <div class="note">
          <span>Tidied up: one item per line. The line numbers are for the tidied text.</span>
          <button type="button" onclick={() => changeTidy(false)}>Show as they are</button>
        </div>
      {:else if summary && tidyable && !tidyTipHidden}
        <div class="note">
          <span>These are {tidyable} files. Tidy them up first, with one item per line? Then the formatting doesn't count, only the content.</span>
          <button type="button" class="go" onclick={() => changeTidy(true)}>Tidy up</button>
          <button type="button" class="x" aria-label="Hide this tip" title="Hide this tip" onclick={() => (tidyTipHidden = true)}>×</button>
        </div>
      {/if}

      {#if filesChanged && summary && !live}
        <div class="note" role="alert">
          <span>A file was changed after it was compared, so the text shown may be out of date.</span>
          <button type="button" onclick={() => run()}>Compare again</button>
        </div>
      {/if}

      <div class="stage" class:cmp={!!summary} bind:this={stageEl} style:--gw="{seamW}px">
        {#if summary}
          <div class="heads">
            {#each [['left', summary.left], ['right', summary.right]] as const as [side, info] (side)}
              {@const pasted = isPasted(info.path)}
              <div class="fh" class:old={side === 'left'} class:new={side === 'right'} class:drop={dropSide === side} style:grid-column={side === 'left' ? 1 : 3}>
                <span class="ic" aria-hidden="true">{side === 'left' ? 'OLD' : 'NEW'}</span>
                <div class="t" title={info.path}>
                  <b><span class="sr">{side === 'left' ? 'Original: ' : 'New: '}</span>{pasted ? 'Pasted text' : info.name}</b>
                  <small>{pasted ? '' : `${folderOf(info.path)} · `}{fmtSize(info.size)} · {fmtNum(info.lines)} {info.lines === 1 ? 'line' : 'lines'}</small>
                </div>
                <span class="lw">
                  <button type="button" class="change" class:on={!!info.part} aria-expanded={linesOpen === side} onclick={() => (linesOpen = linesOpen === side ? null : side)} title="Compare only some lines of this file">
                    {info.part ? `Lines ${fmtNum(info.part[0])}–${fmtNum(info.part[1])}` : 'Lines…'}
                  </button>
                  {#if linesOpen === side}
                    <LinesPanel {side} lines={fullLines[side] || info.lines} range={ranges[side]} onApply={(r) => setRange(side, r)} onClose={() => (linesOpen = null)} />
                  {/if}
                </span>
                <button type="button" class="change" onclick={() => choose(side)}>Change…</button>
              </div>
            {/each}
            <div class="swapdot">
              <button type="button" class:spun={swapped} onclick={swapCards} title="Swap left and right" aria-label="Swap left and right">
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7h11l-3-3M17 17H6l3 3" /></svg>
              </button>
            </div>
          </div>
          {#key summary}
            <DiffView
              bind:this={view}
              {summary}
              fontSize={prefs.fontSize}
              inline={prefs.inline}
              query={findMode === 'find' ? query : ''}
              {matchCase}
              onCurrent={(h) => {
                current = h;
                setTimeout(() => (roll = 0)); // after the counter has used it
              }}
              onZoom={zoom}
              place={keepPlace}
              bind:bookmarks
              onOpenFold={openFoldAt}
              onGotoLine={doGoto}
              onFilesChanged={() => (filesChanged = true)}
            />
          {/key}
        {:else if pasteMode}
          <PastePanel {error} onCompare={comparePasted} onBack={() => (pasteMode = false)} />
        {:else}
          <section class="empty">
            <div class="hello rise" style:--i={0}>
              <h2>What should we compare?</h2>
              <p>Drop two files anywhere, or choose them below. The comparison starts as soon as both are there.</p>
            </div>
            <div class="slots rise" style:--i={1}>
              {#each ['left', 'right'] as const as side, n (side)}
                {#if n === 1}<span class="vs" aria-hidden="true">vs</span>{/if}
                <button
                  type="button"
                  class="slot"
                  class:old={side === 'left'}
                  class:new={side === 'right'}
                  class:drop={dropSide === side}
                  class:filled={!!paths[side]}
                  onclick={() => choose(side)}
                  title={paths[side] ?? (side === 'left' ? 'Choose the original file' : 'Choose the new file')}
                >
                  <svg class="ant" aria-hidden="true"><rect x="1" y="1" rx="11" /></svg>
                  <span class="big" aria-hidden="true">{side === 'left' ? 'OLD' : 'NEW'}</span>
                  {#if paths[side]}
                    <b>{fileName(paths[side] ?? '')}</b>
                    <small>Click to choose another</small>
                  {:else}
                    <b>{side === 'left' ? 'Choose the original' : 'Choose the new version'}</b>
                    <small>or drop a file here</small>
                  {/if}
                </button>
              {/each}
            </div>
            {#if error}
              <p class="error" role="alert">{error}</p>
            {/if}
            <p class="orp rise" style:--i={2}>
              No files? <button type="button" class="link" onclick={() => (pasteMode = true)}>Paste text instead</button>
              {#if paths.left}
                · <button type="button" class="link" onclick={compareClipboard} title="Ctrl+Shift+V">Compare with what you copied</button>
              {/if}
            </p>
            {#if recent.length}
              <div class="recent-home rise" style:--i={3}>
                <h2>Recent</h2>
                <RecentList items={recent} limit={6} cards onOpen={openRecent} onRemove={(r) => (recent = removeRecent(r))} />
              </div>
            {/if}
            <button type="button" class="reopen rise" style:--i={4} role="switch" aria-checked={reopen} onclick={toggleReopen}>
              <span class="knob" aria-hidden="true"></span>
              Reopen my last comparison when SideBySide starts
            </button>
          </section>
        {/if}

        {#if busy && showBusy}
          <div class="busy" role="status">
            <div class="busy-card">
              <div class="scan" aria-hidden="true">
                {#each [[80, 60, 90, 40, 70, 55], [75, 65, 88, 45, 72, 50]] as widths, n (n)}
                  <div class="sheet" class:b={n === 1}>
                    {#each widths as w, j (j)}<i style:width="{w}%"></i>{/each}
                  </div>
                {/each}
              </div>
              <span class="pbar" aria-hidden="true"><i style:transform="scaleX({pct / 1000})"></i></span>
              <span class="busytxt">
                <span>{pct < 800 ? 'Reading both files…' : 'Matching lines…'}</span>
                <b>{Math.floor(pct / 10)}%</b>
              </span>
              <button type="button" class="cancel" onclick={() => cancel()}>Cancel</button>
            </div>
          </div>
        {/if}
      </div>
    </main>
  </div>

  {#if aboutOpen}
    <UpdateDialog version={version} portable={!install?.runningInstalled} known={newVersion} onClose={() => (aboutOpen = false)} />
  {/if}

  {#if gitOpen}
    <GitDialog onClose={() => (gitOpen = false)} />
  {/if}

  {#if keysOpen}
    <ShortcutsDialog onClose={() => (keysOpen = false)} />
  {/if}

  {#if installOpen && install}
    <InstallDialog info={install} onClose={() => (installOpen = false)} />
  {/if}

  {#if notice}
    <div class="toast" role="status">
      <span>{notice.text}</span>
      {#if notice.action}
        <button type="button" onclick={() => { notice?.action?.run(); notice = null; }}>{notice.action.label}</button>
      {/if}
    </div>
  {/if}

  <footer class="status">
    {#if busy}
      <span>Comparing…</span>
    {:else if error}
      <span>Couldn't compare</span>
    {:else if summary}
      {#if summary.blocks === 0}
        <span class="same">✓ Files are identical{ignore.spaces || ignore.caseLetters || ignore.blank || ignore.rules ? ' (with ignore options)' : ''}</span>
      {:else}
        {@const parts = summary.changed + summary.added + summary.removed + summary.moved || 1}
        {#key summary}
          <span class="stack" aria-hidden="true">
            <i class="c" style:flex={summary.changed / parts}></i><i class="a" style:flex={summary.added / parts}></i><i class="d" style:flex={summary.removed / parts}></i><i class="m" style:flex={summary.moved / parts}></i>
          </span>
        {/key}
        <b>{fmtNum(summary.blocks)} {summary.blocks === 1 ? 'difference' : 'differences'}</b>
        <span class="lg c">{fmtNum(summary.changed)} changed</span>
        <span class="lg a">{fmtNum(summary.added)} added</span>
        <span class="lg d">{fmtNum(summary.removed)} removed</span>
        {#if summary.moved}<span class="lg m">{fmtNum(summary.moved)} moved</span>{/if}
      {/if}
    {:else}
      <span>Ready</span>
    {/if}
    <span class="grow"></span>
    {#if summary && !busy}
      {#if live && liveProblem}
        <span class="live-note warn" title={liveProblem}>Can't read a file right now, trying again…</span>
      {:else if live && updating}
        <span class="live-note">Updating…</span>
      {:else if live && updatedAt}
        <span class="live-note">Updated {clock(updatedAt)}</span>
      {:else}
        <span class="took">Compared in {(summary.millis / 1000).toFixed(2)} s</span>
      {/if}
      <span class="views" role="group" aria-label="Layout">
        <button type="button" aria-pressed={!prefs.inline} title="Side by side" aria-label="Side by side" onclick={() => setInline(false)}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3.5" y="5" width="7.5" height="14" rx="1.5" /><rect x="13" y="5" width="7.5" height="14" rx="1.5" /></svg>
        </button>
        <button type="button" aria-pressed={prefs.inline} title="Inline: one column, removed words struck through, added words underlined" aria-label="Inline" onclick={() => setInline(true)}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3.5" y="5" width="17" height="14" rx="1.5" /><path d="M7 10h7M7 14h10" /></svg>
        </button>
      </span>
      <button
        type="button"
        class="live fold"
        class:on={prefs.fold}
        aria-pressed={prefs.fold}
        onclick={toggleFold}
        title={prefs.fold ? 'Unchanged lines are hidden (a few stay around each difference). Click to show them all.' : 'Hide long runs of unchanged lines, so only the differences and a few lines around them show'}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 4l5 5 5-5M7 20l5-5 5 5" /></svg>Hide unchanged
      </button>
      <button
        type="button"
        class="live"
        class:on={live}
        aria-pressed={live}
        onclick={toggleLive}
        title={live ? 'Live: on. The comparison updates by itself when a file changes. Click to turn off.' : 'Live: compare again by itself whenever a file changes (handy for log files)'}
      >
        <span class="pulse" aria-hidden="true"></span>Live
      </button>
    {/if}
    {#if newVersion}
      <button type="button" class="new-version" onclick={() => (aboutOpen = true)}>⬆ Version {newVersion.latest} is available</button>
    {/if}
    <button type="button" class="zoom" onclick={() => zoom(0)} title="Text size (Ctrl + mouse wheel, Ctrl +/-). Click to reset.">
      {Math.round((prefs.fontSize / FONT_DEFAULT) * 100)}%
    </button>
    <span class="ver">SideBySide {version ? `v${version}` : ''}{install?.runningInstalled ? '' : ' · portable'}</span>
  </footer>
</div>

<style>
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--t-bg);
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  /* Recent panel (opened from the clock in the top bar) */
  .panel {
    width: 280px;
    flex: none;
    margin: 4px 0 12px 14px;
    padding: 12px 8px;
    overflow-y: auto;
    background: var(--t-card2);
    border: 1px solid var(--t-line);
    border-radius: 14px;
    animation: panel-in 0.22s cubic-bezier(0.23, 1, 0.32, 1);
  }
  @keyframes panel-in {
    from {
      opacity: 0;
      transform: translateX(-8px);
    }
  }
  .panel h2 {
    margin: 0 8px 8px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--t-faint);
  }
  @media (prefers-reduced-motion: reduce) {
    .panel {
      animation: none;
    }
  }

  .main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  /* "New" and "Recent" beside the name in the top bar */
  .lb {
    height: 28px;
    padding: 0 9px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--t-muted);
    font-size: 12.5px;
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color 0.15s ease,
      color 0.15s ease,
      transform 0.16s cubic-bezier(0.23, 1, 0.32, 1);
  }
  .lb:hover:not(:disabled),
  .lb.on {
    background: var(--t-line);
    color: var(--t-strong);
  }
  .lb:active:not(:disabled) {
    transform: scale(0.95);
  }
  .lb:disabled {
    opacity: 0.35;
    cursor: default;
  }
  @container bar (max-width: 1240px) {
    .lb .lbl {
      display: none;
    }
    .lb {
      width: 28px;
      padding: 0;
      justify-content: center;
    }
  }


  /* ---- The island (the main buttons in the top bar) ---- */
  .ib {
    height: 30px;
    padding: 0 11px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 0;
    border-radius: 9px;
    background: none;
    color: var(--t-fg);
    font-size: 12.5px;
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color 0.15s ease,
      transform 0.16s cubic-bezier(0.23, 1, 0.32, 1);
  }
  .ib.sq {
    width: 30px;
    padding: 0;
    justify-content: center;
  }
  .ib:hover:not(:disabled) {
    background: var(--t-line);
  }
  .ib:active:not(:disabled) {
    transform: scale(0.95);
  }
  .ib:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .ib svg {
    width: 15px;
    height: 15px;
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .counter {
    min-width: 66px;
    padding: 0 6px;
    display: inline-flex;
    align-items: baseline;
    justify-content: center;
    gap: 4px;
    color: var(--t-muted);
    font-size: 12.5px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .counter b {
    color: var(--t-strong);
    font-weight: 600;
    font-size: 13.5px;
  }
  .counter.same {
    color: var(--ok);
    padding: 0 10px;
  }
  .div {
    width: 1px;
    height: 18px;
    margin: 0 5px;
    background: var(--t-line2);
  }
  .ig {
    display: inline-flex;
    gap: 2px;
  }
  .chip {
    height: 26px;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--t-muted);
    font-size: 12.5px;
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color 0.18s ease,
      color 0.18s ease,
      transform 0.16s cubic-bezier(0.23, 1, 0.32, 1);
  }
  .chip:hover {
    color: var(--t-strong);
  }
  .chip:active {
    transform: scale(0.95);
  }
  .chip.on {
    background: var(--t-acc-bg);
    color: var(--t-acc);
  }
  .chip .dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    margin-right: 6px;
    border-radius: 50%;
    background: currentColor;
    opacity: 0.35;
    transform: scale(0.7);
    transition:
      opacity 0.18s ease,
      transform 0.25s cubic-bezier(0.23, 1, 0.32, 1);
  }
  .chip.on .dot {
    opacity: 1;
    transform: none;
  }
  .chip .short {
    display: none;
  }
  .rw {
    position: relative;
    display: inline-flex;
  }
  .iglbl {
    align-self: center;
    padding: 0 4px 0 2px;
    color: var(--t-faint);
    font-size: 11.5px;
  }
  /* Share: a menu under the top bar's Share button. */
  .share {
    position: relative;
    display: inline-flex;
  }
  .sharebtn {
    display: inline-grid;
    place-items: center;
    width: 30px;
    height: 28px;
    padding: 0;
    margin-right: 4px;
    border: 0;
    border-radius: 9px;
    background: none;
    color: var(--t-fg);
    font-size: 12.5px;
    cursor: pointer;
  }
  .sharebtn:hover:not(:disabled),
  .share.open .sharebtn {
    background: var(--t-line);
    color: var(--t-strong);
  }
  .sharebtn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .sharebtn svg {
    width: 15px;
    height: 15px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .sharemenu {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    z-index: 30;
    width: 300px;
    padding: 5px;
    border-radius: 12px;
    background: var(--t-card2);
    border: 1px solid var(--t-line2);
    box-shadow: 0 24px 60px -18px rgba(0, 0, 0, 0.55);
    transform-origin: calc(100% - 30px) 0;
    animation: menu-in 0.16s cubic-bezier(0.23, 1, 0.32, 1);
  }
  @keyframes menu-in {
    from {
      opacity: 0;
      transform: scale(0.96) translateY(-4px);
    }
  }
  .sharemenu button {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: 8px;
    background: none;
    text-align: left;
    color: var(--t-strong);
    cursor: pointer;
  }
  .sharemenu button:hover {
    background: var(--t-line);
  }
  .sharemenu b {
    font-size: 12.5px;
    font-weight: 600;
  }
  .sharemenu small {
    font-size: 11.5px;
    line-height: 1.4;
    color: var(--t-muted);
  }
  /* A short message near the bottom; it goes away by itself. */
  .toast {
    position: fixed;
    left: 50%;
    bottom: 40px;
    z-index: 50;
    display: flex;
    align-items: center;
    gap: 12px;
    max-width: min(560px, calc(100vw - 32px));
    padding: 8px 8px 8px 16px;
    border-radius: 12px;
    background: var(--t-strong);
    color: var(--t-bg);
    font-size: 12.5px;
    box-shadow: 0 16px 40px -12px rgba(0, 0, 0, 0.5);
    transform: translateX(-50%);
    animation: toast-in 0.24s cubic-bezier(0.23, 1, 0.32, 1);
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translate(-50%, 8px);
    }
  }
  .toast button {
    flex: none;
    height: 26px;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: var(--t-acc);
    color: var(--t-on-acc);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  @media (prefers-reduced-motion: reduce) {
    .sharemenu,
    .toast {
      animation: none;
    }
  }
  .install {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    margin-right: 4px;
    border: 0;
    border-radius: 9px;
    background: var(--t-strong);
    color: var(--t-bg);
    font-weight: 600;
    font-size: 12.5px;
    white-space: nowrap;
    cursor: pointer;
    transition: transform 0.16s cubic-bezier(0.23, 1, 0.32, 1);
  }
  .install:active {
    transform: scale(0.96);
  }
  .install svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  /* Narrower window: the island keeps its buttons but drops their words. */
  @container bar (max-width: 1140px) {
    .ib .lbl,
    .chip .long,
    .iglbl {
      display: none;
    }
    .ib:not(.sq) {
      width: 30px;
      padding: 0;
      justify-content: center;
    }
    .chip .short {
      display: inline;
    }
  }
  @container bar (max-width: 1240px) {
    .install .lbl {
      display: none;
    }
    .install {
      width: 30px;
      padding: 0;
      justify-content: center;
    }
  }
  @container bar (max-width: 920px) {
    .chip .dot {
      display: none;
    }
    .chip {
      padding: 0 7px;
    }
    .div {
      margin: 0 2px;
    }
  }
  .grow {
    flex: 1;
  }

  /* A slim message above the files (file changed on disk, tidy up JSON / XML). */
  .note {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 4px 14px 2px;
    padding: 6px 8px 6px 14px;
    border-radius: 12px;
    font-size: 12.5px;
    color: var(--t-strong);
    background: var(--t-acc-bg);
    border: 1px solid color-mix(in srgb, var(--t-acc) 30%, transparent);
    animation: note-in 0.22s cubic-bezier(0.23, 1, 0.32, 1);
  }
  .note span {
    flex: 1;
  }
  @keyframes note-in {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }
  .note button {
    flex: none;
    font: inherit;
    height: 26px;
    color: var(--t-acc);
    background: var(--t-card);
    border: 1px solid color-mix(in srgb, var(--t-acc) 35%, transparent);
    border-radius: 8px;
    padding: 0 10px;
    cursor: pointer;
  }
  .note button:active {
    transform: scale(0.97);
  }
  .note .go {
    color: var(--t-on-acc);
    background: var(--t-acc);
    border-color: transparent;
    font-weight: 600;
  }
  .note .x {
    width: 26px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--t-muted);
    font-size: 16px;
  }
  @media (prefers-reduced-motion: reduce) {
    .note {
      animation: none;
    }
  }
  .stage {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    position: relative;
  }
  .stage.cmp {
    padding: 4px 14px 0;
  }

  /* The file cards' headers (the cards themselves are drawn by DiffView) */
  .heads {
    flex: none;
    display: grid;
    grid-template-columns: 1fr var(--gw, 40px) 1fr;
    margin-right: 22px; /* the difference map (14px) and its gap (8px) */
  }
  .fh {
    grid-row: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    background: var(--t-card2);
    border: 1px solid var(--t-line);
    border-bottom: 0;
    border-radius: 14px 14px 0 0;
    transition: background-color 0.15s ease;
  }
  .fh.drop {
    background: var(--t-acc-bg);
  }
  .fh .ic {
    width: 28px;
    height: 28px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 8px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  .fh.old .ic {
    background: var(--t-del);
    color: var(--t-del-s);
  }
  .fh.new .ic {
    background: var(--t-add);
    color: var(--t-add-s);
  }
  .fh .t {
    flex: 1;
    min-width: 0;
  }
  .fh b,
  .fh small {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fh b {
    color: var(--t-strong);
    font-weight: 600;
    font-size: 13px;
  }
  .fh small {
    color: var(--t-muted);
    font-size: 11.5px;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
  .change {
    flex: none;
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
    background: none;
    border: 1px solid var(--t-line2);
    border-radius: 8px;
    color: var(--t-muted);
    cursor: pointer;
    transition:
      color 0.15s ease,
      transform 0.16s cubic-bezier(0.23, 1, 0.32, 1);
  }
  .change:hover {
    color: var(--t-strong);
  }
  .change.on {
    color: var(--t-acc);
    border-color: color-mix(in srgb, var(--t-acc) 45%, transparent);
    background: var(--t-acc-bg);
  }
  .lw {
    position: relative;
    display: inline-flex;
  }
  .change:active {
    transform: scale(0.96);
  }
  .swapdot {
    grid-row: 1;
    grid-column: 2;
    display: grid;
    place-items: center;
  }
  .swapdot button {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    padding: 0;
    border-radius: 50%;
    border: 1px solid var(--t-line2);
    background: var(--t-card);
    color: var(--t-muted);
    cursor: pointer;
    transition:
      transform 0.35s cubic-bezier(0.23, 1, 0.32, 1),
      color 0.15s ease;
  }
  .swapdot button:hover {
    color: var(--t-acc);
  }
  .swapdot button.spun {
    transform: rotate(180deg);
  }
  .swapdot svg,
  .lb svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.9;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  /* Start screen */
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 22px;
    padding: 32px 24px;
    overflow-y: auto;
  }
  /* Centre the content when it fits, scroll from the top when it doesn't. */
  .empty > :first-child {
    margin-top: auto;
  }
  .empty > :last-child {
    margin-bottom: auto;
  }
  .hello {
    text-align: center;
  }
  .hello h2 {
    margin: 0;
    font-size: 28px;
    font-weight: 650;
    letter-spacing: -0.03em;
    color: var(--t-strong);
    text-wrap: balance;
  }
  .hello p {
    margin: 6px auto 0;
    max-width: 52ch;
    color: var(--t-muted);
    font-size: 13.5px;
    text-wrap: pretty;
  }
  .slots {
    display: grid;
    grid-template-columns: 1fr 44px 1fr;
    align-items: center;
    width: min(680px, 100%);
  }
  .vs {
    text-align: center;
    color: var(--t-faint);
    font-size: 12px;
    font-weight: 600;
  }
  .slot {
    position: relative;
    min-width: 0;
    height: 170px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 0 20px;
    border-radius: 18px;
    background: var(--t-card);
    border: 1px solid var(--t-line);
    color: var(--t-fg);
    font: inherit;
    cursor: pointer;
    transition:
      transform 0.2s cubic-bezier(0.23, 1, 0.32, 1),
      border-color 0.2s ease,
      background-color 0.2s ease;
  }
  .slot:hover {
    background: var(--t-card2);
    border-color: var(--t-line2);
  }
  .slot:active {
    transform: scale(0.985);
  }
  .slot.drop {
    background: var(--t-acc-bg);
    border-color: var(--t-acc);
  }
  .slot.filled {
    border-color: color-mix(in srgb, var(--t-acc) 45%, transparent);
  }
  /* The dashed edge inside the card; it moves while you point at it or drag a file over it. */
  .ant {
    position: absolute;
    inset: 8px;
    width: calc(100% - 16px);
    height: calc(100% - 16px);
    pointer-events: none;
    overflow: visible;
  }
  .ant rect {
    width: calc(100% - 2px);
    height: calc(100% - 2px);
    fill: none;
    stroke: var(--t-faint);
    stroke-width: 1.4;
    stroke-dasharray: 6 6;
    opacity: 0.7;
    animation: march 1s linear infinite paused;
    transition: stroke 0.2s ease;
  }
  .slot:hover .ant rect,
  .slot.drop .ant rect {
    stroke: var(--t-acc);
    animation-play-state: running;
  }
  .slot.filled .ant {
    opacity: 0;
  }
  @keyframes march {
    to {
      stroke-dashoffset: -12;
    }
  }
  .big {
    width: 44px;
    height: 44px;
    display: grid;
    place-items: center;
    border-radius: 12px;
    font-size: 12px;
    font-weight: 700;
  }
  .slot.old .big {
    background: var(--t-del);
    color: var(--t-del-s);
  }
  .slot.new .big {
    background: var(--t-add);
    color: var(--t-add-s);
  }
  .slot b,
  .slot small {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .slot b {
    color: var(--t-strong);
    font-size: 14px;
    font-weight: 600;
  }
  .slot small {
    color: var(--t-muted);
    font-size: 12px;
  }
  .orp {
    margin: -6px 0 0;
    color: var(--t-muted);
    font-size: 13px;
  }
  .link {
    font: inherit;
    background: none;
    border: 0;
    padding: 0;
    color: var(--t-acc);
    border-bottom: 1px solid color-mix(in srgb, var(--t-acc) 40%, transparent);
    cursor: pointer;
  }
  .link:hover {
    border-bottom-color: var(--t-acc);
  }
  .error {
    margin: 0;
    font-size: 13px;
    color: var(--error);
    text-align: center;
    max-width: 640px;
  }
  .recent-home {
    width: min(680px, 100%);
  }
  .recent-home h2 {
    margin: 0 0 8px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--t-faint);
  }
  /* The "reopen at start" switch under Recent. */
  .reopen {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    border: 0;
    background: none;
    padding: 4px 6px;
    border-radius: 8px;
    color: var(--t-muted);
    font-size: 12.5px;
    cursor: pointer;
    transition: color 0.15s ease;
  }
  .reopen:hover,
  .reopen[aria-checked='true'] {
    color: var(--t-fg);
  }
  .reopen .knob {
    position: relative;
    width: 28px;
    height: 16px;
    flex: none;
    border-radius: 999px;
    background: var(--t-line2);
    transition: background-color 0.18s ease;
  }
  .reopen .knob::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--t-card);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
    transition: transform 0.18s cubic-bezier(0.23, 1, 0.32, 1);
  }
  .reopen[aria-checked='true'] .knob {
    background: var(--t-acc);
  }
  .reopen[aria-checked='true'] .knob::after {
    transform: translateX(12px);
  }
  @media (prefers-reduced-motion: reduce) {
    .reopen .knob,
    .reopen .knob::after {
      transition: none;
    }
  }
  /* Opening the start screen: its parts rise in one after another. */
  .rise {
    animation: rise 0.45s cubic-bezier(0.23, 1, 0.32, 1) both;
    animation-delay: calc(var(--i, 0) * 55ms);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(10px);
    }
  }

  /* Comparing (shown only if it takes more than a moment) */
  .busy {
    position: absolute;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--t-bg) 55%, transparent);
    backdrop-filter: blur(3px);
    animation: fade 0.2s ease both;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  .busy-card {
    width: min(420px, calc(100% - 32px));
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    padding: 24px;
    border-radius: 18px;
    background: var(--t-card2);
    border: 1px solid var(--t-line2);
    box-shadow: 0 24px 60px -18px rgba(0, 0, 0, 0.5);
    animation: rise 0.35s cubic-bezier(0.23, 1, 0.32, 1) both;
  }
  .scan {
    display: flex;
    gap: 12px;
  }
  /* Two little pages with a light passing over them. */
  .sheet {
    position: relative;
    width: 120px;
    height: 84px;
    overflow: hidden;
    padding: 12px;
    border-radius: 10px;
    background: var(--t-card);
    border: 1px solid var(--t-line);
  }
  .sheet i {
    display: block;
    height: 5px;
    margin-bottom: 7px;
    border-radius: 3px;
    background: var(--t-line2);
  }
  .sheet::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: -36px;
    height: 36px;
    background: linear-gradient(180deg, transparent, var(--t-acc-bg) 70%, color-mix(in srgb, var(--t-acc) 70%, transparent));
    animation: scan 1.1s cubic-bezier(0.77, 0, 0.175, 1) infinite;
  }
  .sheet.b::after {
    animation-delay: 0.18s;
    background: linear-gradient(
      180deg,
      transparent,
      color-mix(in srgb, var(--t-acc2) 12%, transparent) 70%,
      color-mix(in srgb, var(--t-acc2) 70%, transparent)
    );
  }
  @keyframes scan {
    to {
      transform: translateY(120px);
    }
  }
  .pbar {
    width: 100%;
    height: 6px;
    border-radius: 3px;
    background: var(--t-line);
    overflow: hidden;
  }
  .pbar i {
    display: block;
    height: 100%;
    transform-origin: 0 50%;
    background: linear-gradient(90deg, var(--t-acc), var(--t-acc2));
    transition: transform 0.12s linear;
  }
  .busytxt {
    width: 100%;
    margin-top: -8px;
    display: flex;
    justify-content: space-between;
    color: var(--t-muted);
    font-size: 12.5px;
    font-variant-numeric: tabular-nums;
  }
  .busytxt b {
    color: var(--t-strong);
    font-weight: 600;
  }
  .cancel {
    font: inherit;
    font-size: 12.5px;
    height: 32px;
    padding: 0 18px;
    background: none;
    border: 1px solid var(--t-line2);
    border-radius: 10px;
    color: var(--t-fg);
    cursor: pointer;
    transition:
      background-color 0.15s ease,
      transform 0.16s cubic-bezier(0.23, 1, 0.32, 1);
  }
  .cancel:hover {
    background: var(--t-line);
  }
  .cancel:active {
    transform: scale(0.96);
  }
  @media (max-width: 620px) {
    .slots {
      grid-template-columns: 1fr;
      gap: 10px;
    }
    .vs {
      display: none;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .rise,
    .busy,
    .busy-card,
    .sheet::after,
    .ant rect {
      animation: none;
    }
  }

  /* Status bar */
  .status {
    height: 34px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 0 16px;
    background: var(--t-bg);
    border-top: 1px solid var(--t-line);
    color: var(--t-muted);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
  }
  .status b {
    color: var(--t-strong);
    font-weight: 600;
  }
  .status .same {
    color: var(--ok);
  }
  /* A small bar: how much was changed, added and removed. */
  .stack {
    display: flex;
    gap: 2px;
    width: 110px;
    height: 6px;
    flex: none;
    border-radius: 3px;
    overflow: hidden;
  }
  .stack i {
    min-width: 3px;
    transform-origin: 0 50%;
    animation: grow-x 0.6s cubic-bezier(0.23, 1, 0.32, 1) both;
  }
  .stack i:nth-child(2) {
    animation-delay: 0.08s;
  }
  .stack i:nth-child(3) {
    animation-delay: 0.16s;
  }
  @keyframes grow-x {
    from {
      transform: scaleX(0);
    }
  }
  .stack .c,
  .lg.c::before {
    background: var(--t-chg-s);
  }
  .stack .a,
  .lg.a::before {
    background: var(--t-add-s);
  }
  .stack .d,
  .lg.d::before {
    background: var(--t-del-s);
  }
  .stack .m,
  .lg.m::before {
    background: var(--t-sec);
  }
  .lg {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .lg::before {
    content: '';
    width: 7px;
    height: 7px;
    border-radius: 2px;
  }
  .zoom {
    font: inherit;
    color: inherit;
    background: none;
    border: 0;
    padding: 2px 6px;
    cursor: pointer;
    border-radius: 6px;
  }
  .zoom:hover {
    background: var(--t-line);
    color: var(--t-strong);
  }
  /* Live reload switch: a dot that breathes slowly while it is on. */
  .live {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font: inherit;
    color: inherit;
    background: none;
    border: 1px solid var(--t-line2);
    padding: 1px 8px 1px 7px;
    border-radius: 999px;
    cursor: pointer;
    transition:
      background-color 0.15s ease,
      color 0.15s ease,
      border-color 0.15s ease;
  }
  .live:hover {
    color: var(--t-strong);
    background: var(--t-line);
  }
  .live:active {
    transform: scale(0.97);
  }
  .views {
    display: inline-flex;
    gap: 1px;
    padding: 1px;
    border-radius: 999px;
    border: 1px solid var(--t-line2);
  }
  .views button {
    display: grid;
    place-items: center;
    width: 26px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: none;
    color: var(--t-muted);
    cursor: pointer;
  }
  .views button:hover {
    color: var(--t-strong);
  }
  .views button[aria-pressed='true'] {
    background: var(--t-acc-bg);
    color: var(--t-acc);
  }
  .views svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
  }
  .live.fold svg {
    width: 12px;
    height: 12px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .live.fold.on {
    color: var(--t-acc);
    border-color: color-mix(in srgb, var(--t-acc) 45%, transparent);
    background: var(--t-acc-bg);
  }
  .live .pulse {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--t-faint);
  }
  .live.on {
    color: var(--t-add-s);
    border-color: color-mix(in srgb, var(--t-add-s) 45%, transparent);
    background: var(--t-add);
  }
  .live.on .pulse {
    background: var(--t-add-s);
    animation: breathe 2.4s ease-in-out infinite;
  }
  @keyframes breathe {
    50% {
      opacity: 0.35;
    }
  }
  .live-note {
    font-variant-numeric: tabular-nums;
  }
  .live-note.warn {
    color: var(--t-chg-s);
  }
  @media (prefers-reduced-motion: reduce) {
    .live.on .pulse {
      animation: none;
    }
  }
  .new-version {
    font: inherit;
    font-weight: 600;
    color: var(--t-on-acc);
    background: var(--t-acc);
    border: 0;
    border-radius: 999px;
    padding: 2px 10px;
    cursor: pointer;
  }
  @media (max-width: 900px) {
    .lg {
      display: none;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .stack i {
      animation: none;
    }
  }
  /* The smallest windows: everything a little tighter, so the window buttons always fit. */
  @container bar (max-width: 840px) {
    .chip {
      padding: 0 5px;
      font-size: 12px;
    }
    .ib.sq,
    .ib:not(.sq) {
      width: 26px;
    }
    .lb {
      width: 24px;
    }
    .counter {
      padding: 0 2px;
    }
    .iglbl,
    .div {
      display: none;
    }
  }
  /* Narrow windows: the status bar keeps the switches and drops the less useful text. */
  @media (max-width: 1060px) {
    .status .ver {
      display: none;
    }
  }
  @media (max-width: 900px) {
    .status .took,
    .status .lg {
      display: none;
    }
  }
</style>
