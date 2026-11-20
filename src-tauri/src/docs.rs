//! Office files and PDFs: their text is taken out (one paragraph, table row or PDF text line per
//! line), so two versions of a document can be compared like text files.
//!
//! Word (.docx), Excel (.xlsx), PowerPoint (.pptx) and OpenDocument (.odt, .ods, .odp) files are
//! zip archives with XML inside; the XML is read with a small tag reader (only the text matters
//! here, not the layout). PDF text comes from the lopdf library.

use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Doc {
    Word,
    Excel,
    PowerPoint,
    OpenDocument,
    Pdf,
}

pub fn kind_of(name: &str) -> Option<Doc> {
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "docx" | "docm" | "dotx" => Doc::Word,
        "xlsx" | "xlsm" => Doc::Excel,
        "pptx" | "pptm" => Doc::PowerPoint,
        "odt" | "ods" | "odp" => Doc::OpenDocument,
        "pdf" => Doc::Pdf,
        _ => return None,
    })
}

/// The text of a document, as lines.
pub fn text_of(path: &Path, kind: Doc) -> Result<String, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let bad = |e: String| format!("Could not read the text in \"{name}\": {e}");
    match kind {
        Doc::Pdf => pdf(path).map_err(bad),
        _ => {
            let file = std::fs::File::open(path).map_err(|e| bad(e.to_string()))?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| bad(format!("it isn't a valid {kind:?} file ({e})")))?;
            match kind {
                Doc::Word => word(&mut zip),
                Doc::Excel => excel(&mut zip),
                Doc::PowerPoint => slides(&mut zip),
                _ => open_document(&mut zip),
            }
            .map_err(bad)
        }
    }
}

type Zip = zip::ZipArchive<std::fs::File>;

fn read_entry(zip: &mut Zip, name: &str) -> Result<Option<String>, String> {
    let mut f = match zip.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let mut s = String::new();
    f.read_to_string(&mut s).map_err(|e| e.to_string())?;
    Ok(Some(s))
}

/// Entries whose names start with `prefix` and end with `.xml`, in number order (sheet2 before sheet10).
fn numbered(zip: &Zip, prefix: &str) -> Vec<String> {
    let mut v: Vec<String> = zip
        .file_names()
        .filter_map(|n| n.ok().map(|n| n.into_owned()))
        .filter(|n| n.starts_with(prefix) && n.ends_with(".xml"))
        .collect();
    let num = |s: &str| s[prefix.len()..].trim_end_matches(".xml").parse::<u32>().unwrap_or(u32::MAX);
    v.sort_by_key(|s| (num(s), s.clone()));
    v
}

// ---------------------------------------------------------------------------
// A small XML tag reader: enough to follow the text in Office files.
// ---------------------------------------------------------------------------

/// What the reader meets: a tag (its name without the namespace and whether it closes) or text.
enum Item<'a> {
    Open(&'a str, &'a str),
    Close(&'a str),
    /// A tag that opens and closes at once (<w:tab/>)
    Empty(&'a str, &'a str),
    Text(&'a str),
}

fn items(xml: &str) -> impl Iterator<Item = Item<'_>> {
    let mut rest = xml;
    std::iter::from_fn(move || {
        loop {
            if rest.is_empty() {
                return None;
            }
            if let Some(after) = rest.strip_prefix('<') {
                let end = after.find('>')?;
                let tag = &after[..end];
                rest = &after[end + 1..];
                if tag.starts_with('?') || tag.starts_with('!') {
                    continue; // <?xml ...?>, comments
                }
                fn local(t: &str) -> &str {
                    let n = t.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
                    n.rsplit(':').next().unwrap_or(n)
                }
                if let Some(t) = tag.strip_prefix('/') {
                    return Some(Item::Close(local(t)));
                }
                let attrs = tag.split_once(char::is_whitespace).map_or("", |(_, a)| a);
                return Some(if tag.ends_with('/') { Item::Empty(local(tag), attrs) } else { Item::Open(local(tag), attrs) });
            }
            let end = rest.find('<').unwrap_or(rest.len());
            let t = &rest[..end];
            rest = &rest[end..];
            return Some(Item::Text(t));
        }
    })
}

/// &amp; &lt; &#233; ... in XML text
fn unescape(t: &str, out: &mut String) {
    let mut rest = t;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let Some(j) = after.find(';') else {
            out.push_str(&rest[i..]);
            return;
        };
        let ent = &after[..j];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => ent
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => out.push(c),
            None => {
                out.push('&');
                out.push_str(ent);
                out.push(';');
            }
        }
        rest = &after[j + 1..];
    }
    out.push_str(rest);
}

