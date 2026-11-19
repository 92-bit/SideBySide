//! The comparing engine.
//!
//! How it stays light on memory, even with huge files:
//! 1. Files are read in 1 MB pieces, never loaded whole.
//! 2. Every line becomes a 64-bit "fingerprint" (hash). Only the fingerprints are compared.
//! 3. To show text on screen we remember where every 256th line starts, and read
//!    just the lines that are visible, straight from the file.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use imara_diff::{Diff, InternedInput};
use memchr::{memchr, memmem};
use serde::{Deserialize, Serialize};
use similar::{Algorithm, ChangeTag, TextDiff};
use xxhash_rust::xxh3::Xxh3;

/// Remember the start of every Nth line.
const STRIDE: u32 = 256;
/// Longest line (in bytes) kept for display. Longer lines are cut with "…".
const MAX_SHOW: usize = 10_000;
/// Longest line (in bytes) looked at when searching.
const MAX_SEARCH: usize = 1 << 20;
/// Lines longer than this are not word-compared (they are highlighted as a whole).
const WORD_DIFF_MAX: usize = 2_000;
const READ_BUF: usize = 1 << 20;
/// How many lines are matched up at a time.
const WINDOW: usize = 20_000;
/// The furthest the matching looks ahead (in lines) when a big block was added or removed.
const MAX_WINDOW: usize = 1 << 20;
/// How many lines are sampled to check quickly whether a big block was added or removed.
const PROBE: usize = 2_000;
/// A matching stretch at least this long (in lines) is trusted as a place to stop a window.
const SOLID: usize = 16;

/// Returned as the error when the user pressed Cancel.
pub const CANCELLED: &str = "__cancelled__";

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    #[serde(default)]
    pub ignore_spaces: bool,
    #[serde(default)]
    pub ignore_case: bool,
    #[serde(default)]
    pub ignore_blank_lines: bool,
}

/// Shared with the screen while a comparison runs: how far along it is, and the Cancel switch.
#[derive(Default)]
pub struct Control {
    pub cancel: AtomicBool,
    /// 0 to 1000 (reading the files is 0–800, matching lines is 800–1000)
    pub progress: AtomicU32,
}

impl Control {
    fn check(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::Relaxed) { Err(CANCELLED.into()) } else { Ok(()) }
    }
    fn set(&self, p: u64) {
        self.progress.store(p.min(1000) as u32, Ordering::Relaxed);
    }
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | 0x0B | 0x0C)
}

// ---------------------------------------------------------------------------
// One file
// ---------------------------------------------------------------------------

