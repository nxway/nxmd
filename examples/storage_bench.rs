//! Repeatable storage workload; deliberately not an editor latency benchmark.
use nxmd::vault::Vault;
use std::{fs, path::Path, time::Instant};
fn main() -> anyhow::Result<()> {
    let d = tempfile::tempdir()?;
    let v = Vault::open(d.path())?;
    for n in 0..5000 {
        fs::write(d.path().join(format!("{n:04}.md")), "# Note\n")?;
    }
    let start = Instant::now();
    let entries = v.entries(Path::new(""))?;
    println!(
        "5000-note directory listing: {:?}; entries={}",
        start.elapsed(),
        entries.len()
    );
    let text = "# Unicode 日本語\n\n**Bold** and [link](a.md).\n".repeat(25000);
    fs::write(d.path().join("large.md"), &text)?;
    let start = Instant::now();
    let mut doc = v.load(Path::new("large.md"))?;
    println!("load {} UTF-8 bytes: {:?}", text.len(), start.elapsed());
    doc.text.push_str("\nEdit\n");
    let start = Instant::now();
    v.save(&mut doc)?;
    println!("conflict-checked save: {:?}", start.elapsed());
    Ok(())
}