/// The value of an attribute (`name="value"`) in a tag's attributes.
fn attr<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = attrs;
    while let Some(i) = rest.find(name) {
        let after = &rest[i + name.len()..];
        let before_ok = i == 0 || rest.as_bytes()[i - 1].is_ascii_whitespace() || rest.as_bytes()[i - 1] == b':';
        if before_ok && let Some(v) = after.strip_prefix("=\"") {
            return v.find('"').map(|e| &v[..e]);
        }
        rest = after;
    }
    None
}

/// Text in `text_tag` elements; a new line at the end of each `para_tag`; tabs and line breaks kept.
fn paragraphs(xml: &str, para_tag: &str, text_tag: &str, out: &mut String) {
    let mut in_text = false;
    let mut line = String::new();
    for it in items(xml) {
        match it {
            Item::Open(t, _) if t == text_tag => in_text = true,
            Item::Close(t) if t == text_tag => in_text = false,
            Item::Text(s) if in_text => unescape(s, &mut line),
            Item::Empty("tab", _) => line.push('\t'),
            Item::Empty("br", _) | Item::Empty("line-break", _) => {
                out.push_str(line.trim_end());
                out.push('\n');
                line.clear();
            }
            Item::Close(t) if t == para_tag => {
                out.push_str(line.trim_end());
                out.push('\n');
                line.clear();
            }
            _ => {}
        }
    }
    if !line.is_empty() {
        out.push_str(&line);
        out.push('\n');
    }
}

fn word(zip: &mut Zip) -> Result<String, String> {
    let xml = read_entry(zip, "word/document.xml")?.ok_or("there's no document text in it")?;
    let mut out = String::new();
    paragraphs(&xml, "p", "t", &mut out);
    // Footnotes and endnotes follow, under a heading of their own.
    for (part, title) in [("word/footnotes.xml", "Footnotes"), ("word/endnotes.xml", "Endnotes")] {
        if let Some(x) = read_entry(zip, part)? {
            let mut notes = String::new();
            paragraphs(&x, "p", "t", &mut notes);
            if !notes.trim().is_empty() {
                out.push_str(&format!("\n## {title}\n{notes}"));
            }
        }
    }
    Ok(out)
}

fn slides(zip: &mut Zip) -> Result<String, String> {
    let mut out = String::new();
    for (n, name) in numbered(zip, "ppt/slides/slide").into_iter().enumerate() {
        let xml = read_entry(zip, &name)?.unwrap_or_default();
        out.push_str(&format!("## Slide {}\n", n + 1));
        paragraphs(&xml, "p", "t", &mut out);
    }
    Ok(out)
}

fn open_document(zip: &mut Zip) -> Result<String, String> {
    let xml = read_entry(zip, "content.xml")?.ok_or("there's no content in it")?;
    let mut out = String::new();
    // Text in <text:p> and <text:h>; table cells end with a tab, table rows with a new line.
    let mut line = String::new();
    let mut depth = 0;
    for it in items(&xml) {
        match it {
            Item::Open("p" | "h", _) => depth += 1,
            Item::Close("p" | "h") => {
                depth -= 1;
                if depth == 0 {
                    line.push('\t');
                }
            }
            Item::Text(s) if depth > 0 => unescape(s, &mut line),
            Item::Empty("tab", _) => line.push('\t'),
            Item::Close("table-row") | Item::Close("frame") => {
                out.push_str(line.trim_end());
                out.push('\n');
                line.clear();
            }
            Item::Close("table-cell") => {}
            _ => {
                // a paragraph outside a table ends its line
                if depth == 0 && line.ends_with('\t') && !matches!(it, Item::Open("table-cell", _) | Item::Close("table-cell")) {
                    out.push_str(line.trim_end());
                    out.push('\n');
                    line.clear();
                }
            }
        }
    }
    if !line.trim().is_empty() {
        out.push_str(line.trim_end());
        out.push('\n');
    }
    Ok(out)
}

/// "B12" → column number 2 (counting from 1)
fn column(cell: &str) -> usize {
    cell.bytes().take_while(u8::is_ascii_alphabetic).fold(0, |n, c| n * 26 + (c.to_ascii_uppercase() - b'A' + 1) as usize)
}