pub struct Side {
    path: PathBuf,
    size: u64,
    lines: u32,
    /// Byte position of line 0, 256, 512, ...
    index: Vec<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SideInfo {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub lines: u32,
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

impl Side {
    fn info(&self) -> SideInfo {
        SideInfo {
            name: file_name(&self.path),
            path: self.path.to_string_lossy().into_owned(),
            size: self.size,
            lines: self.lines,
        }
    }

    /// Open the file, positioned at the start of line `line`.
    fn open_at(&self, line: u32) -> io::Result<BufReader<File>> {
        let block = line / STRIDE;
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(self.index[block as usize]))?;
        let mut r = BufReader::with_capacity(64 * 1024, file);
        let mut buf = Vec::new();
        for _ in 0..(line - block * STRIDE) {
            if read_line_capped(&mut r, 0, &mut buf)?.is_none() {
                break;
            }
        }
        Ok(r)
    }

    /// Read `count` lines starting at line `first` (0-based).
    fn read_lines(&self, first: u32, count: u32) -> io::Result<Vec<String>> {
        let mut out = Vec::with_capacity(count as usize);
        if count == 0 || first >= self.lines {
            return Ok(out);
        }
        let mut r = self.open_at(first)?;
        let mut buf = Vec::new();
        for _ in 0..count {
            match read_line_capped(&mut r, MAX_SHOW, &mut buf)? {
                None => break,
                Some(truncated) => {
                    if !truncated && buf.last() == Some(&b'\r') {
                        buf.pop();
                    }
                    let mut s = String::from_utf8_lossy(&buf).into_owned();
                    if truncated {
                        s.push_str(" …");
                    }
                    out.push(s);
                }
            }
        }
        Ok(out)
    }

    /// Look for `needle` in lines `from..to`. Returns the first (or last) line that has it.
    fn search(&self, needle: &[u8], from: u32, to: u32, match_case: bool, want_last: bool, ctl: &Control) -> Result<Option<u32>, String> {
        let to = to.min(self.lines);
        if from >= to || needle.is_empty() {
            return Ok(None);
        }
        let finder = memmem::Finder::new(needle);
        let mut r = self.open_at(from).map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        let mut found = None;
        for line in from..to {
            if line % 65_536 == 0 {
                ctl.check()?;
            }
            if read_line_capped(&mut r, MAX_SEARCH, &mut buf).map_err(|e| e.to_string())?.is_none() {
                break;
            }
            if !match_case {
                buf.make_ascii_lowercase();
            }
            if finder.find(&buf).is_some() {
                found = Some(line);
                if !want_last {
                    break;
                }
            }
        }
        Ok(found)
    }
}

/// Read one line into `out`, keeping at most `cap` bytes.
/// Returns None at end of file, otherwise Some(was_cut_short).
fn read_line_capped<R: BufRead>(r: &mut R, cap: usize, out: &mut Vec<u8>) -> io::Result<Option<bool>> {
    out.clear();
    let mut any = false;
    let mut truncated = false;
    loop {
        let buf = r.fill_buf()?;
        if buf.is_empty() {
            return Ok(if any { Some(truncated) } else { None });
        }
        any = true;
        let (seg, done, used) = match memchr(b'\n', buf) {
            Some(j) => (&buf[..j], true, j + 1),
            None => (buf, false, buf.len()),
        };
        let room = cap.saturating_sub(out.len());
        if seg.len() > room {
            truncated = true;
        }
        out.extend_from_slice(&seg[..seg.len().min(room)]);
        r.consume(used);
        if done {
            return Ok(Some(truncated));
        }
    }
}

struct Scanned {
    side: Side,
    /// One fingerprint per line (blank lines left out when they are ignored)
    hashes: Vec<u64>,
    /// When blank lines are ignored: the real line number of each fingerprint
    map: Option<Vec<u32>>,
}

/// Read a whole file once: fingerprint every line and remember where lines start.
/// `done_before` and `total` are byte counts used for the progress bar.
#[allow(unused_assignments)] // the line-end helper always resets its flags, even after the last line
fn scan(path: &Path, opts: &Options, ctl: &Control, done_before: u64, total: u64) -> Result<Scanned, String> {
    let name = file_name(path);
    let file = File::open(path).map_err(|e| format!("Could not open \"{name}\": {e}"))?;
    let size = file
        .metadata()
        .map_err(|e| format!("Could not read \"{name}\": {e}"))?
        .len();
    let io_err = |e: io::Error| format!("Could not read \"{name}\": {e}");

    let mut r = BufReader::with_capacity(READ_BUF, file);
    let mut pos: u64 = 0;
    let expected_lines;
    {
        let head = r.fill_buf().map_err(io_err)?;
        // Guess the number of lines from the first piece, so memory is reserved once, not over and over.
        let nl = memchr::memchr_iter(b'\n', head).count().max(1) as f64;
        expected_lines = ((size as f64) * nl / head.len().max(1) as f64 * 1.05) as usize + 16;
        if memchr(0, &head[..head.len().min(8192)]).is_some() {
            return Err(format!(
                "\"{name}\" doesn't look like a text file, so it can't be compared."
            ));
        }
        if head.starts_with(&[0xEF, 0xBB, 0xBF]) {
            r.consume(3); // skip the invisible UTF-8 marker at the start
            pos = 3;
        }
    }

    let skip_blank = opts.ignore_blank_lines;
    let mut hashes: Vec<u64> = Vec::with_capacity(expected_lines.min(u32::MAX as usize));
    let mut map: Option<Vec<u32>> = skip_blank.then(Vec::new);
    let mut index: Vec<u64> = Vec::new();
    let mut line_no: u64 = 0;
    let mut hasher = Xxh3::new();
    let mut in_line = false;
    let mut has_text = false;
    let mut pending_cr = false;
    let mut scratch = Vec::new();

    macro_rules! end_line {
        () => {{
            let h = hasher.digest();
            hasher.reset();
            in_line = false;
            match map.as_mut() {
                Some(m) => {
                    if has_text {
                        m.push(line_no as u32);
                        hashes.push(h);
                    }
                }
                None => hashes.push(h),
            }
            line_no += 1;
        }};
    }

    loop {
        ctl.check()?;
        let buf = r.fill_buf().map_err(io_err)?;
        if buf.is_empty() {
            break;
        }
        let len = buf.len();
        let mut i = 0;
        while i < len {
            if !in_line {
                if line_no % STRIDE as u64 == 0 {
                    index.push(pos + i as u64);
                }
                in_line = true;
                has_text = false;
            }
            let (end, ends_line) = match memchr(b'\n', &buf[i..]) {
                Some(j) => (i + j, true),
                None => (len, false),
            };
            let seg = &buf[i..end];
            if skip_blank && !has_text {
                has_text = seg.iter().any(|&b| !is_space(b));
            }
            feed(&mut hasher, seg, ends_line, &mut pending_cr, opts, &mut scratch);
            if ends_line {
                end_line!();
                i = end + 1;
            } else {
                i = end;
            }
        }
        pos += len as u64;
        r.consume(len);
        ctl.set((done_before + pos) * 800 / total.max(1));
    }
    if in_line {
        end_line!();
    }
    if line_no >= u32::MAX as u64 {
        return Err(format!("\"{name}\" has too many lines to compare."));
    }
    hashes.shrink_to_fit();
    if let Some(m) = map.as_mut() {
        m.shrink_to_fit();
    }
    let side = Side { path: path.to_path_buf(), size, lines: line_no as u32, index };
    Ok(Scanned { side, hashes, map })
}

/// Add a piece of a line to its fingerprint. A "\r" right before the line end is
/// left out, so Windows and Mac/Linux line endings count as the same.
fn feed(h: &mut Xxh3, seg: &[u8], ends_line: bool, pending_cr: &mut bool, o: &Options, scratch: &mut Vec<u8>) {
    if seg.is_empty() {
        if ends_line {
            *pending_cr = false;
        }
        return;
    }
    if std::mem::take(pending_cr) {
        add(h, b"\r", o, scratch);
    }
    let mut s = seg;
    if let Some((&b'\r', rest)) = s.split_last() {
        s = rest;
        if !ends_line {
            *pending_cr = true;
        }
    }
    add(h, s, o, scratch);
}

fn add(h: &mut Xxh3, s: &[u8], o: &Options, scratch: &mut Vec<u8>) {
    if !o.ignore_spaces && !o.ignore_case {
        h.update(s);
        return;
    }
    scratch.clear();
    for &b in s {
        if o.ignore_spaces && is_space(b) {
            continue;
        }
        scratch.push(if o.ignore_case { b.to_ascii_lowercase() } else { b });
    }
    h.update(scratch);
}

// ---------------------------------------------------------------------------
// The comparison
// ---------------------------------------------------------------------------

/// A run of lines that are either the same on both sides, or different.
/// A "same" run can have different lengths when it holds ignored blank lines.
#[derive(Debug, Clone, Copy)]
struct Op {
    a: u32,
    b: u32,
    alen: u32,
    blen: u32,
    /// First screen row of this run.
    row: u64,
    same: bool,
}

impl Op {
    fn rows(&self) -> u64 {
        self.alen.max(self.blen) as u64
    }
}

#[derive(Default)]
struct OpsBuilder {
    ops: Vec<Op>,
    row: u64,
}

impl OpsBuilder {
    fn push(&mut self, same: bool, a: usize, alen: usize, b: usize, blen: usize) {
        if alen == 0 && blen == 0 {
            return;
        }
        if let Some(last) = self.ops.last_mut() {
            // Join with the previous run when the lines still pair up one-to-one.
            let joinable = !same || (last.alen == last.blen && alen == blen);
            if last.same == same && joinable {
                self.row -= last.rows();
                last.alen += alen as u32;
                last.blen += blen as u32;
                self.row += last.rows();
                return;
            }
        }
        let op = Op { a: a as u32, b: b as u32, alen: alen as u32, blen: blen as u32, row: self.row, same };
        self.row += op.rows();
        self.ops.push(op);
    }
}

/// When blank lines are ignored, the matching works on non-blank lines only.
/// This turns that result back into real line numbers, putting the blank lines in between.
struct BlankExpander<'a> {
    ob: &'a mut OpsBuilder,
    ma: &'a [u32],
    mb: &'a [u32],
    ra: usize,
    rb: usize,
}

