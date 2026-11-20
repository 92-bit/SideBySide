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
use std::time::{Duration, Instant, SystemTime};

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
/// Lines longer than this (in bytes) are remembered, so the screen can jump over them.
const LONG_LINE: u64 = 64 * 1024;
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
/// Returned as the error when a file was changed on disk after it was compared.
pub const CHANGED: &str = "__changed__";

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

/// How Find treats UPPER and lower case.
#[derive(Clone, Copy)]
enum Case {
    /// exactly as typed
    Exact,
    /// ignore case of plain English letters (fast)
    Ascii,
    /// ignore case of all letters, like "Ä" and "ä"
    Any,
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | 0x0B | 0x0C)
}

// ---------------------------------------------------------------------------
// One file
// ---------------------------------------------------------------------------

pub struct Side {
    /// The file the user picked (its name, size and date are what's shown and checked).
    path: PathBuf,
    /// Where the lines are read from: the file itself, or a converted copy of it.
    read_from: PathBuf,
    /// The converted copy, if one was needed (removed when this is dropped).
    _copy: Option<TempCopy>,
    size: u64,
    /// When the file was last changed, as it was when compared.
    modified: Option<SystemTime>,
    lines: u32,
    /// Byte position of line 0, 256, 512, ...
    index: Vec<u64>,
    /// Very long lines (line number, byte position where the next line starts), so they
    /// can be jumped over instead of read through every time the screen needs a few lines.
    long: Vec<(u32, u64)>,
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

    /// True if the file was changed (or removed) since it was compared.
    fn changed_on_disk(&self) -> bool {
        match std::fs::metadata(&self.path) {
            Ok(m) => m.len() != self.size || m.modified().ok() != self.modified,
            Err(_) => true,
        }
    }

    /// Where the line after `line` starts, if `line` is a very long line.
    fn after_long(&self, line: u32) -> Option<u64> {
        self.long.binary_search_by_key(&line, |&(n, _)| n).ok().map(|k| self.long[k].1)
    }

