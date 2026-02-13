use anyhow::Context;
use fraud_common::config::AppConfig;
use std::path::Path;
use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_target(true)
        .init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        print_usage();
        std::process::exit(if args.len() < 2 { 1 } else { 0 });
    }

    let csv_path = &args[1];

    // Parse optional flags
    let mut config_path = "config/default.toml".to_string();
    let mut threshold_override: Option<f64> = None;
    let mut sweep_steps: usize = 20;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--threshold" => {
                i += 1;
                threshold_override = Some(
                    args.get(i)
                        .context("--threshold requires a value")?
                        .parse()
                        .context("invalid threshold value")?,
                );
            }
            "--sweep-steps" => {
                i += 1;
                sweep_steps = args
                    .get(i)
                    .context("--sweep-steps requires a value")?
                    .parse()
                    .context("invalid sweep-steps value")?;
            }
            "--config" => {
                i += 1;
                config_path = args
                    .get(i)
                    .context("--config requires a value")?
                    .clone();
            }
            other => {
                eprintln!("unknown argument: {other}");
                print_usage();
                std::process::exit(1);
            }
        }
        i += 1;
    }

    // ── Load configuration ───────────────────────────────────────────────
    let config = AppConfig::from_file(Path::new(&config_path))
        .context("failed to load configuration")?;

    let threshold = threshold_override.unwrap_or(config.scorer.alert_threshold);

    // ── Load dataset ─────────────────────────────────────────────────────
    println!("📂 Loading dataset from: {csv_path}");
    let dataset = fraud_replay::Dataset::from_csv(Path::new(csv_path))
        .context("failed to load dataset")?;

    println!(
        "   {} transactions ({} fraud, {} legit)",
        dataset.len(),
        dataset.fraud_count(),
        dataset.legit_count(),
    );

    if dataset.is_empty() {
        eprintln!("Dataset is empty, nothing to evaluate.");
        std::process::exit(1);
    }

    // ── Run replay ───────────────────────────────────────────────────────
    println!("\n🔄 Running replay through feature extractor + ensemble scorer ...");
    let mut engine = fraud_replay::ReplayEngine::new(&config.scorer);
    let predictions = engine.replay(&dataset);

    // ── Report at configured threshold ───────────────────────────────────
    println!();
    let report = fraud_replay::MetricsReport::from_predictions(&predictions, threshold);
    report.print();

    // ── Threshold sweep ──────────────────────────────────────────────────
    let sweep = fraud_replay::ThresholdSweep::run(&predictions, sweep_steps);
    sweep.print();

    println!("\n✅ Backtest complete.");

    Ok(())
}

fn print_usage() {
    eprintln!("Usage: backtest <dataset.csv> [OPTIONS]");
    eprintln!();
    eprintln!("Runs the fraud detection pipeline against a labeled CSV dataset");
    eprintln!("and reports precision / recall / F1 metrics.");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  <dataset.csv>              Path to the labeled CSV dataset");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --threshold <f64>          Classification threshold (default: from config)");
    eprintln!("  --sweep-steps <usize>      Number of steps for threshold sweep (default: 20)");
    eprintln!("  --config <path>            Path to config TOML (default: config/default.toml)");
    eprintln!("  -h, --help                 Print this help message");
}
