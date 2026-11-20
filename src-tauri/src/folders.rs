//! Comparing two folders: which files are the same, which differ, and which are only in one of
//! them (in all folders inside them too). Files with different sizes differ; files of the same
//! size are compared by content (a fast fingerprint of every byte), so the answer is exact.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Instant;

use serde::Serialize;
use xxhash_rust::xxh3::Xxh3;

use crate::engine::{Control, CANCELLED};

/// Folders that are never looked into (Git's own bookkeeping).
const SKIP: &[&str] = &[".git"];

/// 0 = the same, 1 = different, 2 = only in the first folder, 3 = only in the second.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The path inside the folders, with "/" between folder names.
    pub path: String,
    pub kind: u8,
    pub left_size: Option<u64>,
    pub right_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderResult {
    pub left: String,
    pub right: String,
    pub entries: Vec<Entry>,
    pub same: u32,
    pub different: u32,
    pub only_left: u32,
    pub only_right: u32,
    pub millis: u64,
}

/// Every file in a folder (and the folders in it), as (path inside it, size), sorted.
fn files(root: &Path, ctl: &Control) -> Result<Vec<(String, u64)>, String> {
    let mut out = Vec::new();
    let mut todo = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = todo.pop() {
        if ctl.cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        let rd = std::fs::read_dir(&dir).map_err(|e| format!("Could not open the folder \"{}\": {e}", dir.display()))?;
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(ft) = e.file_type() else { continue };
            let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
            if ft.is_dir() {
                if !SKIP.contains(&name.as_str()) {
                    todo.push((e.path(), rel));
                }
            } else if ft.is_file() {
                out.push((rel, e.metadata().map(|m| m.len()).unwrap_or(0)));
            }
            // (links are left out: they could lead in circles)
        }
    }
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()).then(a.0.cmp(&b.0)));
    Ok(out)
}

fn fingerprint(p: &Path, ctl: &Control) -> Result<u64, String> {
    let mut r = BufReader::with_capacity(1 << 20, File::open(p).map_err(|e| format!("Could not read \"{}\": {e}", p.display()))?);
    let mut h = Xxh3::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if ctl.cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        let n = r.read(&mut buf).map_err(|e| format!("Could not read \"{}\": {e}", p.display()))?;
        if n == 0 {
            return Ok(h.digest());
        }
        h.update(&buf[..n]);
    }
}

pub fn compare(a: &Path, b: &Path, ctl: &Control) -> Result<FolderResult, String> {
    let started = Instant::now();
    if !a.is_dir() || !b.is_dir() {
        return Err("Both need to be folders.".into());
    }
    let (fa, fb) = (files(a, ctl)?, files(b, ctl)?);
    let total = (fa.len() + fb.len()).max(1) as u64;
    let mut entries = Vec::with_capacity(fa.len().max(fb.len()));
    let (mut i, mut j) = (0, 0);
    let key = |s: &str| (s.to_lowercase(), s.to_string());
    while i < fa.len() || j < fb.len() {
        ctl.progress.store(((i + j) as u64 * 1000 / total) as u32, Ordering::Relaxed);
        let order = match (fa.get(i), fb.get(j)) {
            (Some(x), Some(y)) => key(&x.0).cmp(&key(&y.0)),
            (Some(_), None) => std::cmp::Ordering::Less,
            _ => std::cmp::Ordering::Greater,
        };
        match order {
            std::cmp::Ordering::Less => {
                entries.push(Entry { path: fa[i].0.clone(), kind: 2, left_size: Some(fa[i].1), right_size: None });
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                entries.push(Entry { path: fb[j].0.clone(), kind: 3, left_size: None, right_size: Some(fb[j].1) });
                j += 1;
            }
            std::cmp::Ordering::Equal => {
                let (x, y) = (&fa[i], &fb[j]);
                let same = x.1 == y.1 && fingerprint(&a.join(&x.0), ctl)? == fingerprint(&b.join(&y.0), ctl)?;
                entries.push(Entry { path: x.0.clone(), kind: if same { 0 } else { 1 }, left_size: Some(x.1), right_size: Some(y.1) });
                i += 1;
                j += 1;
            }
        }
    }
    let count = |k: u8| entries.iter().filter(|e| e.kind == k).count() as u32;
    Ok(FolderResult {
        left: a.to_string_lossy().into_owned(),
        right: b.to_string_lossy().into_owned(),
        same: count(0),
        different: count(1),
        only_left: count(2),
        only_right: count(3),
        entries,
        millis: started.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_are_compared_file_by_file() {
        let root = std::env::temp_dir().join(format!("sbs-folders-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (a, b) = (root.join("a"), root.join("b"));
        for (dir, files) in [
            (&a, vec![("same.txt", "x"), ("changed.txt", "one"), ("sub/deep.txt", "d"), ("gone.txt", "g"), (".git/HEAD", "ref")]),
            (&b, vec![("same.txt", "x"), ("changed.txt", "two"), ("sub/deep.txt", "d"), ("new.txt", "n")]),
        ] {
            for (name, text) in files {
                let p = dir.join(name);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, text).unwrap();
            }
        }
        let r = compare(&a, &b, &Control::default()).unwrap();
        let got: Vec<(&str, u8)> = r.entries.iter().map(|e| (e.path.as_str(), e.kind)).collect();
        assert_eq!(got, vec![("changed.txt", 1), ("gone.txt", 2), ("new.txt", 3), ("same.txt", 0), ("sub/deep.txt", 0)]);
        assert_eq!((r.same, r.different, r.only_left, r.only_right), (2, 1, 1, 1));
        assert!(compare(&a, &a.join("same.txt"), &Control::default()).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
