//! Folding: long runs of unchanged lines are hidden behind one "⋯ 1,234 unchanged lines" row.
//!
//! The comparison itself always works in "real" rows (every row of the side-by-side view). The
//! screen sees "screen" rows: the real rows with each folded run replaced by a single fold row.
//! This turns one into the other. A few unchanged lines stay visible around every difference.

/// Unchanged lines kept visible before and after each difference.
pub const CONTEXT: u64 = 3;
/// Runs that would hide fewer lines than this stay open (a fold row would save almost nothing).
const MIN_HIDDEN: u64 = 4;

#[derive(Debug, Default, Clone)]
pub struct Folds {
    /// (first hidden real row, how many) for each fold, in order.
    list: Vec<(u64, u64)>,
    /// Real rows hidden by the folds before fold i (each fold still shows as one row).
    before: Vec<u64>,
    real_total: u64,
}

/// What a screen row shows.
#[derive(Debug, PartialEq)]
pub enum At {
    Real(u64),
    /// Fold number i.
    Fold(usize),
}

impl Folds {
    /// No folds: screen rows are real rows.
    pub fn none(real_total: u64) -> Self {
        Folds { real_total, ..Default::default() }
    }

    /// Folds for the unchanged runs `same` ((first real row, rows), in order).
    pub fn new(same: &[(u64, u64)], real_total: u64) -> Self {
        let mut list = Vec::new();
        for &(start, len) in same {
            let keep_before = if start == 0 { 0 } else { CONTEXT };
            let keep_after = if start + len >= real_total { 0 } else { CONTEXT };
            if len >= keep_before + keep_after + MIN_HIDDEN {
                list.push((start + keep_before, len - keep_before - keep_after));
            }
        }
        let mut f = Folds { list, before: Vec::new(), real_total };
        f.recount();
        f
    }

    fn recount(&mut self) {
        let mut hidden = 0;
        self.before = self
            .list
            .iter()
            .map(|&(_, len)| {
                let b = hidden;
                hidden += len - 1;
                b
            })
            .collect();
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// How many rows the screen has.
    pub fn total(&self) -> u64 {
        let hidden = self.list.last().map_or(0, |&(_, len)| self.before[self.list.len() - 1] + len - 1);
        self.real_total - hidden
    }

    /// How many real rows fold i hides.
    pub fn len(&self, i: usize) -> u64 {
        self.list[i].1
    }

    /// The screen row of fold i.
    pub fn screen_of(&self, i: usize) -> u64 {
        self.list[i].0 - self.before[i]
    }

    /// What screen row `s` shows.
    pub fn at(&self, s: u64) -> At {
        // How many folds start at or before screen row s (their screen rows are in order).
        let (mut lo, mut hi) = (0, self.list.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.screen_of(mid) <= s { lo = mid + 1 } else { hi = mid }
        }
        let i = lo;
        if i > 0 && self.screen_of(i - 1) == s {
            return At::Fold(i - 1);
        }
        let hidden = if i > 0 { self.before[i - 1] + self.list[i - 1].1 - 1 } else { 0 };
        At::Real(s + hidden)
    }

    /// The real row of screen row `s` (a fold row: the first row it hides).
    pub fn real(&self, s: u64) -> u64 {
        match self.at(s) {
            At::Real(r) => r,
            At::Fold(i) => self.list[i].0,
        }
    }

    /// The screen row showing real row `r`, and the fold hiding it (if one does).
    pub fn screen(&self, r: u64) -> (u64, Option<usize>) {
        let i = self.list.partition_point(|&(at, _)| at <= r);
        if i > 0 {
            let (at, len) = self.list[i - 1];
            if r < at + len {
                return (self.screen_of(i - 1), Some(i - 1));
            }
            return (r - (self.before[i - 1] + len - 1), None);
        }
        (r, None)
    }

    /// Opens fold i. Returns its screen row and how many rows the screen gained.
    pub fn open(&mut self, i: usize) -> (u64, u64) {
        let row = self.screen_of(i);
        let (_, len) = self.list.remove(i);
        self.recount();
        (row, len - 1)
    }

    /// The screen row of the first fold after screen row `s` (or the end of the screen).
    pub fn next_fold_after(&self, s: u64) -> u64 {
        let (mut lo, mut hi) = (0, self.list.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.screen_of(mid) <= s { lo = mid + 1 } else { hi = mid }
        }
        if lo < self.list.len() { self.screen_of(lo) } else { self.total() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_hide_long_runs_but_keep_context() {
        // 100 rows: unchanged 0..40, a difference at 40..42, unchanged 42..100.
        let f = Folds::new(&[(0, 40), (42, 58)], 100);
        // the first run keeps only its last 3 rows; the last run only its first 3
        assert_eq!(f.list, vec![(0, 37), (45, 55)]);
        assert_eq!(f.total(), 100 - 36 - 54);
        assert_eq!(f.at(0), At::Fold(0));
        assert_eq!(f.at(1), At::Real(37));
        assert_eq!(f.at(3), At::Real(39));
        assert_eq!(f.at(4), At::Real(40)); // the difference
        assert_eq!(f.at(9), At::Fold(1));
        assert_eq!(f.total(), 10);
        // and back again
        assert_eq!(f.screen(40), (4, None));
        assert_eq!(f.screen(10), (0, Some(0)));
        assert_eq!(f.screen(70), (9, Some(1)));
        for s in 0..f.total() {
            if let At::Real(r) = f.at(s) {
                assert_eq!(f.screen(r), (s, None));
            }
        }
    }

    #[test]
    fn short_runs_stay_open_and_opening_a_fold_works() {
        let mut f = Folds::new(&[(10, 9), (30, 20)], 60);
        assert_eq!(f.list, vec![(33, 14)]); // 10..19 is too short to be worth folding
        assert_eq!(f.total(), 60 - 13);
        assert_eq!(f.open(0), (33, 13));
        assert!(f.is_empty());
        assert_eq!(f.total(), 60);
        assert_eq!(f.at(40), At::Real(40));
    }
}