impl BlankExpander<'_> {
    /// Blank lines up to (not including) real lines `to_a` / `to_b`: shown, but not counted as different.
    fn gap(&mut self, to_a: usize, to_b: usize) {
        self.ob.push(true, self.ra, to_a - self.ra, self.rb, to_b - self.rb);
        self.ra = to_a;
        self.rb = to_b;
    }

    fn push(&mut self, same: bool, fa: usize, alen: usize, fb: usize, blen: usize) {
        if same {
            for k in 0..alen {
                let (la, lb) = (self.ma[fa + k] as usize, self.mb[fb + k] as usize);
                self.gap(la, lb);
                self.ob.push(true, la, 1, lb, 1);
                self.ra = la + 1;
                self.rb = lb + 1;
            }
        } else {
            // Pair the changed (non-blank) lines one by one. Blank lines in between are shown,
            // but not counted as different and not paired with a real line.
            for k in 0..alen.max(blen) {
                let la = (k < alen).then(|| self.ma[fa + k] as usize);
                let lb = (k < blen).then(|| self.mb[fb + k] as usize);
                self.gap(la.unwrap_or(self.ra), lb.unwrap_or(self.rb));
                self.ob.push(false, self.ra, la.is_some() as usize, self.rb, lb.is_some() as usize);
                self.ra += la.is_some() as usize;
                self.rb += lb.is_some() as usize;
            }
        }
    }
}

pub struct Comparison {
    left: Side,
    right: Side,
    ops: Vec<Op>,
    /// Positions in `ops` of the runs that differ.
    blocks: Vec<u32>,
    total_rows: u64,
    added: u64,
    removed: u64,
    changed: u64,
    millis: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub left: SideInfo,
    pub right: SideInfo,
    pub total_rows: u64,
    pub blocks: u32,
    pub added: u64,
    pub removed: u64,
    pub changed: u64,
    pub millis: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Seg {
    /// text
    pub t: String,
    /// highlighted (this part changed)
    pub h: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Cell {
    /// line number, starting at 1
    pub n: u32,
    /// pieces of the line
    pub s: Vec<Seg>,
}

/// One screen row. k: 0 = same, 1 = changed, 2 = added, 3 = removed.
#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub k: u8,
    pub l: Option<Cell>,
    pub r: Option<Cell>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct Hit {
    /// which difference (0-based)
    pub index: u32,
    /// first screen row of it
    pub row: u64,
    /// how many rows it covers
    pub rows: u64,
}

pub fn compare(left: &Path, right: &Path, opts: &Options, ctl: &Control) -> Result<Comparison, String> {
    let started = Instant::now();
    let size_of = |p: &Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let total = size_of(left) + size_of(right);
    let l = scan(left, opts, ctl, 0, total)?;
    let r = scan(right, opts, ctl, l.side.size, total)?;

    let mut ob = OpsBuilder::default();
    match (&l.map, &r.map) {
        (Some(ma), Some(mb)) => {
            let mut ex = BlankExpander { ob: &mut ob, ma, mb, ra: 0, rb: 0 };
            diff_in_windows(&l.hashes, &r.hashes, ctl, &mut |s, a, al, b, bl| ex.push(s, a, al, b, bl))?;
            ex.gap(l.side.lines as usize, r.side.lines as usize);
        }
        _ => diff_in_windows(&l.hashes, &r.hashes, ctl, &mut |s, a, al, b, bl| ob.push(s, a, al, b, bl))?,
    }
    let (lside, rside) = (l.side, r.side);

    let mut blocks = Vec::new();
    let (mut added, mut removed, mut changed) = (0u64, 0u64, 0u64);
    for (i, op) in ob.ops.iter().enumerate() {
        if !op.same {
            blocks.push(i as u32);
            let m = op.alen.min(op.blen) as u64;
            changed += m;
            removed += op.alen as u64 - m;
            added += op.blen as u64 - m;
        }
    }
    ob.ops.shrink_to_fit();
    ctl.set(1000);

    Ok(Comparison {
        left: lside,
        right: rside,
        total_rows: ob.row,
        ops: ob.ops,
        blocks,
        added,
        removed,
        changed,
        millis: started.elapsed().as_millis() as u64,
    })
}

/// A run of lines inside a window: (old start, old length, new start, new length, same?)
type Part = (usize, usize, usize, usize, bool);

/// Line up one window using imara-diff's "histogram" method (the one Git uses).
/// It is fast even when big blocks were inserted or removed.
fn match_window(input: &mut InternedInput<u64>, a: &[u64], b: &[u64]) -> Vec<Part> {
    input.clear();
    input.update_before(a.iter().copied());
    input.update_after(b.iter().copied());
    let diff = Diff::compute(imara_diff::Algorithm::Histogram, input);
    let mut parts = Vec::new();
    let (mut ai, mut bi) = (0usize, 0usize);
    for h in diff.hunks() {
        let (hs, he) = (h.before.start as usize, h.before.end as usize);
        let (ns, ne) = (h.after.start as usize, h.after.end as usize);
        if hs > ai {
            parts.push((ai, hs - ai, bi, ns - bi, true));
        }
        parts.push((hs, he - hs, ns, ne - ns, false));
        ai = he;
        bi = ne;
    }
    if ai < a.len() {
        parts.push((ai, a.len() - ai, bi, b.len() - bi, true));
    }
    parts
}

/// Line fingerprints are already well mixed, so a hash table can use them as they are.
#[derive(Default)]
struct AsIs(u64);

impl std::hash::Hasher for AsIs {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = self.0.rotate_left(8) ^ b as u64;
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }
}

/// A sample of the next lines of one side (only lines that occur once in the sample, so
/// common lines like "}" or blank lines don't count), to check quickly whether they show up
/// further on in the other side.
struct Probe {
    /// line fingerprint → found yet?
    lines: std::collections::HashMap<u64, bool, std::hash::BuildHasherDefault<AsIs>>,
    found: usize,
    /// How far the other side was already looked through.
    looked: usize,
}

impl Probe {
    fn new(next: &[u64]) -> Self {
        let mut lines = std::collections::HashMap::<u64, bool, _>::default();
        for &h in next.iter().take(PROBE) {
            // false: seen once so far, true: seen more than once
            lines.entry(h).and_modify(|more| *more = true).or_insert(false);
        }
        lines.retain(|_, more| !*more);
        Probe { lines, found: 0, looked: 0 }
    }

