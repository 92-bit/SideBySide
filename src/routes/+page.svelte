<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import DiffView from '$lib/components/DiffView.svelte';
  import FindBar from '$lib/components/FindBar.svelte';
  import RecentList from '$lib/components/RecentList.svelte';
  import PastePanel from '$lib/components/PastePanel.svelte';
  import InstallDialog from '$lib/components/InstallDialog.svelte';
  import UpdateDialog from '$lib/components/UpdateDialog.svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { getVersion } from '@tauri-apps/api/app';
  import { listen } from '@tauri-apps/api/event';
  import {
    CANCELLED,
    STALE,
    cancel,
    closeCompare,
    compare,
    find,
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
  } from '$lib/api';
  import { addRecent, clearRecent, fileName, loadRecent, removeRecent, type Recent } from '$lib/recent';
  import {
    FONT_DEFAULT,
    FONT_MAX,
    FONT_MIN,
    loadAutoUpdate,
    loadFontSize,
    markUpdateChecked,
    saveFontSize,
    updateCheckDue,
  } from '$lib/settings';
  import { currentTheme, setTheme, type Theme } from '$lib/theme';

  type SideName = 'left' | 'right';

  let theme = $state<Theme>('dark');
  let paths = $state<{ left: string | null; right: string | null }>({ left: null, right: null });
  let summary = $state<Summary | null>(null);
  let busy = $state(false);
  let showBusy = $state(false);
  let pct = $state(0);
  let error = $state<string | null>(null);
  let current = $state<Hit | null>(null);
  let dropSide = $state<SideName | null>(null);
  let ignore = $state({ spaces: false, blank: false, caseLetters: false });
  /** The ignore options of the result on screen, so Cancel can put the buttons back. */
  let shownIgnore = { spaces: false, blank: false, caseLetters: false };
  let findBar = $state<ReturnType<typeof FindBar>>();
  let view = $state<ReturnType<typeof DiffView>>();
  let recent = $state<Recent[]>([]);
  let recentOpen = $state(false);
  let pasteMode = $state(false);
  let findMode = $state<'find' | 'goto' | null>(null);
  let query = $state('');
  let matchCase = $state(false);
  let lastFound: number | null = null;
  let fontSize = $state(FONT_DEFAULT);
  let version = $state('');
  let install = $state<InstallInfo | null>(null);
  let installOpen = $state(false);
  let aboutOpen = $state(false);
  /** Set when the daily check found a newer version. */
  let newVersion = $state<UpdateInfo | null>(null);

  function onHelp(action: 'check' | 'about' | 'site') {
    if (action === 'site') openUrl(PROJECT_URL);
    else aboutOpen = true;
  }
  // Show the Install button only for the portable copy on Windows.
  const canInstall = $derived(!!install?.supported && !install.runningInstalled);

  const isPasted = (p: string | null) => !!p && /[\\/]SideBySide[\\/]Pasted text \(/.test(p);

  function toggleTheme() {
    theme = theme === 'dark' ? 'light' : 'dark';
    setTheme(theme);
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
  async function run() {
    const { left, right } = paths;
    if (!left || !right) return;
    const id = ++runId;
    const used = { ...ignore };
    busy = true;
    error = null;
    startProgress();
    try {
      const s = await compare(left, right, {
        ignoreSpaces: used.spaces,
        ignoreCase: used.caseLetters,
        ignoreBlankLines: used.blank,
      });
      if (id === runId) {
        summary = s;
        shownIgnore = used;
        current = null;
        lastFound = null;
        pasteMode = false;
        if (!isPasted(left)) recent = addRecent(left, right);
      }
    } catch (e) {
      const msg = String(e);
      if (id === runId && msg !== STALE) {
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
        stopProgress();
      }
    }
  }

  function setPath(side: SideName, p: string) {
    paths[side] = p;
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
    run();
  }

  function closeAll() {
    runId++;
    busy = false;
    stopProgress();
    summary = null;
    current = null;
    error = null;
    findMode = null;
    paths = { left: null, right: null };
    closeCompare();
  }

  function toggle(k: 'spaces' | 'blank' | 'caseLetters') {
    ignore[k] = !ignore[k];
    run();
  }

  function openRecent(r: Recent) {
    recentOpen = false;
    pasteMode = false;
    paths = { left: r.left, right: r.right };
    run();
  }

  async function comparePasted(a: string, b: string) {
    try {
      const [l, r] = await savePasted(a, b);
      paths = { left: l, right: r };
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
      const row = await find(query, from, forward, matchCase);
      if (row === null) return false;
      lastFound = row;
      view.reveal(row);
      return true;
    } catch {
      return false;
    }
  }

  async function doGoto(left: boolean, line: number): Promise<boolean> {
    const row = await rowForLine(left, line);
    if (row === null || !view) return false;
    view.reveal(row);
    return true;
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
    fontSize = step === 0 ? FONT_DEFAULT : Math.max(FONT_MIN, Math.min(FONT_MAX, fontSize + step));
    saveFontSize(fontSize);
  }

  // ---- Keyboard shortcuts ----
  function onKey(e: KeyboardEvent) {
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
    else return;
    e.preventDefault();
  }

  onMount(() => {
    theme = currentTheme();
    recent = loadRecent();
    fontSize = loadFontSize();

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
    unlistenFiles.then(() => takeFiles()).then(receiveFiles);

    // Drag and drop: drop on the left or right half, or drop two files at once.
    const sideAt = (x: number): SideName => (x / window.devicePixelRatio < window.innerWidth / 2 ? 'left' : 'right');
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
    };
  });
</script>

<svelte:window onkeydown={onKey} />

<div class="app">
  <TitleBar {theme} onToggleTheme={toggleTheme} {onHelp} />

  <div class="body">
    <nav class="activity" aria-label="Sections">
      <button type="button" class:on={!recentOpen} title="Compare files" aria-label="Compare files" onclick={() => (recentOpen = false)}>
        <svg viewBox="0 0 24 24" width="20" height="20"><rect x="3" y="4" width="7.5" height="16" rx="1.5" /><rect x="13.5" y="4" width="7.5" height="16" rx="1.5" /></svg>
      </button>
      <button type="button" title="Compare folders (coming later)" aria-label="Compare folders" disabled>
        <svg viewBox="0 0 24 24" width="20" height="20"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>
      </button>
      <button type="button" class:on={recentOpen} title="Recent comparisons" aria-label="Recent comparisons" aria-pressed={recentOpen} onclick={() => (recentOpen = !recentOpen)}>
        <svg viewBox="0 0 24 24" width="20" height="20"><circle cx="12" cy="12" r="8.5" /><path d="M12 7.5V12l3 2" /></svg>
      </button>
      <button type="button" class="bottom" title="Settings (coming soon)" aria-label="Settings" disabled>
        <svg viewBox="0 0 24 24" width="20" height="20"><circle cx="12" cy="12" r="3" /><path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1" /></svg>
      </button>
    </nav>

    {#if recentOpen}
      <aside class="panel" aria-label="Recent comparisons">
        <h2>Recent comparisons</h2>
        <RecentList items={recent} onOpen={openRecent} onRemove={(r) => (recent = removeRecent(r))} onClear={() => (recent = clearRecent())} />
      </aside>
    {/if}

    <main class="main">
      <div class="tabs">
        <span class="tab on">
          {#if summary}
            {summary.left.name} ↔ {summary.right.name}
            <button type="button" class="tab-x" title="Close this comparison" aria-label="Close comparison" onclick={closeAll}>×</button>
          {:else}
            New comparison
          {/if}
        </span>
      </div>

      <div class="tools">
        <button type="button" disabled={!summary || summary.blocks === 0} onclick={() => view?.prev()} title="Previous difference (F7)">↑ Prev</button>
        <button type="button" disabled={!summary || summary.blocks === 0} onclick={() => view?.next()} title="Next difference (F8)">↓ Next</button>
        <span class="vs"></span>
        <button type="button" disabled={!paths.left || !paths.right} onclick={swap} title="Swap left and right">⇄ Swap</button>
        <button type="button" disabled={!summary} onclick={() => (findMode = 'find')} title="Find text (Ctrl+F)">Find</button>
        <button type="button" disabled={!summary} onclick={() => (findMode = 'goto')} title="Go to a line number (Ctrl+G)">Go to line</button>
        <span class="vs"></span>
        <span class="lbl">Ignore</span>
        <button type="button" class="tg" class:on={ignore.spaces} aria-pressed={ignore.spaces} onclick={() => toggle('spaces')} title="Treat lines as equal if they only differ in spaces or tabs">␣ Spaces</button>
        <button type="button" class="tg" class:on={ignore.blank} aria-pressed={ignore.blank} onclick={() => toggle('blank')} title="Don't count empty lines as differences">¶ Blank lines</button>
        <button type="button" class="tg" class:on={ignore.caseLetters} aria-pressed={ignore.caseLetters} onclick={() => toggle('caseLetters')} title="Treat UPPER and lower case letters as the same">Aa Case</button>
        <span class="grow"></span>
        {#if summary && summary.blocks === 0}
          <span class="same-pill">✓ No differences</span>
        {:else if summary && current}
          <span class="counter">Difference {fmtNum(current.index + 1)} of {fmtNum(summary.blocks)}</span>
        {/if}
        {#if canInstall}
          <span class="vs"></span>
          <button type="button" class="install" onclick={() => (installOpen = true)} title="Add SideBySide to your Start Menu">⤓ {install?.installed ? 'Update install' : 'Install'}</button>
        {/if}
      </div>

      {#if findMode && summary}
        <FindBar
          bind:this={findBar}
          mode={findMode}
          bind:query
          bind:matchCase
          maxLeft={summary.left.lines}
          maxRight={summary.right.lines}
          onFind={doFind}
          onGoto={doGoto}
          onClose={() => (findMode = null)}
        />
      {/if}

      <div class="stage">
        {#if summary}
          <div class="paths">
            {#each [['left', summary.left], ['right', summary.right]] as const as [side, info] (side)}
              <div class="pcell" class:drop={dropSide === side}>
                <span class="tag" class:old={side === 'left'} class:new={side === 'right'}>{side === 'left' ? 'Original' : 'New'}</span>
                <span class="pname" title={info.path}>{isPasted(info.path) ? 'Pasted text' : info.path}</span>
                <span class="meta">{fmtSize(info.size)} · {fmtNum(info.lines)} lines</span>
                <button type="button" class="change" onclick={() => choose(side)}>Change…</button>
              </div>
            {/each}
          </div>
          {#key summary}
            <DiffView
              bind:this={view}
              {summary}
              {fontSize}
              query={findMode === 'find' ? query : ''}
              {matchCase}
              onCurrent={(h) => (current = h)}
              onZoom={zoom}
            />
          {/key}
        {:else if pasteMode}
          <PastePanel {error} onCompare={comparePasted} onBack={() => (pasteMode = false)} />
        {:else}
          <section class="empty">
            <div class="slots">
              {#each ['left', 'right'] as const as side (side)}
                <div class="slot" class:drop={dropSide === side} class:filled={!!paths[side]}>
                  <span class="tag" class:old={side === 'left'} class:new={side === 'right'}>{side === 'left' ? 'Original' : 'New'}</span>
                  <svg class="doc" viewBox="0 0 24 24" width="40" height="40"><path d="M6 2.5h8l4.5 4.5v14.5H6z" /><path d="M14 2.5V7h4.5" /></svg>
                  {#if paths[side]}
                    <p class="chosen" title={paths[side]}>{fileName(paths[side] ?? '')}</p>
                  {:else}
                    <p>Drop a file here</p>
                  {/if}
                  <button type="button" class="choose" onclick={() => choose(side)}>{paths[side] ? 'Change…' : 'Choose file…'}</button>
                </div>
              {/each}
            </div>
            {#if error}
              <p class="error" role="alert">{error}</p>
            {/if}
            <p class="hint">
              Tip: drop two files at once to compare them straight away, or
              <button type="button" class="link" onclick={() => (pasteMode = true)}>paste text instead</button>.
            </p>
            {#if recent.length}
              <div class="recent-home">
                <h2>Recent</h2>
                <RecentList items={recent} limit={5} onOpen={openRecent} onRemove={(r) => (recent = removeRecent(r))} />
              </div>
            {/if}
          </section>
        {/if}

        {#if busy && showBusy}
          <div class="busy" role="status">
            <div class="busy-card">
              <span>{pct < 800 ? 'Reading files…' : 'Matching lines…'} <b>{Math.floor(pct / 10)}%</b></span>
              <span class="bar"><span style:width="{pct / 10}%"></span></span>
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

  {#if installOpen && install}
    <InstallDialog info={install} onClose={() => (installOpen = false)} />
  {/if}

  <footer class="status">
    {#if busy}
      <span>Comparing…</span>
    {:else if error}
      <span>Couldn't compare</span>
    {:else if summary}
      <span>
        {#if summary.blocks === 0}
          Files are identical{ignore.spaces || ignore.caseLetters || ignore.blank ? ' (with ignore options)' : ''}
        {:else}
          {fmtNum(summary.blocks)} {summary.blocks === 1 ? 'difference' : 'differences'} · {fmtNum(summary.changed)} changed · {fmtNum(summary.added)} added · {fmtNum(summary.removed)} removed
        {/if}
      </span>
    {:else}
      <span>Ready</span>
    {/if}
    <span class="grow"></span>
    {#if summary && !busy}
      <span>Compared in {(summary.millis / 1000).toFixed(2)} s</span>
    {/if}
    {#if newVersion}
      <button type="button" class="new-version" onclick={() => (aboutOpen = true)}>⬆ Version {newVersion.latest} is available</button>
    {/if}
    <button type="button" class="zoom" onclick={() => zoom(0)} title="Text size (Ctrl + mouse wheel, Ctrl +/-). Click to reset.">
      {Math.round((fontSize / FONT_DEFAULT) * 100)}%
    </button>
    <span>SideBySide {version ? `v${version}` : ''}{install?.runningInstalled ? '' : ' · portable'}</span>
  </footer>
</div>

<style>
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  /* Left icon bar */
  .activity {
    width: 48px;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding-top: 8px;
    background: var(--bg-chrome);
  }
  .activity button {
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    background: none;
    border: 0;
    border-radius: 6px;
    color: var(--faint);
    cursor: pointer;
  }
  .activity button:hover:not(:disabled) {
    color: var(--fg-strong);
  }
  .activity button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .activity button.on {
    color: var(--fg-strong);
    box-shadow: inset 2px 0 0 var(--accent);
    border-radius: 0 6px 6px 0;
  }
  .activity svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .activity .bottom {
    margin-top: auto;
    margin-bottom: 8px;
  }

  /* Recent panel */
  .panel {
    width: 270px;
    flex: none;
    background: var(--bg-chrome);
    border-right: 1px solid var(--border);
    padding: 12px 8px;
    overflow-y: auto;
  }
  .panel h2,
  .recent-home h2 {
    margin: 0 8px 8px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  /* Tabs */
  .tabs {
    height: 34px;
    flex: none;
    display: flex;
    background: var(--bg-chrome);
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 16px;
    font-size: 12px;
    color: var(--muted);
    max-width: 60%;
    white-space: nowrap;
  }
  .tab.on {
    background: var(--bg);
    color: var(--fg-strong);
    box-shadow: inset 0 1px 0 var(--accent);
  }
  .tab-x {
    background: none;
    border: 0;
    color: var(--muted);
    width: 20px;
    height: 20px;
    border-radius: 4px;
    font-size: 15px;
    line-height: 1;
    cursor: pointer;
  }
  .tab-x:hover {
    background: var(--hover);
    color: var(--fg-strong);
  }

  /* Toolbar */
  .tools {
    flex: none;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
    overflow: hidden;
  }
  .tools button {
    font-size: 12px;
    background: none;
    border: 1px solid transparent;
    color: var(--fg);
    padding: 4px 9px;
    border-radius: 5px;
    cursor: pointer;
  }
  .tools button:hover:not(:disabled) {
    background: var(--hover);
  }
  .tools button:disabled {
    color: var(--faint);
    cursor: default;
  }
  .tools .tg {
    border-color: var(--border-2);
  }
  .tools .tg.on {
    background: var(--accent-bg);
    border-color: var(--accent-border);
    color: var(--accent-fg);
  }
  .vs {
    width: 1px;
    height: 18px;
    background: var(--border-2);
    margin: 0 6px;
  }
  .lbl {
    font-size: 12px;
    color: var(--muted);
    margin-right: 2px;
  }
  .tools .install {
    border-color: var(--accent-border);
    color: var(--accent-fg);
    background: var(--accent-bg);
  }
  .counter {
    font-size: 12px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .same-pill {
    font-size: 12px;
    color: #3fb950;
  }
  .grow {
    flex: 1;
  }

  .stage {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    position: relative;
  }

  /* File names above the comparison */
  .paths {
    flex: none;
    display: grid;
    grid-template-columns: 1fr 1fr;
    padding-right: 14px;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
  }
  .pcell {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 10px;
    min-width: 0;
    transition: background 0.15s ease;
  }
  .pcell + .pcell {
    border-left: 1px solid var(--border);
  }
  .pcell.drop {
    background: var(--accent-bg);
  }
  .pname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
    color: var(--fg-strong);
  }
  .meta {
    color: var(--muted);
    white-space: nowrap;
  }
  .change {
    font-size: 11px;
    background: none;
    border: 1px solid var(--border-2);
    border-radius: 4px;
    padding: 1px 8px;
    color: var(--fg);
    cursor: pointer;
  }
  .change:hover {
    background: var(--hover);
  }

  /* Empty start screen */
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 18px;
    padding: 32px;
    overflow-y: auto;
  }
  .slots {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 20px;
    width: min(760px, 100%);
  }
  .slot {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 44px 20px 32px;
    border: 2px dashed var(--border-2);
    border-radius: 12px;
    background: var(--bg-raised);
    transition:
      border-color 0.15s ease,
      background 0.15s ease;
    min-width: 0;
  }
  .slot .tag {
    position: absolute;
    top: 12px;
    left: 12px;
  }
  .slot:hover {
    border-color: var(--accent-border);
  }
  .slot.drop {
    border-color: var(--accent);
    background: var(--accent-bg);
  }
  .slot.filled {
    border-style: solid;
  }
  .slot p {
    margin: 0;
    color: var(--muted);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .slot p.chosen {
    color: var(--fg-strong);
    font-weight: 600;
  }
  .doc {
    fill: none;
    stroke: var(--faint);
    stroke-width: 1.3;
    stroke-linejoin: round;
  }
  .choose {
    background: var(--accent-bg);
    border: 1px solid var(--accent-border);
    color: var(--accent-fg);
    padding: 6px 14px;
    border-radius: 6px;
    cursor: pointer;
  }
  .choose:hover {
    filter: brightness(1.1);
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--faint);
    text-align: center;
  }
  .link {
    font: inherit;
    background: none;
    border: 0;
    padding: 0;
    color: var(--accent);
    cursor: pointer;
    text-decoration: underline;
  }
  .error {
    margin: 0;
    font-size: 13px;
    color: var(--error);
    text-align: center;
    max-width: 640px;
  }
  .recent-home {
    width: min(760px, 100%);
    margin-top: 6px;
  }

  /* Progress box */
  .busy {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--bg) 55%, transparent);
  }
  .busy-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    align-items: center;
    padding: 18px 28px;
    border-radius: 10px;
    background: var(--bg-raised);
    border: 1px solid var(--border-2);
    color: var(--fg-strong);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.25);
    font-variant-numeric: tabular-nums;
  }
  .bar {
    width: 220px;
    height: 6px;
    border-radius: 3px;
    background: var(--border-2);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    border-radius: 3px;
    transition: width 0.12s linear;
  }
  .cancel {
    font: inherit;
    font-size: 12px;
    background: none;
    border: 1px solid var(--border-2);
    color: var(--fg);
    border-radius: 5px;
    padding: 4px 16px;
    cursor: pointer;
  }
  .cancel:hover {
    background: var(--hover);
  }

  /* Status bar */
  .status {
    height: 24px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 12px;
    background: var(--status-bg);
    color: var(--status-fg);
    font-size: 11px;
    white-space: nowrap;
  }
  .zoom {
    font: inherit;
    color: inherit;
    background: none;
    border: 0;
    padding: 0 4px;
    cursor: pointer;
    border-radius: 3px;
  }
  .new-version {
    font: inherit;
    font-weight: 600;
    color: var(--status-bg);
    background: var(--status-fg);
    border: 0;
    border-radius: 999px;
    padding: 1px 10px;
    cursor: pointer;
  }
  .zoom:hover {
    background: rgba(255, 255, 255, 0.15);
  }
</style>
