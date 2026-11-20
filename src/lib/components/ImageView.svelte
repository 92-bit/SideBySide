<script lang="ts">
  import { t } from '#lib/i18n.svelte.ts';
  // Two pictures: a slider to wipe between them, side by side, or the changed pixels in red.
  import { fileName } from '#lib/recent.ts';
  import { fmtNum, fmtSize, imageData } from '#lib/api.ts';

  let { left, right }: { left: string; right: string } = $props();

  type Mode = 'slider' | 'side' | 'diff';
  let mode = $state<Mode>('slider');
  let srcA = $state('');
  let srcB = $state('');
  let error = $state('');
  let sizes = $state<{ a: [number, number]; b: [number, number] } | null>(null);
  let bytes = $state<[number, number]>([0, 0]);
  /** Share of pixels that differ (0..1), once worked out. */
  let changed = $state<number | null>(null);
  let diffUrl = $state('');
  let split = $state(50);

  $effect(() => {
    const [l, r] = [left, right];
    error = '';
    srcA = srcB = diffUrl = '';
    changed = null;
    sizes = null;
    Promise.all([imageData(l), imageData(r)])
      .then(async ([a, b]) => {
        srcA = a;
        srcB = b;
        bytes = [Math.round(((a.length - a.indexOf(',') - 1) * 3) / 4), Math.round(((b.length - b.indexOf(',') - 1) * 3) / 4)];
        await measure(a, b);
      })
      .catch((e) => (error = String(e)));
  });

  const load = (src: string) =>
    new Promise<HTMLImageElement>((ok, bad) => {
      const im = new Image();
      im.onload = () => ok(im);
      im.onerror = () => bad(new Error("This picture couldn't be opened (the file may be damaged or of an unknown kind)."));
      im.src = src;
    });

  /** Sizes, and the changed pixels (compared at the larger of the two sizes). */
  async function measure(a: string, b: string) {
    const [ia, ib] = await Promise.all([load(a), load(b)]);
    sizes = { a: [ia.naturalWidth, ia.naturalHeight], b: [ib.naturalWidth, ib.naturalHeight] };
    const w = Math.max(ia.naturalWidth, ib.naturalWidth);
    const h = Math.max(ia.naturalHeight, ib.naturalHeight);
    // (very big pictures are compared at a smaller size, so this stays quick)
    const scale = Math.min(1, Math.sqrt(16_000_000 / Math.max(1, w * h)));
    const [cw, ch] = [Math.max(1, Math.round(w * scale)), Math.max(1, Math.round(h * scale))];
    const draw = (im: HTMLImageElement) => {
      const c = new OffscreenCanvas(cw, ch);
      const g = c.getContext('2d', { willReadFrequently: true })!;
      g.drawImage(im, 0, 0, im.naturalWidth * scale, im.naturalHeight * scale);
      return g.getImageData(0, 0, cw, ch);
    };
    const [da, db] = [draw(ia), draw(ib)];
    const out = new ImageData(cw, ch);
    let n = 0;
    for (let i = 0; i < da.data.length; i += 4) {
      const d =
        Math.abs(da.data[i] - db.data[i]) + Math.abs(da.data[i + 1] - db.data[i + 1]) + Math.abs(da.data[i + 2] - db.data[i + 2]) + Math.abs(da.data[i + 3] - db.data[i + 3]);
      if (d > 24) {
        n++;
        out.data.set([255, 40, 70, 255], i);
      } else {
        // unchanged: a faint grey copy of the new picture
        const grey = (db.data[i] + db.data[i + 1] + db.data[i + 2]) / 3;
        out.data.set([grey, grey, grey, 60], i);
      }
    }
    changed = n / (cw * ch);
    const c = new OffscreenCanvas(cw, ch);
    c.getContext('2d')!.putImageData(out, 0, 0);
    diffUrl = URL.createObjectURL(await c.convertToBlob());
  }

  // Dragging the slider
  let stage = $state<HTMLDivElement>();
  let dragging = false;
  function at(e: PointerEvent) {
    const r = stage!.getBoundingClientRect();
    split = Math.max(0, Math.min(100, ((e.clientX - r.left) / r.width) * 100));
  }
  function down(e: PointerEvent) {
    dragging = true;
    stage!.setPointerCapture(e.pointerId);
    at(e);
  }
  function move(e: PointerEvent) {
    if (dragging) at(e);
  }
  function up() {
    dragging = false;
  }
  function key(e: KeyboardEvent) {
    const step = e.shiftKey ? 10 : 2;
    if (e.key === 'ArrowLeft') split = Math.max(0, split - step);
    else if (e.key === 'ArrowRight') split = Math.min(100, split + step);
    else return;
    e.preventDefault();
  }
  const dims = (s: [number, number]) => `${fmtNum(s[0])} × ${fmtNum(s[1])}`;
</script>

