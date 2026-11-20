//! Command-line mode: compare two files without opening a window, for scripts.
//!
//!   sidebyside old.txt new.txt --summary
//!   sidebyside old.txt new.txt --patch > changes.diff
//!   sidebyside old.txt new.txt --report changes.html --ignore-times
//!
//! The exit code says what was found, like the classic "diff" tool: 0 = the files are the same,
//! 1 = they differ, 2 = something went wrong (a file is missing, an option is wrong...).

use std::io::Write;
use std::path::PathBuf;

use crate::engine::{compare, Control, Options};
use crate::export;

/// True if the command line asks for command-line mode (not the window).
pub fn wanted(args: &[String]) -> bool {
    args.iter().skip(1).any(|a| matches!(a.as_str(), "--summary" | "--patch" | "--report" | "--help" | "-h" | "/?"))
}

const HELP: &str = "SideBySide: compare two text files.

  sidebyside OLD NEW              open them in the window
  sidebyside OLD NEW --summary    print how many lines differ
  sidebyside OLD NEW --patch [FILE]
                                  print the differences as a patch (unified diff),
                                  or save them to FILE
  sidebyside OLD NEW --report FILE
                                  save an HTML report (opens in any browser)

Options:
  --ignore-spaces        lines that only differ in spaces or tabs count as the same
  --ignore-case          UPPER and lower case count as the same
  --ignore-blank-lines   empty lines don't count
  --ignore-times         dates and times in lines don't count (handy for logs)
  --ignore REGEX         parts of lines matching REGEX don't count (can be repeated)
  --tidy                 tidy up JSON and XML first (one item per line)
  --old-lines A-B        compare only lines A to B of OLD
  --new-lines A-B        compare only lines A to B of NEW

Exit code: 0 = the same, 1 = different, 2 = something went wrong.
In PowerShell, add \"| Write-Output\" so it waits and shows the result.
";

/// Runs command-line mode. Returns the exit code.
pub fn run(args: &[String]) -> i32 {
    attach_console();
    match go(args) {
        Ok(same) => {
            if same { 0 } else { 1 }
        }
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "SideBySide: {e}");
            2
        }
    }
}

enum Out {
    Summary,
    Patch(Option<PathBuf>),
    Report(PathBuf),
}

fn range(s: &str) -> Result<(u32, u32), String> {
    let bad = || format!("\"{s}\" isn't a line range like 10-250.");
    let (a, b) = s.split_once('-').ok_or_else(bad)?;
    let (a, b) = (a.trim().parse::<u32>().map_err(|_| bad())?, b.trim().parse::<u32>().map_err(|_| bad())?);
    if a == 0 || b < a {
        return Err(bad());
    }
    Ok((a, b))
}