    /// Look through `other` (continuing where the last look stopped).
    /// True when at least half of the sample was found.
    fn look_in(&mut self, other: &[u64]) -> bool {
        for h in &other[self.looked.min(other.len())..] {
            if let Some(found) = self.lines.get_mut(h) {
                if !*found {
                    *found = true;
                    self.found += 1;
                }
            }
        }
        self.looked = other.len();
        self.found * 2 >= self.lines.len()
    }
}

/// Match up lines a window at a time, so memory stays small however big the files are.
/// Identical stretches are skipped quickly; only the area around each difference is matched.
/// Every run found is handed to `sink(same, a, a_len, b, b_len)`.
fn diff_in_windows(
    a: &[u64],
    b: &[u64],
    ctl: &Control,
    sink: &mut dyn FnMut(bool, usize, usize, usize, usize),
) -> Result<(), String> {
    let mut input = InternedInput::default();
    let (mut i, mut j) = (0usize, 0usize);
    loop {
        ctl.check()?;
        ctl.set(800 + (i as u64 * 200) / a.len().max(1) as u64);
        let mut s = 0;
        while i + s < a.len() && j + s < b.len() && a[i + s] == b[j + s] {
            s += 1;
        }
        sink(true, i, s, j, s);
        i += s;
        j += s;
        if i >= a.len() && j >= b.len() {
            return Ok(());
        }
        // Match up the next window of lines.
        let mut ae = (i + WINDOW).min(a.len());
        let mut be = (j + WINDOW).min(b.len());
        let mut parts = match_window(&mut input, &a[i..ae], &b[j..be]);
        let matched: usize = parts.iter().filter(|p| p.4).map(|p| p.1).sum();
        let at_end = ae == a.len() && be == b.len();

        // Mostly different? Then maybe a big block was added (or removed) here, and the lines
        // that match again lie further on. Look further ahead (doubling the stretch) until
        // the next lines of one side show up in the other side. This check is quick; if it
        // finds them, the window is matched once more, as big as needed. If nothing shows up
        // even far ahead, these parts are simply different and the first window is used.
        if !at_end && matched * 2 < (ae - i).min(be - j) {
            let mut probe_a = Probe::new(&a[i..]);
            let mut probe_b = Probe::new(&b[j..]);
            let mut size = WINDOW;
            let found = loop {
                let (e1, e2) = ((i + size).min(a.len()), (j + size).min(b.len()));
                let found = probe_a.look_in(&b[j..e2]) | probe_b.look_in(&a[i..e1]);
                if found || (e1 == a.len() && e2 == b.len()) || size >= MAX_WINDOW {
                    break found;
                }
                size *= 2;
            };
            if found && size > WINDOW {
                ctl.check()?;
                ae = (i + size).min(a.len());
                be = (j + size).min(b.len());
                parts = match_window(&mut input, &a[i..ae], &b[j..be]);
            }
        }
        let last_window = ae == a.len() && be == b.len();

        // Keep the result up to a solid matching stretch in the first half of the window;
        // the rest is matched again in the next window, with more lines to look ahead.
        // Short matches (often lines like "}" or blank lines that just happen to line up)
        // are only trusted as a place to stop when there is no solid matching stretch at all.
        let keep = if last_window {
            parts.len()
        } else {
            let (half_a, half_b) = ((ae - i) / 2, (be - j) / 2);
            let (mut in_first_half, mut last_solid, mut any) = (None, None, None);
            for (k, &(oi, ol, ni, nl, same)) in parts.iter().enumerate() {
                if same {
                    any = Some(k + 1);
                    if ol >= SOLID {
                        last_solid = Some(k + 1);
                        if oi + ol <= half_a.max(1) && ni + nl <= half_b.max(1) {
                            in_first_half = Some(k + 1);
                        }
                    }
                }
            }
            in_first_half.or(last_solid).or(any).unwrap_or(0)
        };

        if keep == 0 {
            // Nothing at all in common in this stretch: it's all different.
            sink(false, i, ae - i, j, be - j);
            i = ae;
            j = be;
            continue;
        }
        let (mut next_i, mut next_j) = (i, j);
        for &(oi, ol, ni, nl, same) in &parts[..keep] {
            sink(same, i + oi, ol, j + ni, nl);
            next_i = i + oi + ol;
            next_j = j + ni + nl;
        }
        i = next_i;
        j = next_j;
    }
}

impl Comparison {
    pub fn summary(&self) -> Summary {
        Summary {
            left: self.left.info(),
            right: self.right.info(),
            total_rows: self.total_rows,
            blocks: self.blocks.len() as u32,
            added: self.added,
            removed: self.removed,
            changed: self.changed,
            millis: self.millis,
        }
    }

    /// The rows the screen needs, starting at row `start`.
    pub fn rows(&self, start: u64, count: u32) -> Result<Vec<Row>, String> {
        let end = start.saturating_add(count as u64).min(self.total_rows);
        if start >= end {
            return Ok(Vec::new());
        }
        let mut plan: Vec<(u8, Option<u32>, Option<u32>)> = Vec::with_capacity((end - start) as usize);
        let mut i = self.ops.partition_point(|o| o.row + o.rows() <= start);
        let mut r = start;
        while r < end && i < self.ops.len() {
            let op = self.ops[i];
            let op_end = op.row + op.rows();
            while r < end && r < op_end {
                let k = (r - op.row) as u32;
                let l = (k < op.alen).then(|| op.a + k);
                let rr = (k < op.blen).then(|| op.b + k);
                let kind = if op.same {
                    0
                } else {
                    match (l, rr) {
                        (Some(_), Some(_)) => 1,
                        (None, Some(_)) => 2,
                        _ => 3,
                    }
                };
                plan.push((kind, l, rr));
                r += 1;
            }
            i += 1;
        }

        // The lines on each side are in order, so one read per side is enough.
        let read = |side: &Side, nums: &mut dyn Iterator<Item = u32>| -> Result<(u32, Vec<String>), String> {
            let mut first = None;
            let mut last = 0;
            for n in nums {
                first.get_or_insert(n);
                last = n;
            }
            match first {
                Some(f) => Ok((f, side.read_lines(f, last - f + 1).map_err(|e| e.to_string())?)),
                None => Ok((0, Vec::new())),
            }
        };
        let (lfirst, ltext) = read(&self.left, &mut plan.iter().filter_map(|p| p.1))?;
        let (rfirst, rtext) = read(&self.right, &mut plan.iter().filter_map(|p| p.2))?;
        let text = |v: &[String], first: u32, n: u32| v.get((n - first) as usize).cloned().unwrap_or_default();

        let mut out = Vec::with_capacity(plan.len());
        for (k, l, r) in plan {
            let lt = l.map(|n| text(&ltext, lfirst, n));
            let rt = r.map(|n| text(&rtext, rfirst, n));
            let (ls, rs) = match (lt, rt) {
                (Some(a), Some(b)) if k == 1 => {
                    let (x, y) = word_diff(&a, &b);
                    (Some(x), Some(y))
                }
                // Added/removed lines are already colored as a whole row; no extra highlight.
                (a, b) => (a.map(|t| plain(t, false)), b.map(|t| plain(t, false))),
            };
            out.push(Row {
                k,
                l: l.zip(ls).map(|(n, s)| Cell { n: n + 1, s }),
                r: r.zip(rs).map(|(n, s)| Cell { n: n + 1, s }),
            });
        }
        Ok(out)
    }

