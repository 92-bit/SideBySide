//! Tidy up JSON and XML before comparing: one item per line, indented, so a file squashed onto
//! one line (or formatted differently) can be compared line by line.
//!
//! Both work as a stream (piece by piece), so even very big files need little memory. Nothing is
//! checked or changed except the spaces and line breaks between items: text inside quotes, numbers
//! and the order of everything stay exactly as they are. A file that isn't valid JSON / XML is
//! still tidied as well as possible, never refused.

use std::io::{self, Read, Write};

/// Which kind of file this is, from its name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Json,
    Xml,
}

pub fn kind_of(name: &str) -> Option<Kind> {
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    match ext.as_str() {
        "json" | "jsonc" | "json5" | "geojson" | "ipynb" | "har" | "webmanifest" => Some(Kind::Json),
        "xml" | "xsd" | "xsl" | "xslt" | "svg" | "xaml" | "csproj" | "vbproj" | "fsproj" | "props" | "targets"
        | "resx" | "config" | "plist" | "nuspec" | "wsdl" | "kml" | "gpx" | "rss" | "atom" => Some(Kind::Xml),
        _ => None,
    }
}

const INDENT: &[u8] = b"  ";
const PIECE: usize = 1 << 20;

fn newline(out: &mut impl Write, level: usize) -> io::Result<()> {
    out.write_all(b"\n")?;
    for _ in 0..level {
        out.write_all(INDENT)?;
    }
    Ok(())
}

