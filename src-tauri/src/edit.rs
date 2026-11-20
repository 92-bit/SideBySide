//! Editing and merging: changes are made to a working copy of the file in the Temp folder, never
//! to the file itself, until the user presses Save. Every change replaces some whole lines with
//! other lines; it remembers what it replaced, so it can be undone (and redone).
//!
//! The working copy is rewritten as a stream for each change (read line by line, write the new
//! version next to it, then swap), so even very big files can be edited with little memory.

use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static COPIES: AtomicU32 = AtomicU32::new(0);
const BUF: usize = 1 << 20;

/// One change: at line `at` (0-based), the lines `old` were replaced by the lines `new`
/// (raw bytes, each line with its line end).
#[derive(Debug, Clone)]
pub struct Change {
    pub at: u32,
    pub old: Vec<u8>,
    pub new: Vec<u8>,
}

impl Change {
    fn inverse(&self) -> Change {
        Change { at: self.at, old: self.new.clone(), new: self.old.clone() }
    }
}

/// How many lines some raw text holds (a last line without a line end counts too).
pub fn count_lines(b: &[u8]) -> u32 {
    let n = memchr::memchr_iter(b'\n', b).count() as u32;
    if b.last().is_some_and(|&c| c != b'\n') { n + 1 } else { n }
}

/// A file being edited: the user's file and its working copy.
#[derive(Debug)]
pub struct Working {
    pub real: PathBuf,
    pub copy: PathBuf,
    /// "\r\n" or "\n": how the file ends its lines (new lines get the same).
    pub newline: &'static [u8],
}

impl Drop for Working {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.copy);
    }
}

impl Working {
    /// Starts editing `real`: makes the working copy.
    pub fn start(real: &Path) -> io::Result<Working> {
        let n = COPIES.fetch_add(1, Ordering::SeqCst);
        let copy = std::env::temp_dir().join(format!("SideBySide-{}-edit{n}.txt", std::process::id()));
        std::fs::copy(real, &copy)?;
        // Line ends: as in the first line of the file ("\n" if it has none).
        let mut first = Vec::new();
        BufReader::new(File::open(&copy)?).take(1 << 20).read_until(b'\n', &mut first)?;
        let newline: &'static [u8] = if first.ends_with(b"\r\n") { b"\r\n" } else { b"\n" };
        Ok(Working { real: real.to_path_buf(), copy, newline })
    }

    /// Text typed by the user as whole lines with this file's line ends.
    pub fn lines_of(&self, text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        for line in text.split('\n') {
            out.extend_from_slice(line.strip_suffix('\r').unwrap_or(line).as_bytes());
            out.extend_from_slice(self.newline);
        }
        out
    }

    /// Lines copied from another file, with their line ends changed to this file's.
    pub fn adapt(&self, raw: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(raw.len());
        for line in raw.split_inclusive(|&b| b == b'\n') {
            let text = line.strip_suffix(b"\n").unwrap_or(line);
            let text = text.strip_suffix(b"\r").unwrap_or(text);
            out.extend_from_slice(text);
            out.extend_from_slice(self.newline);
        }
        out
    }

    /// Replaces `count` lines at line `at` (0-based) with `new` (whole lines). Returns the change,
    /// which knows the lines it replaced.
    pub fn replace(&self, at: u32, count: u32, new: Vec<u8>) -> io::Result<Change> {
        let old = self.rewrite(at, count, &new)?;
        Ok(Change { at, old, new })
    }

    /// Undoes `c` (or redoes it, given its inverse). Returns the change that undoes this one.
    pub fn apply_inverse(&self, c: &Change) -> io::Result<Change> {
        let inv = c.inverse();
        self.rewrite(inv.at, count_lines(&inv.old), &inv.new)?;
        Ok(inv)
    }

