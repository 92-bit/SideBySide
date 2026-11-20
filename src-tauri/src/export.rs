//! Sharing a comparison: as a patch (the "unified diff" text that Git and other tools read), or as
//! an HTML report that opens in any browser and can be printed to PDF.
//!
//! Both show every difference with a few unchanged lines around it, never the whole file, so even a
//! comparison of two huge files gives a report of a sensible size.

use crate::engine::{Cell, Comparison, Row, Seg};

/// Unchanged lines shown around each difference.
const CONTEXT: u64 = 3;

/// Groups of rows to show: each difference with CONTEXT rows around it, joined when they touch.
fn hunks(c: &Comparison) -> Vec<(u64, u64)> {
    let total = c.total_rows();
    let mut out: Vec<(u64, u64)> = Vec::new();
    for (row, rows) in c.block_rows() {
        let start = row.saturating_sub(CONTEXT);
        let end = (row + rows + CONTEXT).min(total);
        match out.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => out.push((start, end)),
        }
    }
    out
}

/// The rows of one group, read in pieces.
fn rows_of(c: &Comparison, (start, end): (u64, u64)) -> Result<Vec<Row>, String> {
    let mut out = Vec::new();
    let mut at = start;
    while at < end {
        let n = (end - at).min(2000) as u32;
        out.extend(c.rows(at, n)?);
        at += n as u64;
    }
    Ok(out)
}