    /// Find a difference. dir: 1 = next after `row`, -1 = previous before `row`,
    /// 0 = first one at or after `row`.
    pub fn nav(&self, row: u64, dir: i32) -> Option<Hit> {
        let row_of = |oi: &u32| self.ops[*oi as usize].row;
        let n = self.blocks.len();
        let bi = match dir.signum() {
            1 => {
                let p = self.blocks.partition_point(|oi| row_of(oi) <= row);
                (p < n).then_some(p)?
            }
            -1 => {
                let p = self.blocks.partition_point(|oi| row_of(oi) < row);
                p.checked_sub(1)?
            }
            _ => {
                let p = self.blocks.partition_point(|oi| row_of(oi) < row);
                (p < n).then_some(p)?
            }
        };
        let op = &self.ops[self.blocks[bi] as usize];
        Some(Hit { index: bi as u32, row: op.row, rows: op.rows() })
    }

    /// The screen row showing line `line` (0-based) of the left or right file.
    pub fn row_for_line(&self, left: bool, line: u32) -> Option<u64> {
        let i = self.ops.partition_point(|o| {
            let (s, l) = start_len(o, left);
            s + l <= line
        });
        let o = self.ops.get(i)?;
        let (s, l) = start_len(o, left);
        (line >= s && line - s < l).then(|| o.row + (line - s) as u64)
    }

    /// The first left and right line shown at or after screen row `row`.
    fn lines_at_row(&self, row: u64) -> (u32, u32) {
        let i = self.ops.partition_point(|o| o.row + o.rows() <= row);
        match self.ops.get(i) {
            Some(o) => {
                let k = (row.saturating_sub(o.row)) as u32;
                (o.a + k.min(o.alen), o.b + k.min(o.blen))
            }
            None => (self.left.lines, self.right.lines),
        }
    }

    /// Search both files for `query`. Returns the screen row of the next match after
    /// `from_row` (or before it, when going backwards), wrapping around at the end.
    pub fn find(&self, query: &str, from_row: Option<u64>, forward: bool, match_case: bool, ctl: &Control) -> Result<Option<u64>, String> {
        let needle: Vec<u8> = if match_case { query.as_bytes().to_vec() } else { query.to_ascii_lowercase().into_bytes() };
        if needle.is_empty() || self.total_rows == 0 {
            return Ok(None);
        }
        let sides = [(&self.left, true), (&self.right, false)];
        let row_of = |left: bool, line: Option<u32>| line.and_then(|n| self.row_for_line(left, n));
        let mut best: Option<u64> = None;
        // Keep the nearest match: the smallest row going forward, the largest going back.
        let take = |best: &mut Option<u64>, row: Option<u64>, pick_smaller: bool| {
            if let Some(r) = row {
                *best = Some(match *best {
                    Some(b) if (pick_smaller && b <= r) || (!pick_smaller && b >= r) => b,
                    _ => r,
                });
            }
        };
        if forward {
            let (la, lb) = match from_row {
                Some(r) => self.lines_at_row(r + 1),
                None => (0, 0),
            };
            for ((side, left), from) in sides.iter().zip([la, lb]) {
                take(&mut best, row_of(*left, side.search(&needle, from, u32::MAX, match_case, false, ctl)?), true);
            }
            if best.is_none() && from_row.is_some() {
                // wrap around to the top
                for ((side, left), to) in sides.iter().zip([la, lb]) {
                    take(&mut best, row_of(*left, side.search(&needle, 0, to, match_case, false, ctl)?), true);
                }
            }
        } else {
            let (la, lb) = match from_row {
                Some(r) => self.lines_at_row(r),
                None => (self.left.lines, self.right.lines),
            };
            for ((side, left), to) in sides.iter().zip([la, lb]) {
                take(&mut best, row_of(*left, side.search(&needle, 0, to, match_case, true, ctl)?), false);
            }
            if best.is_none() && from_row.is_some() {
                // wrap around to the bottom
                for ((side, left), from) in sides.iter().zip([la, lb]) {
                    take(&mut best, row_of(*left, side.search(&needle, from, u32::MAX, match_case, true, ctl)?), false);
                }
            }
        }
        Ok(best)
    }

    /// A small "map" of where the differences are, for the strip beside the scrollbar.
    /// 0 = nothing, 1 = changed, 2 = added, 3 = removed.
    pub fn overview(&self, buckets: u32) -> Vec<u8> {
        let n = buckets.clamp(1, 4000) as usize;
        let mut out = vec![0u8; n];
        if self.total_rows == 0 {
            return out;
        }
        let t = self.total_rows as u128;
        for &oi in &self.blocks {
            let op = &self.ops[oi as usize];
            let s = (op.row as u128 * n as u128 / t) as usize;
            let e = (((op.row + op.rows()) as u128 * n as u128 + t - 1) / t) as usize;
            let kind = if op.alen == 0 { 2 } else if op.blen == 0 { 3 } else { 1 };
            for b in s..e.max(s + 1).min(n) {
                if out[b] == 0 || kind == 1 {
                    out[b] = kind;
                }
            }
        }
        out
    }
}

fn start_len(o: &Op, left: bool) -> (u32, u32) {
    if left { (o.a, o.alen) } else { (o.b, o.blen) }
}

fn plain(t: String, h: bool) -> Vec<Seg> {
    vec![Seg { t, h }]
}

fn push_seg(v: &mut Vec<Seg>, t: &str, h: bool) {
    if let Some(last) = v.last_mut() {
        if last.h == h {
            last.t.push_str(t);
            return;
        }
    }
    v.push(Seg { t: t.to_string(), h });
}

/// Find the exact words that changed between two lines.
fn word_diff(a: &str, b: &str) -> (Vec<Seg>, Vec<Seg>) {
    if a.len() > WORD_DIFF_MAX || b.len() > WORD_DIFF_MAX {
        return (plain(a.to_string(), true), plain(b.to_string(), true));
    }
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Myers)
        .timeout(Duration::from_millis(20))
        .diff_words(a, b);
    let (mut l, mut r) = (Vec::new(), Vec::new());
    for ch in diff.iter_all_changes() {
        let t = ch.value();
        match ch.tag() {
            ChangeTag::Equal => {
                push_seg(&mut l, t, false);
                push_seg(&mut r, t, false);
            }
            ChangeTag::Delete => push_seg(&mut l, t, true),
            ChangeTag::Insert => push_seg(&mut r, t, true),
        }
    }
    (l, r)
}