<section class="images">
  <header>
    <div class="side">
      <span class="tag old">{t("OLD")}</span><b title={left}>{fileName(left)}</b>
      <small>{sizes ? dims(sizes.a) : ''}{bytes[0] ? ` · ${fmtSize(bytes[0])}` : ''}</small>
    </div>
    <div class="modes" role="group" aria-label={t("How to compare")}>
      <button type="button" aria-pressed={mode === 'slider'} onclick={() => (mode = 'slider')}>{t("Slider")}</button>
      <button type="button" aria-pressed={mode === 'side'} onclick={() => (mode = 'side')}>{t("Side by side")}</button>
      <button type="button" aria-pressed={mode === 'diff'} onclick={() => (mode = 'diff')}>{t("Difference")}</button>
    </div>
    <div class="side right">
      <small>{sizes ? dims(sizes.b) : ''}{bytes[1] ? ` · ${fmtSize(bytes[1])}` : ''}</small>
      <b title={right}>{fileName(right)}</b><span class="tag new">{t("NEW")}</span>
    </div>
  </header>

  {#if error}
    <p class="msg bad">{error}</p>
  {:else if !srcA || !srcB}
    <p class="msg">{t("Opening the pictures…")}</p>
  {:else if mode === 'side'}
    <div class="pair">
      <div class="frame checker"><img src={srcA} alt={t("Original")} /></div>
      <div class="frame checker"><img src={srcB} alt={t("New version")} /></div>
    </div>
  {:else if mode === 'diff'}
    <div class="frame big checker">
      {#if diffUrl}<img src={diffUrl} alt={t("Changed pixels in red")} />{:else}<p class="msg">{t("Finding the changed pixels…")}</p>{/if}
    </div>
  {:else}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="frame big checker slider"
      bind:this={stage}
      role="slider"
      tabindex="0"
      aria-label={t("Drag to compare: left shows the original, right the new version")}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(split)}
      onpointerdown={down}
      onpointermove={move}
      onpointerup={up}
      onkeydown={key}
    >
      <img src={srcB} alt={t("New version")} />
      <img src={srcA} alt={t("Original")} class="top" style:clip-path="inset(0 {100 - split}% 0 0)" />
      <div class="handle" style:left="{split}%"><span>⟷</span></div>
    </div>
  {/if}

  <footer>
    {#if changed !== null}
      {#if changed === 0}
        <b class="same">{t("✓ The pictures are the same, pixel for pixel.")}</b>
      {:else}
        <b>{t('{share} of the pixels differ.', { share: changed < 0.0001 ? t('Less than 0.01%') : `${(changed * 100).toFixed(changed < 0.01 ? 2 : 1)}%` })}</b>
      {/if}
      {#if sizes && (sizes.a[0] !== sizes.b[0] || sizes.a[1] !== sizes.b[1])}
        <span>{t('The sizes differ ({a} and {b}).', { a: dims(sizes.a), b: dims(sizes.b) })}</span>
      {/if}
    {/if}
    {#if mode === 'slider'}<span class="hint">{t("Drag across the picture (or use ← →) to move between the two.")}</span>{/if}
  </footer>
</section>

<style>
  .images {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    margin: 4px 14px 12px;
    border-radius: 14px;
    background: var(--t-card);
    border: 1px solid var(--t-line);
    overflow: hidden;
  }
  header {
    flex: none;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--t-line);
  }
  .side {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    color: var(--t-strong);
  }
  .side.right {
    justify-content: flex-end;
  }
  .side b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .side small {
    color: var(--t-muted);
    white-space: nowrap;
  }
  .modes {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border-radius: 9px;
    background: var(--t-line);
  }
  .modes button {
    border: 0;
    background: none;
    height: 26px;
    padding: 0 12px;
    border-radius: 7px;
    font: inherit;
    font-size: 12.5px;
    color: var(--t-muted);
    cursor: pointer;
  }
  .modes button[aria-pressed='true'] {
    background: var(--t-card);
    color: var(--t-strong);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
  }
  .msg {
    margin: 40px auto;
    color: var(--t-muted);
  }
  .msg.bad {
    color: var(--t-del-s);
  }
  .pair {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    padding: 12px;
  }
  .frame {
    position: relative;
    min-height: 0;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 10px;
  }
  .frame.big {
    flex: 1;
    margin: 12px;
  }
  /* A checkerboard shows where a picture is see-through. */
  .checker {
    background-color: var(--t-card2);
    background-image: conic-gradient(var(--t-line) 25%, transparent 0 50%, var(--t-line) 0 75%, transparent 0);
    background-size: 16px 16px;
  }
  img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .slider {
    cursor: ew-resize;
    touch-action: none;
    user-select: none;
  }
  .slider img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }
  .slider:focus-visible {
    outline: 2px solid var(--t-acc);
  }
  .handle {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--t-acc);
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.6);
    pointer-events: none;
  }
  .handle span {
    position: absolute;
    top: 50%;
    left: 50%;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: var(--t-acc);
    color: var(--t-on-acc);
    font-size: 14px;
    transform: translate(-50%, -50%);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }
  footer {
    flex: none;
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
    padding: 8px 14px 10px;
    font-size: 12.5px;
    color: var(--t-muted);
    border-top: 1px solid var(--t-line);
  }
  footer b {
    color: var(--t-strong);
  }
  footer .same {
    color: var(--t-add-s);
  }
  .hint {
    margin-left: auto;
  }
</style>