fn text(c: &Option<Cell>) -> String {
    c.as_ref().map(|c| c.s.iter().map(|s| s.t.as_str()).collect()).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Patch
// ---------------------------------------------------------------------------

/// The differences as a unified diff. Stops (and says so) after `limit` bytes.
pub fn patch(c: &Comparison, limit: usize) -> Result<String, String> {
    let s = c.summary();
    let mut out = format!("--- {}\n+++ {}\n", s.left.path, s.right.path);
    for h in hunks(c) {
        let rows = rows_of(c, h)?;
        let first = |left: bool| {
            rows.iter()
                .find_map(|r| if left { r.l.as_ref() } else { r.r.as_ref() }.map(|c| c.n))
        };
        let count = |left: bool| rows.iter().filter(|r| if left { r.l.is_some() } else { r.r.is_some() }).count();
        let (nl, nr) = (count(true), count(false));
        // (an empty side is written as the line before it, as diff tools expect)
        let line_before = |left: bool| -> u32 {
            let lines_seen = rows.first().and_then(|r| if left { r.l.as_ref() } else { r.r.as_ref() }.map(|c| c.n));
            lines_seen.map_or(0, |n| n.saturating_sub(1))
        };
        let al = first(true).unwrap_or_else(|| line_before(true));
        let ar = first(false).unwrap_or_else(|| line_before(false));
        out.push_str(&format!("@@ -{al},{nl} +{ar},{nr} @@\n"));
        // Unchanged rows as they are; a run of changed rows as all its "-" lines, then all its "+" lines.
        let mut i = 0;
        while i < rows.len() {
            if rows[i].k == 0 {
                out.push(' ');
                out.push_str(&text(&rows[i].l));
                out.push('\n');
                i += 1;
                continue;
            }
            let j = rows[i..].iter().position(|r| r.k == 0).map_or(rows.len(), |p| i + p);
            for r in &rows[i..j] {
                if r.l.is_some() {
                    out.push('-');
                    out.push_str(&text(&r.l));
                    out.push('\n');
                }
            }
            for r in &rows[i..j] {
                if r.r.is_some() {
                    out.push('+');
                    out.push_str(&text(&r.r));
                    out.push('\n');
                }
            }
            i = j;
        }
        if out.len() > limit {
            out.push_str("\n(… cut off here: the full list of differences is too long to copy.)\n");
            break;
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// HTML report
// ---------------------------------------------------------------------------

fn esc(t: &str, out: &mut String) {
    for ch in t.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

fn cell_html(c: &Option<Cell>, out: &mut String) {
    let Some(c) = c else { return };
    for Seg { t, h } in &c.s {
        if *h {
            out.push_str("<mark>");
            esc(t, out);
            out.push_str("</mark>");
        } else {
            esc(t, out);
        }
    }
}

fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// A self-contained HTML page with every difference (and a few lines around it). Stops after
/// `limit` bytes and says so. `when` is the date and time to print in it.
pub fn report(c: &Comparison, when: &str, limit: usize) -> Result<String, String> {
    let s = c.summary();
    let mut out = String::with_capacity(64 * 1024);
    out.push_str("<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\n<title>");
    esc(&format!("{} vs {}", s.left.name, s.right.name), &mut out);
    out.push_str("</title>\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<style>");
    out.push_str(STYLE);
    out.push_str("</style></head><body>\n<header><h1>");
    esc(&s.left.name, &mut out);
    out.push_str(" <span>vs</span> ");
    esc(&s.right.name, &mut out);
    out.push_str("</h1>\n<dl><dt>Original</dt><dd>");
    esc(&s.left.path, &mut out);
    out.push_str("</dd><dt>New</dt><dd>");
    esc(&s.right.path, &mut out);
    out.push_str("</dd><dt>Compared</dt><dd>");
    esc(when, &mut out);
    out.push_str(" with SideBySide</dd></dl>\n<p class=\"sum\">");
    if s.blocks == 0 {
        out.push_str("<b>The files are identical.</b>");
    } else {
        out.push_str(&format!(
            "<b>{} {}</b> · <i class=\"c\"></i>{} changed · <i class=\"a\"></i>{} added · <i class=\"d\"></i>{} removed",
            fmt_num(s.blocks as u64),
            if s.blocks == 1 { "difference" } else { "differences" },
            fmt_num(s.changed),
            fmt_num(s.added),
            fmt_num(s.removed)
        ));
        if s.moved > 0 {
            out.push_str(&format!(" · <i class=\"m\"></i>{} moved", fmt_num(s.moved)));
        }
    }
    out.push_str("</p></header>\n<main>\n");
    let mut cut = false;
    for (n, h) in hunks(c).into_iter().enumerate() {
        if out.len() > limit {
            cut = true;
            break;
        }
        if n > 0 {
            out.push_str("<div class=\"gap\">⋯</div>\n");
        }
        out.push_str("<table>\n");
        for r in rows_of(c, h)? {
            let k = match r.k {
                1 => "c",
                2 => "a",
                3 => "d",
                5 | 6 => "m",
                _ => "s",
            };
            out.push_str(&format!("<tr class=\"{k}\"><td class=\"n\">"));
            if let Some(c) = &r.l {
                out.push_str(&c.n.to_string());
            }
            out.push_str("</td><td class=\"t\">");
            cell_html(&r.l, &mut out);
            out.push_str("</td><td class=\"n\">");
            if let Some(c) = &r.r {
                out.push_str(&c.n.to_string());
            }
            out.push_str("</td><td class=\"t\">");
            cell_html(&r.r, &mut out);
            out.push_str("</td></tr>\n");
        }
        out.push_str("</table>\n");
    }
    if cut {
        out.push_str("<p class=\"cut\">The report stops here: the full list of differences is too long for one page.</p>\n");
    }
    out.push_str("</main>\n<footer>Made with SideBySide by 92-bit · github.com/92-bit/SideBySide</footer>\n</body></html>\n");
    Ok(out)
}

const STYLE: &str = r#"
:root{--fg:#1f2330;--muted:#6b7183;--line:#e3e6ee;--add:#e6f6ea;--add-s:#16a34a;--del:#fde8ec;--del-s:#e11d48;--chg:#fdf4e3;--chg-s:#d97706;--mark:#fbd38d;--mv:#efe9fd;--mv-s:#7c3aed;color-scheme:light}
*{box-sizing:border-box}
body{margin:0;padding:32px clamp(16px,4vw,48px);font:14px/1.5 "Segoe UI",system-ui,sans-serif;color:var(--fg);background:#fff}
h1{margin:0 0 12px;font-size:22px;letter-spacing:-.01em}h1 span{color:var(--muted);font-weight:400}
dl{display:grid;grid-template-columns:max-content 1fr;gap:2px 12px;margin:0 0 12px;font-size:12.5px}dt{color:var(--muted)}dd{margin:0;word-break:break-all}
.sum{margin:0 0 20px;font-size:13px}.sum i{display:inline-block;width:8px;height:8px;border-radius:50%;margin:0 5px 0 4px}
i.c{background:var(--chg-s)}i.a{background:var(--add-s)}i.d{background:var(--del-s)}i.m{background:var(--mv-s)}
table{width:100%;border-collapse:collapse;table-layout:fixed;font:12.5px/1.55 "Cascadia Mono",Consolas,monospace;border:1px solid var(--line);border-radius:10px;overflow:hidden}
td{vertical-align:top;padding:0 8px;white-space:pre-wrap;overflow-wrap:anywhere}
td.n{width:4.5em;text-align:right;color:var(--muted);user-select:none;border-right:1px solid var(--line)}
td.t+td.n{border-left:1px solid var(--line)}
tr.c td{background:var(--chg)}tr.a td:nth-child(n+3){background:var(--add)}tr.d td:nth-child(-n+2){background:var(--del)}tr.m td{background:var(--mv)}
mark{background:var(--mark);border-radius:3px;color:inherit}
.gap{text-align:center;color:var(--muted);margin:6px 0}
.cut{color:var(--del-s)}
footer{margin-top:28px;color:var(--muted);font-size:12px}
@media print{body{padding:0}table{break-inside:auto}tr{break-inside:avoid}}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{compare, Control, Options};

    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    fn cmp(a: &str, b: &str) -> Comparison {
        let dir = std::env::temp_dir();
        let n = format!("{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
        let (pa, pb) = (dir.join(format!("sbs-export-{n}-a.txt")), dir.join(format!("sbs-export-{n}-b.txt")));
        std::fs::write(&pa, a).unwrap();
        std::fs::write(&pb, b).unwrap();
        compare(&pa, &pb, &Options::default(), &Control::default()).unwrap()
    }

    #[test]
    fn patch_is_a_unified_diff() {
        let a: String = (1..=12).map(|i| format!("line {i}\n")).collect();
        let b = a.replace("line 6\n", "line six\n").replace("line 12\n", "line 12\nline 13\n");
        let p = patch(&cmp(&a, &b), 1 << 20).unwrap();
        let body: Vec<&str> = p.lines().skip(2).collect();
        assert_eq!(
            body,
            vec![
                "@@ -3,10 +3,11 @@", " line 3", " line 4", " line 5", "-line 6", "+line six", " line 7", " line 8",
                " line 9", " line 10", " line 11", " line 12", "+line 13",
            ]
        );
    }

    #[test]
    fn report_is_html_with_the_differences() {
        let r = report(&cmp("a\n<b>\nc\n", "a\n<B>\nc\n"), "today", 1 << 20).unwrap();
        assert!(r.starts_with("<!doctype html>"));
        assert!(r.contains("1 difference</b>"));
        assert!(r.contains("&lt;<mark>b</mark>&gt;") || r.contains("<mark>&lt;b&gt;</mark>"), "{r}");
    }
}