fn excel(zip: &mut Zip) -> Result<String, String> {
    // Shared texts (cells point to them by number).
    let mut shared = Vec::new();
    if let Some(xml) = read_entry(zip, "xl/sharedStrings.xml")? {
        let mut cur: Option<String> = None;
        let mut in_t = false;
        for it in items(&xml) {
            match it {
                Item::Open("si", _) => cur = Some(String::new()),
                Item::Close("si") => shared.push(cur.take().unwrap_or_default()),
                Item::Open("t", _) => in_t = true,
                Item::Close("t") => in_t = false,
                Item::Text(s) if in_t => {
                    if let Some(c) = cur.as_mut() {
                        unescape(s, c);
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = String::new();
    for (n, name) in numbered(zip, "xl/worksheets/sheet").into_iter().enumerate() {
        let xml = read_entry(zip, &name)?.unwrap_or_default();
        out.push_str(&format!("## Sheet {}\n", n + 1));
        let mut cells: Vec<String> = Vec::new();
        let (mut col, mut typ) = (0usize, String::new());
        let (mut in_v, mut value) = (false, String::new());
        for it in items(&xml) {
            match it {
                Item::Open("row", _) => cells.clear(),
                Item::Close("row") => {
                    out.push_str(cells.join("\t").trim_end());
                    out.push('\n');
                }
                Item::Open("c", a) | Item::Empty("c", a) => {
                    col = attr(a, "r").map(column).unwrap_or(cells.len() + 1);
                    typ = attr(a, "t").unwrap_or("").to_string();
                    value.clear();
                }
                Item::Open("v" | "t", _) => in_v = true,
                Item::Close("v" | "t") => in_v = false,
                Item::Text(s) if in_v => unescape(s, &mut value),
                Item::Close("c") => {
                    let shown = if typ == "s" {
                        value.trim().parse::<usize>().ok().and_then(|i| shared.get(i).cloned()).unwrap_or_default()
                    } else if typ == "b" {
                        (if value.trim() == "1" { "TRUE" } else { "FALSE" }).to_string()
                    } else {
                        value.clone()
                    };
                    while cells.len() + 1 < col {
                        cells.push(String::new()); // empty cells in between
                    }
                    cells.push(shown.replace(['\t', '\n', '\r'], " "));
                }
                _ => {}
            }
        }
    }
    Ok(out)
}

fn pdf(path: &Path) -> Result<String, String> {
    let doc = lopdf::Document::load(path).map_err(|e| format!("it isn't a PDF that can be read ({e})"))?;
    let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
    let mut out = String::new();
    for p in pages {
        out.push_str(&format!("## Page {p}\n"));
        match doc.extract_text(&[p]) {
            Ok(t) => {
                for line in t.lines() {
                    out.push_str(line.trim_end());
                    out.push('\n');
                }
            }
            Err(_) => out.push_str("(the text on this page couldn't be read)\n"),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn office(name: &str, parts: &[(&str, &str)]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("sbs-docs-{}-{name}", std::process::id()));
        let mut z = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
        for (n, body) in parts {
            z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        }
        z.finish().unwrap();
        p
    }

    #[test]
    fn word_paragraphs_become_lines() {
        let p = office(
            "a.docx",
            &[(
                "word/document.xml",
                r#"<?xml version="1.0"?><w:document><w:body><w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:t xml:space="preserve"> world &amp; you</w:t></w:r></w:p><w:p><w:r><w:t>A</w:t><w:tab/><w:t>B</w:t></w:r></w:p></w:body></w:document>"#,
            )],
        );
        assert_eq!(text_of(&p, Doc::Word).unwrap(), "Hello world & you\nA\tB\n");
    }

    #[test]
    fn excel_rows_become_lines() {
        let p = office(
            "b.xlsx",
            &[
                ("xl/sharedStrings.xml", "<sst><si><t>Name</t></si><si><t>Bob</t></si></sst>"),
                (
                    "xl/worksheets/sheet1.xml",
                    r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="C1"><v>42</v></c></row><row r="2"><c r="A2" t="s"><v>1</v></c><c r="B2" t="b"><v>1</v></c></row></sheetData></worksheet>"#,
                ),
            ],
        );
        assert_eq!(text_of(&p, Doc::Excel).unwrap(), "## Sheet 1\nName\t\t42\nBob\tTRUE\n");
        assert_eq!(column("AB7"), 28);
        assert_eq!(kind_of("Report.DOCX"), Some(Doc::Word));
        assert_eq!(kind_of("notes.txt"), None);
    }
}