// ---------------------------------------------------------------------------
// Automatic tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn tmp(content: &[u8]) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("sbs-test-{}-{}.txt", std::process::id(), n));
        std::fs::write(&p, content).unwrap();
        p
    }

    fn cmp_paths(a: &Path, b: &Path, o: Options) -> Result<Comparison, String> {
        compare(a, b, &o, &Control::default())
    }

    fn cmp(a: &str, b: &str, o: Options) -> Comparison {
        cmp_paths(&tmp(a.as_bytes()), &tmp(b.as_bytes()), o).unwrap()
    }

    fn text(c: &Option<Cell>) -> String {
        c.as_ref().map(|c| c.s.iter().map(|s| s.t.as_str()).collect()).unwrap_or_default()
    }

    fn find(c: &Comparison, q: &str, from: Option<u64>, fwd: bool, case: bool) -> Option<u64> {
        c.find(q, from, fwd, case, &Control::default()).unwrap()
    }

    #[test]
    fn identical_files() {
        let c = cmp("a\nb\nc\n", "a\nb\nc\n", Options::default());
        let s = c.summary();
        assert_eq!(s.blocks, 0);
        assert_eq!(s.total_rows, 3);
        assert_eq!(s.left.lines, 3);
    }

    #[test]
    fn one_changed_line_with_word_highlight() {
        let c = cmp("theme = \"light\"\nx\n", "theme = \"dark\"\nx\n", Options::default());
        let s = c.summary();
        assert_eq!((s.blocks, s.changed, s.added, s.removed), (1, 1, 0, 0));
        let rows = c.rows(0, 10).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].k, 1);
        let l = &rows[0].l.as_ref().unwrap().s;
        assert!(l.iter().any(|s| s.h && s.t.contains("light")));
        assert!(l.iter().any(|s| !s.h && s.t.contains("theme")));
        assert_eq!(rows[1].k, 0);
    }

    #[test]
    fn added_and_removed_lines_line_up() {
        let c = cmp("a\nb\nc\nd\n", "a\nc\nd\ne\nf\n", Options::default());
        let s = c.summary();
        assert_eq!((s.removed, s.added), (1, 2));
        let rows = c.rows(0, 100).unwrap();
        let kinds: Vec<u8> = rows.iter().map(|r| r.k).collect();
        assert_eq!(kinds, vec![0, 3, 0, 0, 2, 2]);
        assert_eq!(text(&rows[1].l), "b");
        assert!(rows[1].r.is_none());
        assert_eq!(rows[4].r.as_ref().unwrap().n, 4);
        assert_eq!(text(&rows[5].r), "f");
    }

    #[test]
    fn windows_and_unix_line_endings_are_equal() {
        let c = cmp("a\r\nb\r\nc", "a\nb\nc\n", Options::default());
        assert_eq!(c.summary().blocks, 0);
        assert_eq!(text(&c.rows(0, 1).unwrap()[0].l), "a");
    }

    #[test]
    fn ignore_case_and_spaces() {
        assert_eq!(cmp("Hello World\n", "hello world\n", Options::default()).summary().blocks, 1);
        let o = Options { ignore_case: true, ..Default::default() };
        assert_eq!(cmp("Hello World\n", "hello world\n", o).summary().blocks, 0);
        let o = Options { ignore_spaces: true, ..Default::default() };
        assert_eq!(cmp("a = 1\n", "a=1   \n", o).summary().blocks, 0);
    }

    #[test]
    fn ignore_blank_lines() {
        let a = "one\n\ntwo\nthree\n";
        let b = "one\ntwo\n\n\nthree\nfour\n";
        assert!(cmp(a, b, Options::default()).summary().blocks >= 2);

        let o = Options { ignore_blank_lines: true, ..Default::default() };
        let c = cmp(a, b, o);
        let s = c.summary();
        assert_eq!((s.blocks, s.added, s.removed, s.changed), (1, 1, 0, 0));
        // every line of both files is still shown, in order
        let rows = c.rows(0, 100).unwrap();
        let left: Vec<u32> = rows.iter().filter_map(|r| r.l.as_ref().map(|c| c.n)).collect();
        let right: Vec<u32> = rows.iter().filter_map(|r| r.r.as_ref().map(|c| c.n)).collect();
        assert_eq!(left, vec![1, 2, 3, 4]);
        assert_eq!(right, vec![1, 2, 3, 4, 5, 6]);
        // "two" and "three" line up side by side
        let two = rows.iter().find(|r| text(&r.l) == "two").unwrap();
        assert_eq!(text(&two.r), "two");
        let three = rows.iter().find(|r| text(&r.l) == "three").unwrap();
        assert_eq!(text(&three.r), "three");
        assert_eq!(rows.last().unwrap().k, 2);
        // a line made only of spaces also counts as blank
        let c = cmp("x\n   \t\ny\n", "x\ny\n", o);
        assert_eq!(c.summary().blocks, 0);
    }

    #[test]
    fn utf8_marker_and_missing_last_newline() {
        let c = cmp_paths(&tmp(b"\xEF\xBB\xBFone\ntwo"), &tmp(b"one\ntwo\n"), Options::default()).unwrap();
        assert_eq!(c.summary().blocks, 0);
        assert_eq!(text(&c.rows(0, 1).unwrap()[0].l), "one");
    }

    #[test]
    fn empty_files() {
        let c = cmp("", "", Options::default());
        assert_eq!(c.summary().total_rows, 0);
        assert!(c.rows(0, 10).unwrap().is_empty());
        let c = cmp("", "x\n", Options::default());
        assert_eq!(c.summary().added, 1);
    }

    #[test]
    fn binary_files_are_refused() {
        let r = cmp_paths(&tmp(b"abc\0def"), &tmp(b"abc"), Options::default());
        assert!(r.is_err());
    }

    #[test]
    fn cancel_stops_the_comparison() {
        let ctl = Control::default();
        ctl.cancel.store(true, Ordering::SeqCst);
        let r = compare(&tmp(b"a\n"), &tmp(b"b\n"), &Options::default(), &ctl);
        assert_eq!(r.err().as_deref(), Some(CANCELLED));
    }

    #[test]
    fn progress_reaches_the_end() {
        let ctl = Control::default();
        compare(&tmp(b"a\nb\n"), &tmp(b"a\nc\n"), &Options::default(), &ctl).unwrap();
        assert_eq!(ctl.progress.load(Ordering::SeqCst), 1000);
    }

    #[test]
    fn rows_far_into_a_big_file() {
        let a: String = (0..5000).map(|i| format!("line {i}\n")).collect();
        let b = a.replace("line 3000\n", "line 3000 changed\n");
        let c = cmp(&a, &b, Options::default());
        let hit = c.nav(0, 0).unwrap();
        assert_eq!((hit.index, hit.row, hit.rows), (0, 3000, 1));
        let rows = c.rows(2999, 3).unwrap();
        assert_eq!(text(&rows[0].l), "line 2999");
        assert_eq!(text(&rows[1].r), "line 3000 changed");
        assert_eq!(rows[1].k, 1);
        assert_eq!(rows[2].l.as_ref().unwrap().n, 3002);
    }

    #[test]
    fn differences_across_many_windows() {
        let a: Vec<String> = (0..60_000).map(|i| format!("line {i}\n")).collect();
        let mut b = a.clone();
        b.remove(59_990);
        let extra: Vec<String> = (0..15_000).map(|i| format!("inserted {i}\n")).collect();
        b.splice(30_001..30_001, extra);
        b[10] = "line 10 edited\n".into();
        let c = cmp(&a.concat(), &b.concat(), Options::default());
        let s = c.summary();
        assert_eq!((s.blocks, s.changed, s.added, s.removed), (3, 1, 15_000, 1));
        assert_eq!(c.nav(11, 1).unwrap().row, 30_001);
    }

    fn counts(c: &Comparison) -> (u32, u64, u64, u64) {
        let s = c.summary();
        (s.blocks, s.changed, s.added, s.removed)
    }

    /// Lines that look like program code: many repeated blank lines and "}" lines.
    fn code_like(prefix: &str, n: usize) -> Vec<String> {
        (0..n)
            .map(|i| match i % 7 {
                0 => "\n".to_string(),
                3 => "}\n".to_string(),
                _ => format!("{prefix} {i}\n"),
            })
            .collect()
    }

    #[test]
    fn big_insert_bigger_than_a_window() {
        let a: Vec<String> = (0..60_000).map(|i| format!("line {i}\n")).collect();
        let mut b = a.clone();
        let extra: Vec<String> = (0..25_000).map(|i| format!("inserted {i}\n")).collect();
        b.splice(30_000..30_000, extra);
        let c = cmp(&a.concat(), &b.concat(), Options::default());
        assert_eq!(counts(&c), (1, 0, 25_000, 0));
        // the lines after the insertion line up again
        let row = c.rows(55_000, 1).unwrap();
        assert_eq!(row[0].k, 0);
        assert_eq!(text(&row[0].l), "line 30000");
        assert_eq!(text(&row[0].r), "line 30000");
    }

    #[test]
    fn big_delete_bigger_than_a_window() {
        let a: Vec<String> = (0..60_000).map(|i| format!("line {i}\n")).collect();
        let mut b = a.clone();
        b.drain(10_000..35_000);
        let c = cmp(&a.concat(), &b.concat(), Options::default());
        assert_eq!(counts(&c), (1, 0, 0, 25_000));
    }

    #[test]
    fn big_insert_at_the_start() {
        let a: Vec<String> = (0..30_000).map(|i| format!("line {i}\n")).collect();
        let mut b: Vec<String> = (0..21_000).map(|i| format!("header {i}\n")).collect();
        b.extend(a.iter().cloned());
        let c = cmp(&a.concat(), &b.concat(), Options::default());
        assert_eq!(counts(&c), (1, 0, 21_000, 0));
    }

    #[test]
    fn big_insert_in_code_with_repeated_lines() {
        let a = code_like("old", 60_000);
        let mut b = a.clone();
        b.splice(30_002..30_002, code_like("new", 25_000));
        let c = cmp(&a.concat(), &b.concat(), Options::default());
        // Repeated lines like "}" must not split it into pieces: it is one added block.
        assert_eq!(counts(&c), (1, 0, 25_000, 0));
    }

    #[test]
    fn two_completely_different_files() {
        let a: Vec<String> = (0..50_000).map(|i| format!("left {i}\n")).collect();
        let b: Vec<String> = (0..45_000).map(|i| format!("right {i}\n")).collect();
        let c = cmp(&a.concat(), &b.concat(), Options::default());
        assert_eq!(counts(&c), (1, 45_000, 0, 5_000));
    }

    /// Checks that hold for any comparison: every line of both files is shown once, in order,
    /// and rows marked "same" really show matching lines.
    fn check_rows(c: &Comparison, a: &[&str], b: &[&str], ignore_blank: bool) {
        let rows = c.rows(0, c.summary().total_rows as u32 + 1).unwrap();
        assert_eq!(rows.len() as u64, c.summary().total_rows);
        let left: Vec<u32> = rows.iter().filter_map(|r| r.l.as_ref().map(|c| c.n)).collect();
        let right: Vec<u32> = rows.iter().filter_map(|r| r.r.as_ref().map(|c| c.n)).collect();
        assert_eq!(left, (1..=a.len() as u32).collect::<Vec<_>>());
        assert_eq!(right, (1..=b.len() as u32).collect::<Vec<_>>());
        for r in &rows {
            match (&r.l, &r.r) {
                (Some(l), Some(rr)) if r.k == 0 => assert_eq!(a[l.n as usize - 1], b[rr.n as usize - 1]),
                (Some(l), None) if r.k == 0 => assert!(ignore_blank && a[l.n as usize - 1].is_empty()),
                (None, Some(rr)) if r.k == 0 => assert!(ignore_blank && b[rr.n as usize - 1].is_empty()),
                (Some(l), _) if r.k != 0 && ignore_blank => assert!(!a[l.n as usize - 1].is_empty()),
                _ => {}
            }
            if r.k != 0 && ignore_blank {
                if let Some(rr) = &r.r {
                    assert!(!b[rr.n as usize - 1].is_empty());
                }
            }
        }
    }

    #[test]
    fn random_files_always_show_every_line_in_order() {
        // A small fixed random generator, so the test is the same every run.
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut rnd = |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        let words = ["", "a", "b", "c", "}", "x y", "d"];
        for round in 0..400 {
            let a: Vec<&str> = (0..rnd(30)).map(|_| words[rnd(7) as usize]).collect();
            let mut b = a.clone();
            for _ in 0..rnd(6) {
                let at = rnd(b.len() as u64 + 1) as usize;
                match rnd(3) {
                    0 => b.insert(at, words[rnd(7) as usize]),
                    1 if at < b.len() => {
                        b.remove(at);
                    }
                    _ if at < b.len() => b[at] = words[rnd(7) as usize],
                    _ => {}
                }
            }
            let join = |v: &[&str]| v.iter().map(|l| format!("{l}\n")).collect::<String>();
            for ignore_blank in [false, true] {
                let o = Options { ignore_blank_lines: ignore_blank, ..Default::default() };
                let c = cmp(&join(&a), &join(&b), o);
                check_rows(&c, &a, &b, ignore_blank);
                if ignore_blank {
                    // Same numbers as comparing the files with their blank lines taken out.
                    let fa: Vec<&str> = a.iter().copied().filter(|l| !l.is_empty()).collect();
                    let fb: Vec<&str> = b.iter().copied().filter(|l| !l.is_empty()).collect();
                    let f = cmp(&join(&fa), &join(&fb), Options::default());
                    let (s, t) = (c.summary(), f.summary());
                    assert_eq!((s.changed, s.added, s.removed), (t.changed, t.added, t.removed), "round {round}");
                }
            }
        }
    }

    #[test]
    fn big_changes_always_show_every_line_in_order() {
        let a: Vec<String> = code_like("line", 70_000);
        let mut b = a.clone();
        b.splice(5_000..5_000, code_like("first", 30_000));
        b.drain(60_000..85_000);
        b.splice(70_000..70_000, code_like("second", 45_000));
        let ar: Vec<&str> = a.iter().map(|l| l.trim_end_matches('\n')).collect();
        let br: Vec<&str> = b.iter().map(|l| l.trim_end_matches('\n')).collect();
        let c = cmp(&a.concat(), &b.concat(), Options::default());
        check_rows(&c, &ar, &br, false);
        assert_eq!(counts(&c), (3, 0, 75_000, 25_000));
    }

    #[test]
    fn ignore_blank_lines_inside_a_change() {
        let o = Options { ignore_blank_lines: true, ..Default::default() };
        let c = cmp("a\nX\n\n\n\nY\nz\n", "a\nP\nQ\nz\n", o);
        // X→P and Y→Q, with the (ignored) blank lines between them
        assert_eq!(counts(&c), (2, 2, 0, 0));
        let rows = c.rows(0, 20).unwrap();
        // X lines up with P, Y with Q
        let x = rows.iter().find(|r| text(&r.l) == "X").unwrap();
        assert_eq!(text(&x.r), "P");
        let y = rows.iter().find(|r| text(&r.l) == "Y").unwrap();
        assert_eq!(text(&y.r), "Q");
        // every line of both files is still shown, in order
        let left: Vec<u32> = rows.iter().filter_map(|r| r.l.as_ref().map(|c| c.n)).collect();
        let right: Vec<u32> = rows.iter().filter_map(|r| r.r.as_ref().map(|c| c.n)).collect();
        assert_eq!(left, (1..=7).collect::<Vec<u32>>());
        assert_eq!(right, vec![1, 2, 3, 4]);
        // the blank lines are not shown as differences
        assert!(rows.iter().filter(|r| r.k != 0).all(|r| !text(&r.l).is_empty() || r.l.is_none()));
    }

    #[test]
    fn next_and_previous() {
        let c = cmp("a\nb\nc\nd\ne\n", "a\nB\nc\nD\ne\n", Options::default());
        let first = c.nav(0, 0).unwrap();
        let second = c.nav(first.row, 1).unwrap();
        assert_eq!((first.row, second.row), (1, 3));
        assert_eq!(second.index, 1);
        assert!(c.nav(second.row, 1).is_none());
        assert_eq!(c.nav(second.row, -1).unwrap(), first);
        assert!(c.nav(first.row, -1).is_none());
        let map = c.overview(5);
        assert_eq!(map, vec![0, 1, 0, 1, 0]);
    }

    #[test]
    fn go_to_line() {
        // left: a b c d       right: a X c d e
        let c = cmp("a\nb\nc\nd\n", "a\nc\nd\ne\nf\n", Options::default());
        // rows: a | b(removed) | c | d | e(added) | f(added)
        assert_eq!(c.row_for_line(true, 0), Some(0));
        assert_eq!(c.row_for_line(true, 1), Some(1));
        assert_eq!(c.row_for_line(true, 2), Some(2));
        assert_eq!(c.row_for_line(false, 1), Some(2));
        assert_eq!(c.row_for_line(false, 4), Some(5));
        assert_eq!(c.row_for_line(true, 99), None);
    }

    #[test]
    fn find_text_forward_backward_and_wrap() {
        let a = "apple\nbanana\ncherry\nbanana split\n";
        let b = "apple\nBANANA\ncherry\ndate\n";
        let c = cmp(a, b, Options::default());
        assert_eq!(find(&c, "banana", None, true, false), Some(1));
        assert_eq!(find(&c, "banana", Some(1), true, false), Some(3));
        // wraps back to the top
        assert_eq!(find(&c, "banana", Some(3), true, false), Some(1));
        assert_eq!(find(&c, "banana", Some(3), false, false), Some(1));
        // match case: "BANANA" only on the right
        assert_eq!(find(&c, "BANANA", None, true, true), Some(1));
        assert_eq!(find(&c, "date", None, true, false), Some(3));
        assert_eq!(find(&c, "zebra", None, true, false), None);
    }

    #[test]
    fn very_long_lines_are_cut_for_display() {
        let long = "x".repeat(50_000);
        let c = cmp(&format!("{long}\nend\n"), &format!("{long}\nend\n"), Options::default());
        let rows = c.rows(0, 2).unwrap();
        assert!(text(&rows[0].l).ends_with('…'));
        assert_eq!(text(&rows[1].l), "end");
    }
}