    /// Open the file, positioned at the start of line `line`.
    fn open_at(&self, line: u32) -> io::Result<BufReader<File>> {
        let block = line / STRIDE;
        let (mut at_line, mut pos) = (block * STRIDE, self.index[block as usize]);
        // Jump past the last very long line before `line`, if there is one in this block.
        let k = self.long.partition_point(|&(n, _)| n < line);
        if let Some(&(n, next)) = k.checked_sub(1).map(|k| &self.long[k]) {
            if n >= at_line {
                (at_line, pos) = (n + 1, next);
            }
        }
        let mut file = File::open(&self.read_from)?;
        file.seek(SeekFrom::Start(pos))?;
        let mut r = BufReader::with_capacity(64 * 1024, file);
        let mut buf = Vec::new();
        for _ in at_line..line {
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
        for line in first..first.saturating_add(count) {
            match read_line_start(&mut r, MAX_SHOW, &mut buf)? {
                None => break,
                Some(truncated) => {
                    if truncated {
                        // Don't read the rest of a cut line: jump to the next line if we know
                        // where it starts (very long lines), otherwise read on to it.
                        match self.after_long(line) {
                            Some(next) => {
                                r.seek(SeekFrom::Start(next))?;
                            }
                            None => skip_rest_of_line(&mut r)?,
                        }
                    } else if buf.last() == Some(&b'\r') {
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
    fn search(&self, needle: &[u8], from: u32, to: u32, case: Case, want_last: bool, ctl: &Control) -> Result<Option<u32>, String> {
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
            let hit = match case {
                Case::Exact => finder.find(&buf).is_some(),
                Case::Ascii => {
                    buf.make_ascii_lowercase();
                    finder.find(&buf).is_some()
                }
                Case::Any => finder.find(String::from_utf8_lossy(&buf).to_lowercase().as_bytes()).is_some(),
            };
            if hit {
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

/// Read the start of one line into `out`, at most `cap` bytes. Unlike `read_line_capped`,
/// a longer line is left half-read: the caller must skip the rest.
/// Returns None at end of file, otherwise Some(was_cut_short).
fn read_line_start<R: BufRead>(r: &mut R, cap: usize, out: &mut Vec<u8>) -> io::Result<Option<bool>> {
    out.clear();
    let mut any = false;
    loop {
        let buf = r.fill_buf()?;
        if buf.is_empty() {
            return Ok(any.then_some(false));
        }
        any = true;
        let room = cap - out.len();
        match memchr(b'\n', &buf[..buf.len().min(room + 1)]) {
            Some(j) => {
                out.extend_from_slice(&buf[..j]);
                r.consume(j + 1);
                return Ok(Some(false));
            }
            None if buf.len() > room => {
                // More than `cap` bytes before the line end.
                out.extend_from_slice(&buf[..room]);
                r.consume(room);
                return Ok(Some(true));
            }
            None => {
                let n = buf.len();
                out.extend_from_slice(buf);
                r.consume(n);
            }
        }
    }
}

/// Read on to the start of the next line.
fn skip_rest_of_line<R: BufRead>(r: &mut R) -> io::Result<()> {
    loop {
        let buf = r.fill_buf()?;
        if buf.is_empty() {
            return Ok(());
        }
        if let Some(j) = memchr(b'\n', buf) {
            r.consume(j + 1);
            return Ok(());
        }
        let n = buf.len();
        r.consume(n);
    }
}

struct Scanned {
    side: Side,
    /// One fingerprint per line (blank lines left out when they are ignored)
    hashes: Vec<u64>,
    /// When blank lines are ignored: the real line number of each fingerprint
    map: Option<Vec<u32>>,
}

// ---------------------------------------------------------------------------
// Other text formats (UTF-16, old Mac line endings)
// ---------------------------------------------------------------------------

/// A converted copy of a file in the Temp folder. It is removed when no longer needed.
struct TempCopy(PathBuf);

impl Drop for TempCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[derive(Debug, PartialEq)]
enum Format {
    /// Normal text (UTF-8 or plain ASCII): compared as it is.
    Normal,
    /// UTF-16, as some Windows programs save it.
    Utf16 { big_endian: bool },
    /// Lines end with "\r" only (old Mac files).
    CrOnly,
}

/// Look at the start of a file to see how its text is stored.
fn detect(head: &[u8]) -> Format {
    if head.starts_with(&[0xFF, 0xFE]) {
        return Format::Utf16 { big_endian: false };
    }
    if head.starts_with(&[0xFE, 0xFF]) {
        return Format::Utf16 { big_endian: true };
    }
    // UTF-16 without a marker: in mostly-English text, every second byte is a zero.
    let n = head.len().min(8192) & !1;
    if n >= 16 {
        let zeros = |first: usize| head[first..n].iter().step_by(2).filter(|&&b| b == 0).count();
        let (even, odd, half) = (zeros(0), zeros(1), n / 2);
        if even * 10 < half && odd * 10 >= half * 7 {
            return Format::Utf16 { big_endian: false };
        }
        if odd * 10 < half && even * 10 >= half * 7 {
            return Format::Utf16 { big_endian: true };
        }
    }
    if memchr(b'\n', head).is_none() && memchr(b'\r', head).is_some() {
        return Format::CrOnly;
    }
    Format::Normal
}

/// Copy text to `out`, turning a lone "\r" into a line end ("\r\n" stays as it is).
fn put_text(text: &[u8], pending_cr: &mut bool, out: &mut Vec<u8>) {
    for &b in text {
        if *pending_cr {
            out.push(if b == b'\n' { b'\r' } else { b'\n' });
            *pending_cr = false;
        }
        if b == b'\r' {
            *pending_cr = true;
        } else {
            out.push(b);
        }
    }
}

static COPY_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Remove converted copies left in the Temp folder by a SideBySide that didn't close
/// normally. Copies of a SideBySide that is still running are kept.
pub fn remove_left_over_copies(running: impl Fn(u32) -> bool) {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let pid = name
            .strip_prefix("SideBySide-")
            .and_then(|r| r.strip_suffix(".txt"))
            .and_then(|r| r.split_once('-'))
            .and_then(|(pid, n)| n.parse::<u32>().ok().and(pid.parse::<u32>().ok()));
        if let Some(pid) = pid {
            if pid != std::process::id() && !running(pid) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// Write a normal UTF-8 copy of the file (with "\n" line ends) to the Temp folder.
fn convert(path: &Path, name: &str, format: &Format, ctl: &Control) -> Result<TempCopy, String> {
    use std::io::{Read, Write};
    let n = COPY_COUNTER.fetch_add(1, Ordering::SeqCst);
    let copy = TempCopy(std::env::temp_dir().join(format!("SideBySide-{}-{n}.txt", std::process::id())));
    let read_err = |e: io::Error| format!("Could not read \"{name}\": {e}");
    let write_err = |e: io::Error| format!("Could not convert \"{name}\" (Temp folder: {e})");

    let mut input = File::open(path).map_err(read_err)?;
    let mut output = std::io::BufWriter::with_capacity(READ_BUF, File::create(&copy.0).map_err(write_err)?);
    let mut buf = vec![0u8; READ_BUF];
    let mut out = Vec::with_capacity(READ_BUF * 2);
    let mut pending_cr = false;
    let mut carry: Vec<u8> = Vec::new(); // a byte left over from the last piece
    let mut high: Option<u16> = None; // the first half of a character split between pieces
    let mut first = true;
    loop {
        ctl.check()?;
        let got = input.read(&mut buf).map_err(read_err)?;
        if got == 0 {
            break;
        }
        out.clear();
        match format {
            Format::Utf16 { big_endian } => {
                carry.extend_from_slice(&buf[..got]);
                let pairs = carry.len() / 2;
                let mut units: Vec<u16> = Vec::with_capacity(pairs + 1);
                units.extend(high.take());
                units.extend(carry[..pairs * 2].chunks_exact(2).map(|c| {
                    if *big_endian { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) }
                }));
                carry.drain(..pairs * 2);
                if matches!(units.last(), Some(0xD800..=0xDBFF)) {
                    high = units.pop();
                }
                let mut text = String::with_capacity(units.len());
                for c in char::decode_utf16(units) {
                    let c = c.unwrap_or(char::REPLACEMENT_CHARACTER);
                    if first && c == '\u{FEFF}' {
                        first = false; // the invisible marker at the start
                        continue;
                    }
                    first = false;
                    text.push(c);
                }
                put_text(text.as_bytes(), &mut pending_cr, &mut out);
            }
            _ => put_text(&buf[..got], &mut pending_cr, &mut out),
        }
        output.write_all(&out).map_err(write_err)?;
    }
    let mut tail = Vec::new();
    if high.is_some() || !carry.is_empty() {
        tail.extend_from_slice("\u{FFFD}".as_bytes()); // a cut-off character at the very end
    }
    if pending_cr {
        tail.push(b'\n');
    }
    output.write_all(&tail).map_err(write_err)?;
    output.flush().map_err(write_err)?;
    Ok(copy)
}

/// Scan a file, first converting it to normal text if it's stored in another way.
fn scan_file(path: &Path, opts: &Options, ctl: &Control, done_before: u64, total: u64) -> Result<Scanned, String> {
    use std::io::Read;
    let name = file_name(path);
    let mut head = Vec::new();
    File::open(path)
        .and_then(|f| f.take(READ_BUF as u64).read_to_end(&mut head))
        .map_err(|e| format!("Could not open \"{name}\": {e}"))?;
    let format = detect(&head);
    if format == Format::Normal {
        return scan(path, &name, opts, ctl, done_before, total);
    }
    let meta = std::fs::metadata(path).map_err(|e| format!("Could not read \"{name}\": {e}"))?;
    let copy = convert(path, &name, &format, ctl)?;
    let mut s = scan(&copy.0, &name, opts, ctl, done_before, total)?;
    s.side.path = path.to_path_buf();
    s.side.size = meta.len();
    s.side.modified = meta.modified().ok();
    s.side.read_from = copy.0.clone();
    s.side._copy = Some(copy);
    Ok(s)
}

/// Read a whole file once: fingerprint every line and remember where lines start.
/// `done_before` and `total` are byte counts used for the progress bar.
#[allow(unused_assignments)] // the line-end helper always resets its flags, even after the last line
fn scan(path: &Path, name: &str, opts: &Options, ctl: &Control, done_before: u64, total: u64) -> Result<Scanned, String> {
    let file = File::open(path).map_err(|e| format!("Could not open \"{name}\": {e}"))?;
    let meta = file.metadata().map_err(|e| format!("Could not read \"{name}\": {e}"))?;
    let (size, modified) = (meta.len(), meta.modified().ok());
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
    // (A guess from a first piece full of very short lines could be far too big:
    // never reserve more than one line per two bytes, and carry on if it can't be reserved.)
    let mut hashes: Vec<u64> = Vec::new();
    let _ = hashes.try_reserve(expected_lines.min(size as usize / 2 + 16).min(u32::MAX as usize));
    let mut map: Option<Vec<u32>> = skip_blank.then(Vec::new);
    let mut index: Vec<u64> = Vec::new();
    let mut long: Vec<(u32, u64)> = Vec::new();
    let mut line_start: u64 = 0;
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
                line_start = pos + i as u64;
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
                let line_end = pos + end as u64;
                if line_end - line_start > LONG_LINE {
                    long.push((line_no as u32, line_end + 1));
                }
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
    long.shrink_to_fit();
    let side = Side {
        path: path.to_path_buf(),
        read_from: path.to_path_buf(),
        _copy: None,
        size,
        modified,
        lines: line_no as u32,
        index,
        long,
    };
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
    let l = scan_file(left, opts, ctl, 0, total)?;
    let r = scan_file(right, opts, ctl, l.side.size, total)?;

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
    /// True if either file was changed (or removed) since it was compared. Cheap: it only
    /// looks at each file's size and time, so it can be asked every second (live reload).
    pub fn changed_on_disk(&self) -> bool {
        self.left.changed_on_disk() || self.right.changed_on_disk()
    }

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
        // The text is read from the files as needed; if one was changed since, it would be wrong.
        if self.left.changed_on_disk() || self.right.changed_on_disk() {
            return Err(CHANGED.into());
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
        // Plain English letters only: the quick way. Other letters (like "Ä"): full Unicode lower case.
        let (needle, case) = if match_case {
            (query.as_bytes().to_vec(), Case::Exact)
        } else if query.is_ascii() {
            (query.to_ascii_lowercase().into_bytes(), Case::Ascii)
        } else {
            (query.to_lowercase().into_bytes(), Case::Any)
        };
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
                take(&mut best, row_of(*left, side.search(&needle, from, u32::MAX, case, false, ctl)?), true);
            }
            if best.is_none() && from_row.is_some() {
                // wrap around to the top
                for ((side, left), to) in sides.iter().zip([la, lb]) {
                    take(&mut best, row_of(*left, side.search(&needle, 0, to, case, false, ctl)?), true);
                }
            }
        } else {
            let (la, lb) = match from_row {
                Some(r) => self.lines_at_row(r),
                None => (self.left.lines, self.right.lines),
            };
            for ((side, left), to) in sides.iter().zip([la, lb]) {
                take(&mut best, row_of(*left, side.search(&needle, 0, to, case, true, ctl)?), false);
            }
            if best.is_none() && from_row.is_some() {
                // wrap around to the bottom
                for ((side, left), from) in sides.iter().zip([la, lb]) {
                    take(&mut best, row_of(*left, side.search(&needle, from, u32::MAX, case, true, ctl)?), false);
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
    // Removed and added words that sit together are looked at letter by letter, so
    // "readFile" → "readFiles" marks only the "s".
    let (mut del, mut ins) = (String::new(), String::new());
    for ch in diff.iter_all_changes() {
        let t = ch.value();
        match ch.tag() {
            ChangeTag::Equal => {
                letters(&del, &ins, &mut l, &mut r);
                del.clear();
                ins.clear();
                push_seg(&mut l, t, false);
                push_seg(&mut r, t, false);
            }
            ChangeTag::Delete => del.push_str(t),
            ChangeTag::Insert => ins.push_str(t),
        }
    }
    letters(&del, &ins, &mut l, &mut r);
    (l, r)
}

/// Longest stretch of changed text that is looked at letter by letter.
const LETTER_DIFF_MAX: usize = 300;

/// A removed piece and the added piece that replaced it. If they are mostly the same letters,
/// mark only the letters that differ; otherwise mark both pieces whole (marking scattered
/// letters of two different words would only be noise).
fn letters(del: &str, ins: &str, l: &mut Vec<Seg>, r: &mut Vec<Seg>) {
    if del.is_empty() || ins.is_empty() || del.len() > LETTER_DIFF_MAX || ins.len() > LETTER_DIFF_MAX {
        if !del.is_empty() {
            push_seg(l, del, true);
        }
        if !ins.is_empty() {
            push_seg(r, ins, true);
        }
        return;
    }
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Myers)
        .timeout(Duration::from_millis(5))
        .diff_chars(del, ins);
    if diff.ratio() < 0.6 {
        push_seg(l, del, true);
        push_seg(r, ins, true);
        return;
    }
    for ch in diff.iter_all_changes() {
        let t = ch.value();
        match ch.tag() {
            ChangeTag::Equal => {
                push_seg(l, t, false);
                push_seg(r, t, false);
            }
            ChangeTag::Delete => push_seg(l, t, true),
            ChangeTag::Insert => push_seg(r, t, true),
        }
    }
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
    fn similar_words_mark_only_the_changed_letters() {
        let marked = |s: &[Seg]| s.iter().filter(|x| x.h).map(|x| x.t.as_str()).collect::<Vec<_>>().join("|");
        // one letter added inside a word
        let (l, r) = word_diff("import { readFile } from", "import { readFiles } from");
        assert_eq!(marked(&l), "");
        assert_eq!(marked(&r), "s");
        // a number that changed by one digit
        let (l, r) = word_diff("const MAX = 1024;", "const MAX = 1028;");
        assert_eq!((marked(&l), marked(&r)), ("4".to_string(), "8".to_string()));
        // different words stay marked whole
        let (l, r) = word_diff("theme = light", "theme = dark");
        assert_eq!((marked(&l), marked(&r)), ("light".to_string(), "dark".to_string()));
        // the text itself is never changed
        let (l, r) = word_diff("a readFile b", "a readFiles c");
        assert_eq!(l.iter().map(|x| x.t.as_str()).collect::<String>(), "a readFile b");
        assert_eq!(r.iter().map(|x| x.t.as_str()).collect::<String>(), "a readFiles c");
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

    fn utf16(text: &str, big_endian: bool, marker: bool) -> Vec<u8> {
        let mut v = Vec::new();
        let units = marker.then_some(0xFEFFu16).into_iter().chain(text.encode_utf16());
        for u in units {
            v.extend_from_slice(&if big_endian { u.to_be_bytes() } else { u.to_le_bytes() });
        }
        v
    }

    #[test]
    fn utf16_files_are_compared_like_normal_text() {
        let s = "caf\u{e9} \u{1F600}\r\nline two\r\nend\r\n";
        for (be, marker) in [(false, true), (true, true), (false, false), (true, false)] {
            let c = cmp_paths(&tmp(&utf16(s, be, marker)), &tmp(s.as_bytes()), Options::default()).unwrap();
            assert_eq!(c.summary().blocks, 0, "big_endian={be} marker={marker}");
            assert_eq!(text(&c.rows(0, 1).unwrap()[0].l), "caf\u{e9} \u{1F600}");
            assert_eq!(c.left.lines, 3);
        }
    }

    #[test]
    fn utf16_characters_split_between_pieces() {
        // Big enough to be read in several pieces, with 4-byte characters at every position.
        let line = "x\u{1F600}y\u{e9}\n";
        let s: String = (0..60_000).map(|i| if i % 3 == 0 { format!("a{line}") } else { line.to_string() }).collect();
        let c = cmp_paths(&tmp(&utf16(&s, false, true)), &tmp(s.as_bytes()), Options::default()).unwrap();
        assert_eq!(c.summary().blocks, 0);
    }

    #[test]
    fn old_mac_line_endings() {
        let c = cmp_paths(&tmp(b"a\rb\rc\r"), &tmp(b"a\nb\nc\n"), Options::default()).unwrap();
        assert_eq!(c.summary().blocks, 0);
        assert_eq!(c.left.lines, 3);
        assert_eq!(text(&c.rows(1, 1).unwrap()[0].l), "b");
    }

    #[test]
    fn converted_copy_is_removed_afterwards() {
        let a = tmp(&utf16("one\ntwo\n", false, true));
        let c = cmp_paths(&a, &tmp(b"one\ntwo\n"), Options::default()).unwrap();
        let copy = c.left.read_from.clone();
        assert!(copy != a && copy.exists());
        assert_eq!(c.left.info().path, a.to_string_lossy());
        assert_eq!(c.left.info().size, std::fs::metadata(&a).unwrap().len());
        drop(c);
        assert!(!copy.exists());
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
    fn very_long_lines_are_jumped_over() {
        // Lines of 200 KB, 15 KB and 3 MB between short ones, with a change after them.
        let big = |c: char, n: usize| c.to_string().repeat(n);
        let a = format!("start\n{}\nmid\n{}\n{}\nend\nlast\n", big('a', 200_000), big('b', 15_000), big('c', 3_000_000));
        let b = a.replace("end\n", "END\n");
        let c = cmp(&a, &b, Options::default());
        assert_eq!(counts(&c), (1, 1, 0, 0));
        let rows = c.rows(0, 10).unwrap();
        let left: Vec<String> = rows.iter().map(|r| text(&r.l)).collect();
        assert_eq!(left[0], "start");
        assert!(left[1].starts_with("aaa") && left[1].ends_with('…') && left[1].len() < 20_000);
        assert_eq!(left[2], "mid");
        assert!(left[3].starts_with("bbb") && left[3].ends_with('…'));
        assert!(left[4].starts_with("ccc") && left[4].ends_with('…'));
        assert_eq!((left[5].as_str(), text(&rows[5].r).as_str(), rows[5].k), ("end", "END", 1));
        assert_eq!(left[6], "last");
        // starting in the middle, right after a long line, and at a long line
        assert_eq!(text(&c.rows(5, 1).unwrap()[0].l), "end");
        assert!(text(&c.rows(4, 1).unwrap()[0].l).starts_with("ccc"));
        assert_eq!(text(&c.rows(2, 1).unwrap()[0].l), "mid");
        // a line exactly as long as what is shown is not cut
        let exact = big('x', MAX_SHOW);
        let c = cmp(&format!("{exact}\nz\n"), &format!("{exact}\nz\n"), Options::default());
        let rows = c.rows(0, 2).unwrap();
        assert_eq!(text(&rows[0].l), exact);
        assert_eq!(text(&rows[1].l), "z");
    }

    #[test]
    fn a_file_changed_after_comparing_is_noticed() {
        let (pa, pb) = (tmp(b"one\ntwo\n"), tmp(b"one\nthree\n"));
        let c = cmp_paths(&pa, &pb, Options::default()).unwrap();
        assert!(c.rows(0, 2).is_ok());
        assert!(!c.changed_on_disk());
        std::fs::write(&pb, b"one\nthree\nfour\n").unwrap();
        assert!(c.changed_on_disk());
        assert_eq!(c.rows(0, 2).err().as_deref(), Some(CHANGED));
    }

    #[test]
    fn find_ignores_case_of_all_letters() {
        let c = cmp("x\nÄpfel\ny\n", "x\nÄpfel\ny\n", Options::default());
        assert_eq!(find(&c, "äpfel", None, true, false), Some(1));
        assert_eq!(find(&c, "ÄPFEL", None, true, false), Some(1));
        assert_eq!(find(&c, "äpfel", None, true, true), None);
        assert_eq!(find(&c, "Äpfel", None, true, true), Some(1));
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
