//! Speed test for the comparing engine.
//! Run: cargo run --release --example bench -- <old file> <new file>
use std::path::Path;
use std::time::Instant;

use sidebyside_lib::engine::{compare, Control, Options};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: bench <old file> <new file>");
        std::process::exit(2);
    }
    let t = Instant::now();
    let c = compare(Path::new(&args[0]), Path::new(&args[1]), &Options::default(), &Control::default()).expect("compare failed");
    let s = c.summary();
    println!(
        "compared in {:.2}s | lines {} vs {} | differences {} (changed {}, added {}, removed {})",
        t.elapsed().as_secs_f64(), s.left.lines, s.right.lines, s.blocks, s.changed, s.added, s.removed
    );
    let t = Instant::now();
    let rows = c.rows(s.total_rows / 2, 60).unwrap();
    println!("60 rows from the middle fetched in {:.2} ms", t.elapsed().as_secs_f64() * 1000.0);
    let t = Instant::now();
    let hit = c.nav(s.total_rows / 2, 1);
    println!("next difference found in {:.3} ms: {:?}", t.elapsed().as_secs_f64() * 1000.0, hit);
    println!("sample row: {:?}", rows.first().map(|r| r.k));
    // pause so memory can be measured while the comparison is held
    std::thread::sleep(std::time::Duration::from_millis(1500));
}