fn go(args: &[String]) -> Result<bool, String> {
    let mut files = Vec::new();
    let mut opts = Options::default();
    let mut out = Vec::new();
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut it = args.iter().skip(1).peekable();
    while let Some(a) = it.next() {
        let mut value = |what: &str| it.next().cloned().ok_or_else(|| format!("{a} needs {what} after it."));
        match a.as_str() {
            "--help" | "-h" | "/?" => {
                print!("{HELP}");
                return Ok(true);
            }
            "--summary" => out.push(Out::Summary),
            "--patch" => {
                // an optional file name (not another option)
                let file = it.next_if(|n| !n.starts_with('-')).map(|f| cwd.join(f));
                out.push(Out::Patch(file));
            }
            "--report" => out.push(Out::Report(cwd.join(value("a file name")?))),
            "--ignore-spaces" => opts.ignore_spaces = true,
            "--ignore-case" => opts.ignore_case = true,
            "--ignore-blank-lines" => opts.ignore_blank_lines = true,
            "--ignore-times" => opts.ignore_rules.push(TIMES.into()),
            "--ignore" => opts.ignore_rules.push(value("a regular expression")?),
            "--tidy" => opts.tidy = true,
            "--old-lines" => opts.left_lines = Some(range(&value("a line range")?)?),
            "--new-lines" => opts.right_lines = Some(range(&value("a line range")?)?),
            s if s.starts_with("--") => return Err(format!("\"{s}\" isn't an option SideBySide knows. Try --help.")),
            f => files.push(cwd.join(f)),
        }
    }
    let [old, new] = files.as_slice() else {
        return Err("give two files to compare (try --help).".into());
    };
    if out.is_empty() {
        out.push(Out::Summary);
    }
    let c = compare(old, new, &opts, &Control::default())?;
    let s = c.summary();
    for o in out {
        match o {
            Out::Summary => {
                if s.blocks == 0 {
                    println!("{} and {} are the same.", s.left.name, s.right.name);
                } else {
                    let mut line = format!(
                        "{} vs {}: {} {} ({} changed, {} added, {} removed",
                        s.left.name,
                        s.right.name,
                        s.blocks,
                        if s.blocks == 1 { "difference" } else { "differences" },
                        s.changed,
                        s.added,
                        s.removed
                    );
                    if s.moved > 0 {
                        line.push_str(&format!(", {} moved", s.moved));
                    }
                    println!("{line} lines)");
                }
            }
            Out::Patch(file) => {
                let p = export::patch(&c, usize::MAX)?;
                match file {
                    Some(f) => std::fs::write(&f, p).map_err(|e| format!("could not save {}: {e}", f.display()))?,
                    None => print!("{p}"),
                }
            }
            Out::Report(f) => {
                let html = export::report(&c, &now(), usize::MAX)?;
                std::fs::write(&f, html).map_err(|e| format!("could not save {}: {e}", f.display()))?;
                println!("Report saved: {}", f.display());
            }
        }
    }
    let _ = std::io::stdout().flush();
    Ok(s.blocks == 0)
}

/// The same rule as "Dates and times" in the app (src/lib/rules.ts).
const TIMES: &str = r"\d{4}[-/.]\d{1,2}[-/.]\d{1,2}(?:[T ]\d{1,2}:\d{2}(?::\d{2})?(?:[.,]\d+)?(?:Z|[+-]\d{2}:?\d{2})?)?|\d{1,2}[-/.]\d{1,2}[-/.]\d{4}|\d{1,2}:\d{2}:\d{2}(?:[.,]\d+)?";

/// The date and time now, for the report (UTC; there's no time zone library in the app).
fn now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rest) = (secs / 86400, secs % 86400);
    // Days since 1970 to a calendar date (Howard Hinnant's method).
    let z = days as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y}-{m:02}-{d:02} {:02}:{:02} UTC", rest / 3600, rest / 60 % 60)
}

/// The window app has no console of its own: borrow the one it was started from, so the
/// results show there. (Not needed when the output goes into a file or a pipe.)
#[cfg(windows)]
fn attach_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE};
    unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        if h.is_null() || h == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}
#[cfg(not(windows))]
fn attach_console() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_and_exit_codes() {
        let dir = std::env::temp_dir();
        let n = std::process::id();
        let (a, b) = (dir.join(format!("sbs-cli-{n}-a.log")), dir.join(format!("sbs-cli-{n}-b.log")));
        std::fs::write(&a, "10:00:01 start\n10:00:02 done\n").unwrap();
        std::fs::write(&b, "11:30:15 start\n11:30:16 done\n").unwrap();
        let args = |extra: &[&str]| -> Vec<String> {
            ["sidebyside", a.to_str().unwrap(), b.to_str().unwrap()].iter().chain(extra).map(|s| s.to_string()).collect()
        };
        assert!(wanted(&args(&["--summary"])));
        assert!(!wanted(&args(&[])));
        assert_eq!(go(&args(&["--summary"])), Ok(false)); // the times differ
        assert_eq!(go(&args(&["--summary", "--ignore-times"])), Ok(true));
        assert!(go(&args(&["--summary", "--nope"])).unwrap_err().contains("--nope"));
        assert!(go(&args(&["--old-lines", "5-2", "--summary"])).is_err());
        assert_eq!(range("10-250"), Ok((10, 250)));
        assert!(now().ends_with("UTC"));
    }
}