/// Read in pieces; `step` gets every byte. `stop` is asked between pieces (Cancel).
fn each_byte(
    mut input: impl Read,
    stop: &dyn Fn() -> io::Result<()>,
    mut step: impl FnMut(u8) -> io::Result<()>,
) -> io::Result<()> {
    let mut buf = vec![0u8; PIECE];
    let mut first = true;
    loop {
        stop()?;
        let n = input.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        let mut piece = &buf[..n];
        if first && piece.starts_with(&[0xEF, 0xBB, 0xBF]) {
            piece = &piece[3..]; // the invisible UTF-8 marker at the start
        }
        first = false;
        for &b in piece {
            step(b)?;
        }
    }
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

pub fn json(input: impl Read, out: &mut impl Write, stop: &dyn Fn() -> io::Result<()>) -> io::Result<()> {
    #[derive(PartialEq)]
    enum In {
        Code,
        Str,
        StrEscape,
        /// Saw a "/": maybe a comment starts (JSON with comments).
        Slash,
        LineComment,
        BlockComment,
        BlockCommentStar,
    }
    let mut st = In::Code;
    let mut level = 0usize;
    // Just after "{" or "[": the line break waits, so an empty {} or [] stays on one line.
    let mut opened = false;
    let mut wrote = false;

    // Before writing the next item: the waiting line break.
    macro_rules! before {
        () => {
            if opened {
                newline(out, level)?;
                opened = false;
            }
        };
    }

    each_byte(input, stop, |b| {
        match st {
            In::Str => {
                out.write_all(&[b])?;
                if b == b'\\' {
                    st = In::StrEscape;
                } else if b == b'"' {
                    st = In::Code;
                }
            }
            In::StrEscape => {
                out.write_all(&[b])?;
                st = In::Str;
            }
            In::LineComment => {
                if b == b'\n' || b == b'\r' {
                    st = In::Code;
                    newline(out, level)?;
                } else {
                    out.write_all(&[b])?;
                }
            }
            In::BlockComment | In::BlockCommentStar => {
                out.write_all(&[b])?;
                st = if b == b'*' {
                    In::BlockCommentStar
                } else if b == b'/' && st == In::BlockCommentStar {
                    In::Code
                } else {
                    In::BlockComment
                };
            }
            In::Slash | In::Code => {
                if st == In::Slash {
                    st = In::Code;
                    match b {
                        b'/' => {
                            out.write_all(b"//")?;
                            st = In::LineComment;
                            return Ok(());
                        }
                        b'*' => {
                            out.write_all(b"/*")?;
                            st = In::BlockComment;
                            return Ok(());
                        }
                        _ => out.write_all(b"/")?,
                    }
                }
                match b {
                    b' ' | b'\t' | b'\r' | b'\n' => {}
                    b'/' => {
                        before!();
                        st = In::Slash;
                    }
                    b'"' => {
                        before!();
                        out.write_all(b"\"")?;
                        st = In::Str;
                    }
                    b'{' | b'[' => {
                        before!();
                        out.write_all(&[b])?;
                        level += 1;
                        opened = true;
                    }
                    b'}' | b']' => {
                        level = level.saturating_sub(1);
                        if opened {
                            opened = false; // empty: {} or []
                        } else {
                            newline(out, level)?;
                        }
                        out.write_all(&[b])?;
                    }
                    b',' => {
                        out.write_all(b",")?;
                        newline(out, level)?;
                    }
                    b':' => out.write_all(b": ")?,
                    _ => {
                        before!();
                        out.write_all(&[b])?;
                    }
                }
            }
        }
        wrote = true;
        Ok(())
    })?;
    if st == In::Slash {
        out.write_all(b"/")?;
    }
    if wrote {
        out.write_all(b"\n")?;
    }
    out.flush()
}

// ---------------------------------------------------------------------------
// XML
// ---------------------------------------------------------------------------

pub fn xml(input: impl Read, out: &mut impl Write, stop: &dyn Fn() -> io::Result<()>) -> io::Result<()> {
    let mut x = Xml::default();
    each_byte(input, stop, |b| x.byte(b, out))?;
    x.finish(out)?;
    out.flush()
}

#[derive(Default)]
struct Xml {
    /// The tag being read ("<...>"), or the text between tags.
    tag: Vec<u8>,
    in_tag: bool,
    /// Inside a tag: the quote character of an attribute value, if in one.
    quote: Option<u8>,
    text: Vec<u8>,
    level: usize,
    /// An opening tag that was just written: if only text and its closing tag follow,
    /// they stay on the same line (<name>Bob</name>).
    open_pending: bool,
    started: bool,
}

impl Xml {
    fn byte(&mut self, b: u8, out: &mut impl Write) -> io::Result<()> {
        if !self.in_tag {
            if b == b'<' {
                self.in_tag = true;
                self.tag.clear();
                self.tag.push(b);
            } else {
                self.text.push(b);
            }
            return Ok(());
        }
        self.tag.push(b);
        // Comments, CDATA and <? ?> end with their own marks; ">" inside them doesn't count.
        let t = &self.tag;
        let done = if t.starts_with(b"<!--") {
            t.len() >= 7 && t.ends_with(b"-->")
        } else if t.starts_with(b"<![CDATA[") {
            t.ends_with(b"]]>")
        } else if t.starts_with(b"<?") {
            t.len() >= 4 && t.ends_with(b"?>")
        } else {
            match self.quote {
                Some(q) if b == q => {
                    self.quote = None;
                    false
                }
                Some(_) => false,
                None if (b == b'"' || b == b'\'') && !t.starts_with(b"<!") => {
                    self.quote = Some(b);
                    false
                }
                None => b == b'>',
            }
        };
        if done {
            self.in_tag = false;
            let tag = std::mem::take(&mut self.tag);
            self.tag_done(&tag, out)?;
            self.tag = tag;
        }
        Ok(())
    }

    fn line(&mut self, out: &mut impl Write) -> io::Result<()> {
        if self.started {
            newline(out, self.level)
        } else {
            self.started = true;
            Ok(())
        }
    }

    fn tag_done(&mut self, tag: &[u8], out: &mut impl Write) -> io::Result<()> {
        let text = trim(&self.text).to_vec();
        self.text.clear();
        let closing = tag.starts_with(b"</");
        let special = tag.starts_with(b"<?") || tag.starts_with(b"<!");
        let self_closing = tag.ends_with(b"/>");

        if closing {
            self.level = self.level.saturating_sub(1);
            if self.open_pending {
                // <name>Bob</name> on one line
                out.write_all(&text)?;
                out.write_all(tag)?;
                self.open_pending = false;
                return Ok(());
            }
            if !text.is_empty() {
                self.level += 1;
                self.line(out)?;
                out.write_all(&text)?;
                self.level -= 1;
            }
            self.line(out)?;
            out.write_all(tag)?;
            return Ok(());
        }

        // Anything else ends a waiting "<name>": its text goes on its own line.
        if self.open_pending {
            self.open_pending = false;
        }
        if !text.is_empty() {
            self.line(out)?;
            out.write_all(&text)?;
        }
        self.line(out)?;
        out.write_all(tag)?;
        if !closing && !special && !self_closing {
            self.level += 1;
            self.open_pending = true;
        }
        Ok(())
    }

    fn finish(&mut self, out: &mut impl Write) -> io::Result<()> {
        if self.in_tag {
            // An unfinished tag at the end: keep it as text.
            let tag = std::mem::take(&mut self.tag);
            self.text.extend_from_slice(&tag);
        }
        let text = trim(&self.text).to_vec();
        if !text.is_empty() {
            self.line(out)?;
            out.write_all(&text)?;
        }
        if self.started {
            out.write_all(b"\n")?;
        }
        Ok(())
    }
}

fn trim(s: &[u8]) -> &[u8] {
    let is_space = |b: &u8| matches!(b, b' ' | b'\t' | b'\r' | b'\n');
    let start = s.iter().position(|b| !is_space(b)).unwrap_or(s.len());
    let end = s.iter().rposition(|b| !is_space(b)).map_or(start, |e| e + 1);
    &s[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(kind: Kind, s: &str) -> String {
        let mut out = Vec::new();
        let stop = || Ok(());
        match kind {
            Kind::Json => json(s.as_bytes(), &mut out, &stop).unwrap(),
            Kind::Xml => xml(s.as_bytes(), &mut out, &stop).unwrap(),
        }
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn json_one_item_per_line() {
        let got = run(Kind::Json, r#"{"name":"Side, \"By\" Side","n":[1,2.5e3,true,null],"e":{},"a":[]}"#);
        let want = "{\n  \"name\": \"Side, \\\"By\\\" Side\",\n  \"n\": [\n    1,\n    2.5e3,\n    true,\n    null\n  ],\n  \"e\": {},\n  \"a\": []\n}\n";
        assert_eq!(got, want);
        // already tidy, or tidied differently: the same result
        assert_eq!(run(Kind::Json, &want.replace("  ", "\t")), want);
    }

    #[test]
    fn json_comments_are_kept() {
        let got = run(Kind::Json, "{ // first\n\"a\": 1, /* two */ \"b\": 2 }");
        assert_eq!(got, "{\n  // first\n  \"a\": 1,\n  /* two */\"b\": 2\n}\n");
    }

    #[test]
    fn xml_tags_on_their_own_lines() {
        let got = run(
            Kind::Xml,
            "<?xml version=\"1.0\"?><root a=\"x > y\"><name>Bob</name><empty/><!-- note <b> --><list>\n  <i>1</i></list>tail</root>",
        );
        let want = "<?xml version=\"1.0\"?>\n<root a=\"x > y\">\n  <name>Bob</name>\n  <empty/>\n  <!-- note <b> -->\n  <list>\n    <i>1</i>\n  </list>\n  tail\n</root>\n";
        assert_eq!(got, want);
        assert_eq!(run(Kind::Xml, want), want);
    }

    #[test]
    fn kinds_by_name() {
        assert_eq!(kind_of("a.JSON"), Some(Kind::Json));
        assert_eq!(kind_of("App.config"), Some(Kind::Xml));
        assert_eq!(kind_of("notes.txt"), None);
        assert_eq!(kind_of("Makefile"), None);
    }
}