    /// The streaming rewrite: lines before `at` as they are, then `new` instead of the next
    /// `count` lines, then the rest. Returns the lines that were taken out.
    fn rewrite(&self, at: u32, count: u32, new: &[u8]) -> io::Result<Vec<u8>> {
        let tmp = self.copy.with_extension("next");
        let mut r = BufReader::with_capacity(BUF, File::open(&self.copy)?);
        let mut w = BufWriter::with_capacity(BUF, File::create(&tmp)?);
        let mut line = Vec::new();
        let mut no = 0u32;
        let mut old = Vec::new();
        // The last line written had no line end (only possible at the very end of the file).
        let mut open_end = false;
        let put = |w: &mut BufWriter<File>, bytes: &[u8], open_end: &mut bool| -> io::Result<()> {
            if bytes.is_empty() {
                return Ok(());
            }
            if *open_end {
                w.write_all(self.newline)?;
            }
            w.write_all(bytes)?;
            *open_end = !bytes.ends_with(b"\n");
            Ok(())
        };
        loop {
            if no == at {
                put(&mut w, new, &mut open_end)?;
            }
            line.clear();
            if r.read_until(b'\n', &mut line)? == 0 {
                break;
            }
            if no >= at && no < at.saturating_add(count) {
                old.extend_from_slice(&line);
            } else {
                put(&mut w, &line, &mut open_end)?;
            }
            no += 1;
        }
        if no < at {
            // Past the end: add the new lines at the end.
            put(&mut w, new, &mut open_end)?;
        }
        w.flush()?;
        drop(w);
        drop(r);
        std::fs::rename(&tmp, &self.copy)?;
        Ok(old)
    }

    /// Writes the working copy over the user's file (through a file next to it, then swapped in,
    /// so the user's file is never left half written).
    pub fn save(&self) -> io::Result<()> {
        let mut side = self.real.clone().into_os_string();
        side.push(".sidebyside-saving");
        let side = PathBuf::from(side);
        std::fs::copy(&self.copy, &side)?;
        std::fs::rename(&side, &self.real).inspect_err(|_| {
            let _ = std::fs::remove_file(&side);
        })
    }
}

/// Raw lines `first .. first + count` (0-based) of a file, each with its line end.
pub fn read_raw(path: &Path, first: u32, count: u32) -> io::Result<Vec<u8>> {
    let mut r = BufReader::with_capacity(BUF, File::open(path)?);
    let mut line = Vec::new();
    let mut out = Vec::new();
    let mut no = 0u32;
    while no < first.saturating_add(count) {
        line.clear();
        if r.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if no >= first {
            out.extend_from_slice(&line);
        }
        no += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, text: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("sbs-edit-{}-{name}", std::process::id()));
        std::fs::write(&p, text).unwrap();
        p
    }
    fn read(p: &Path) -> String {
        std::fs::read_to_string(p).unwrap()
    }

    #[test]
    fn replace_undo_redo_and_save() {
        let real = file("a.txt", "one\r\ntwo\r\nthree\r\n");
        let w = Working::start(&real).unwrap();
        assert_eq!(w.newline, b"\r\n");
        // change line 2, keeping the file's line ends
        let c1 = w.replace(1, 1, w.lines_of("TWO")).unwrap();
        assert_eq!(read(&w.copy), "one\r\nTWO\r\nthree\r\n");
        assert_eq!(c1.old, b"two\r\n");
        // insert two lines at the start, delete the last line
        let c2 = w.replace(0, 0, w.adapt(b"zero\nhalf\n")).unwrap();
        let c3 = w.replace(4, 1, Vec::new()).unwrap();
        assert_eq!(read(&w.copy), "zero\r\nhalf\r\none\r\nTWO\r\n");
        // undo all three, newest first: back to the start
        let r3 = w.apply_inverse(&c3).unwrap();
        w.apply_inverse(&c2).unwrap();
        w.apply_inverse(&c1).unwrap();
        assert_eq!(read(&w.copy), read(&real));
        // redo the last one undone, by undoing its undo
        w.apply_inverse(&c1.inverse()).unwrap();
        assert_eq!(read(&w.copy), "one\r\nTWO\r\nthree\r\n");
        let _ = r3;
        // the real file only changes on save
        assert_eq!(read(&real), "one\r\ntwo\r\nthree\r\n");
        w.save().unwrap();
        assert_eq!(read(&real), "one\r\nTWO\r\nthree\r\n");
    }

    #[test]
    fn a_last_line_without_line_end() {
        let real = file("b.txt", "a\nb");
        let w = Working::start(&real).unwrap();
        w.replace(2, 0, w.lines_of("c")).unwrap(); // add at the end
        assert_eq!(read(&w.copy), "a\nb\nc\n");
        let real = file("c.txt", "a\nb");
        let w = Working::start(&real).unwrap();
        w.replace(1, 1, w.lines_of("B")).unwrap();
        assert_eq!(read(&w.copy), "a\nB\n");
        assert_eq!(read_raw(&real, 1, 5).unwrap(), b"b");
        assert_eq!(count_lines(b"x\ny"), 2);
        assert_eq!(count_lines(b"x\ny\n"), 2);
        assert_eq!(count_lines(b""), 0);
    }
}
