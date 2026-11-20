//! Three-way comparing: an original file and two changed versions of it (A and B), shown as
//! "A | original | B". Two ordinary comparisons (original↔A and original↔B) are lined up on the
//! original's lines: where both versions still match the original, nothing changed; in between
//! are the changes, marked by who made them, and as a conflict where A and B both changed the
//! same lines in different ways.

use std::path::Path;

use serde::Serialize;

use crate::engine::{compare, Comparison, Control, Hit, Options};

/// 0 = unchanged, 1 = changed in A only, 2 = changed in B only, 3 = changed the same way in both,
/// 4 = changed differently in both (a conflict).
#[derive(Debug, Clone, Copy)]
struct Run {
    o: u32,
    olen: u32,
    a: u32,
    alen: u32,
    b: u32,
    blen: u32,
    row: u64,
    kind: u8,
}

impl Run {
    fn rows(&self) -> u64 {
        self.olen.max(self.alen).max(self.blen) as u64
    }
}

pub struct Three {
    /// original ↔ A, and original ↔ B
    ca: Comparison,
    cb: Comparison,
    runs: Vec<Run>,
    /// Positions in `runs` of the changes.
    changes: Vec<u32>,
    total: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary3 {
    pub original: crate::engine::SideInfo,
    pub a: crate::engine::SideInfo,
    pub b: crate::engine::SideInfo,
    pub total_rows: u64,
    pub changes: u32,
    pub conflicts: u32,
    pub millis: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Line {
    pub n: u32,
    pub t: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Row3 {
    pub k: u8,
    pub a: Option<Line>,
    pub o: Option<Line>,
    pub b: Option<Line>,
}

/// How many lines of [from, to) (original lines) are matched unchanged by `spans`, starting at
/// line `other` of the other file (the spans are (original, other, length), in order).
fn unchanged(spans: &[(u32, u32, u32)], from: u32, to: u32, other: u32) -> u32 {
    let mut n = 0;
    let first = spans.partition_point(|&(o, _, len)| o + len <= from);
    for &(o, x, len) in &spans[first..] {
        if o >= to {
            break;
        }
        let (s, e) = (o.max(from), (o + len).min(to));
        // (only lines that line up with the same place in the other file count)
        if e > s && x + (s - o) == other + (s - from) {
            n += e - s;
        }
    }
    n
}

/// Lines that are long enough to make reading two whole changes to compare them too slow.
const SAME_CHANGE_MAX: u32 = 20_000;

impl Three {
    pub fn new(original: &Path, a: &Path, b: &Path, opts: &Options, ctl: &Control) -> Result<Three, String> {
        let ca = compare(original, a, opts, ctl)?;
        let cb = compare(original, b, opts, ctl)?;
        let (ma, mb) = (ca.same_spans(), cb.same_spans());
        let (so, sa, sb) = (ca.summary().left.lines, ca.summary().right.lines, cb.summary().right.lines);

        // Original lines unchanged in both: where both span lists overlap.
        let mut stable: Vec<(u32, u32, u32, u32)> = Vec::new(); // (original, a, b, length)
        let (mut i, mut j) = (0, 0);
        while i < ma.len() && j < mb.len() {
            let (oa, xa, la) = ma[i];
            let (ob, xb, lb) = mb[j];
            let (s, e) = (oa.max(ob), (oa + la).min(ob + lb));
            if e > s {
                stable.push((s, xa + (s - oa), xb + (s - ob), e - s));
            }
            if oa + la < ob + lb { i += 1 } else { j += 1 }
        }

        let mut runs = Vec::new();
        let mut row = 0u64;
        let (mut po, mut pa, mut pb) = (0u32, 0u32, 0u32);
        let push_change = |runs: &mut Vec<Run>, row: &mut u64, o: u32, olen: u32, a: u32, alen: u32, b: u32, blen: u32| {
            if olen == 0 && alen == 0 && blen == 0 {
                return;
            }
            let a_changed = !(alen == olen && unchanged(&ma, o, o + olen, a) == olen);
            let b_changed = !(blen == olen && unchanged(&mb, o, o + olen, b) == olen);
            let kind = match (a_changed, b_changed) {
                (false, false) => 0,
                (true, false) => 1,
                (false, true) => 2,
                (true, true) => {
                    let same = alen == blen
                        && alen <= SAME_CHANGE_MAX
                        && ca.lines(false, a, alen).ok() == cb.lines(false, b, blen).ok();
                    if same { 3 } else { 4 }
                }
            };
            let r = Run { o, olen, a, alen, b, blen, row: *row, kind };
            *row += r.rows();
            runs.push(r);
        };
        for &(o, a, b, len) in &stable {
            // (a stable run can start "before" the last one ended in A or B if the two
            // comparisons lined lines up differently; such a run is skipped)
            if o < po || a < pa || b < pb {
                continue;
            }
            push_change(&mut runs, &mut row, po, o - po, pa, a - pa, pb, b - pb);
            runs.push(Run { o, olen: len, a, alen: len, b, blen: len, row, kind: 0 });
            row += len as u64;
            (po, pa, pb) = (o + len, a + len, b + len);
        }
        push_change(&mut runs, &mut row, po, so.saturating_sub(po), pa, sa.saturating_sub(pa), pb, sb.saturating_sub(pb));
        let changes = runs.iter().enumerate().filter(|(_, r)| r.kind != 0).map(|(i, _)| i as u32).collect();
        Ok(Three { ca, cb, runs, changes, total: row })
    }

    pub fn summary(&self, millis: u64) -> Summary3 {
        Summary3 {
            original: self.ca.summary().left,
            a: self.ca.summary().right,
            b: self.cb.summary().right,
            total_rows: self.total,
            changes: self.changes.len() as u32,
            conflicts: self.runs.iter().filter(|r| r.kind == 4).count() as u32,
            millis,
        }
    }

    pub fn rows(&self, start: u64, count: u32) -> Result<Vec<Row3>, String> {
        let end = start.saturating_add(count as u64).min(self.total);
        let mut plan = Vec::new(); // (kind, original, a, b) line numbers from 0
        let mut i = self.runs.partition_point(|r| r.row + r.rows() <= start);
        let mut row = start;
        while row < end && i < self.runs.len() {
            let r = self.runs[i];
            while row < end && row < r.row + r.rows() {
                let k = (row - r.row) as u32;
                plan.push((r.kind, (k < r.olen).then(|| r.o + k), (k < r.alen).then(|| r.a + k), (k < r.blen).then(|| r.b + k)));
                row += 1;
            }
            i += 1;
        }
        // One read per file: the lines on screen are in order in each of them.
        let read = |nums: Vec<u32>, c: &Comparison, left: bool| -> Result<(u32, Vec<String>), String> {
            match (nums.first(), nums.last()) {
                (Some(&f), Some(&l)) => Ok((f, c.lines(left, f, l - f + 1)?)),
                _ => Ok((0, Vec::new())),
            }
        };
        let (fo, to) = read(plan.iter().filter_map(|p| p.1).collect(), &self.ca, true)?;
        let (fa, ta) = read(plan.iter().filter_map(|p| p.2).collect(), &self.ca, false)?;
        let (fb, tb) = read(plan.iter().filter_map(|p| p.3).collect(), &self.cb, false)?;
        let line = |n: Option<u32>, first: u32, text: &[String]| {
            n.map(|n| Line { n: n + 1, t: text.get((n - first) as usize).cloned().unwrap_or_default() })
        };
        Ok(plan
            .into_iter()
            .map(|(k, o, a, b)| Row3 { k, o: line(o, fo, &to), a: line(a, fa, &ta), b: line(b, fb, &tb) })
            .collect())
    }

    /// The next (dir 1) or previous (-1) change from row `row`, or the first one at or after it (0).
    pub fn nav(&self, row: u64, dir: i32) -> Option<Hit> {
        let hit = |n: usize| {
            let r = self.runs[self.changes[n] as usize];
            Hit { index: n as u32, row: r.row, rows: r.rows() }
        };
        let starts = |n: usize| self.runs[self.changes[n] as usize].row;
        let n = self.changes.len();
        if n == 0 {
            return None;
        }
        let at = self.changes.partition_point(|&c| self.runs[c as usize].row < row);
        Some(match dir {
            0 => hit(at.min(n - 1)),
            1 => hit((0..n).find(|&k| starts(k) > row).unwrap_or(0)),
            _ => hit((0..n).rev().find(|&k| starts(k) < row).unwrap_or(n - 1)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_files_line_up_on_the_original() {
        let dir = std::env::temp_dir();
        let n = std::process::id();
        let file = |name: &str, text: &str| {
            let p = dir.join(format!("sbs-three-{n}-{name}"));
            std::fs::write(&p, text).unwrap();
            p
        };
        let o = file("o.txt", "a\nb\nc\nd\ne\nf\n");
        let a = file("a.txt", "a\nB\nc\nd\nE1\nf\n"); // changed b, and e
        let b = file("b.txt", "a\nb\nc\nx\nd\nE2\nf\n"); // added x, changed e differently
        let t = Three::new(&o, &a, &b, &Options::default(), &Control::default()).unwrap();
        let s = t.summary(0);
        let kinds: Vec<(u8, Option<String>, Option<String>, Option<String>)> = t
            .rows(0, 100)
            .unwrap()
            .into_iter()
            .map(|r| (r.k, r.a.map(|l| l.t), r.o.map(|l| l.t), r.b.map(|l| l.t)))
            .collect();
        let s3 = |x: &str| Some(x.to_string());
        assert_eq!(
            kinds,
            vec![
                (0, s3("a"), s3("a"), s3("a")),
                (1, s3("B"), s3("b"), s3("b")),
                (0, s3("c"), s3("c"), s3("c")),
                (2, None, None, s3("x")),
                (0, s3("d"), s3("d"), s3("d")),
                (4, s3("E1"), s3("e"), s3("E2")),
                (0, s3("f"), s3("f"), s3("f")),
            ]
        );
        assert_eq!((s.changes, s.conflicts), (3, 1));
        assert_eq!(t.nav(0, 1).map(|h| h.row), Some(1));
        assert_eq!(t.nav(1, 1).map(|h| h.row), Some(3));
        assert_eq!(t.nav(1, -1).map(|h| h.row), Some(5)); // round to the last one
    }
}
