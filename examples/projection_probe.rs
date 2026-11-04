//! Repeatable console benchmark for educational projection, no native window required.
//! cargo run --release --example projection_probe -- path/to/large.rs
#[allow(dead_code)]
#[path = "../src/parser.rs"]
mod parser;

use std::{env, fs, time::Instant};

fn median_ms(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args_os()
        .nth(1)
        .ok_or("usage: projection_probe <real .rs file>")?;
    let path = std::path::PathBuf::from(path);
    let source = fs::read_to_string(&path)?;
    let mut engine = parser::ProjectionEngine::new()?;
    let first_start = Instant::now();
    let first = engine
        .project(&source, &|| false)
        .ok_or("projection cancelled")?;
    let first_ms = first_start.elapsed().as_secs_f64() * 1_000.0;

    let mut full = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        let p = parser::project(&source);
        assert_eq!(p.anchors.len(), first.anchors.len());
        full.push(start.elapsed().as_secs_f64() * 1_000.0);
    }

    let mut incremental = Vec::new();
    for step in 0..5 {
        let edited = format!("{source}\n// rustbridge-probe-{step}");
        let start = Instant::now();
        let p = engine
            .project(&edited, &|| false)
            .ok_or("incremental projection cancelled")?;
        incremental.push(start.elapsed().as_secs_f64() * 1_000.0);
        assert!(!p.java.is_empty() || edited.is_empty());
    }

    println!("path={}", path.display());
    println!(
        "bytes={} lines={} anchors={}",
        source.len(),
        first.rust_line_starts.len(),
        first.anchors.len()
    );
    println!("cold_ms={first_ms:.3}");
    println!("full_median_ms={:.3}", median_ms(full));
    println!("incremental_median_ms={:.3}", median_ms(incremental));
    Ok(())
}
