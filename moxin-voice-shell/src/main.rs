//! Moxin TTS - Standalone TTS Application
//!
//! A standalone desktop application for text-to-speech with voice cloning,
//! powered by GPT-SoVITS.

mod app;

use clap::Parser;

#[derive(Parser, Debug, Default, Clone)]
#[command(name = "moxin-voice")]
#[command(about = "Moxin TTS - Voice Cloning & Text-to-Speech")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Args {
    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    pub log_level: String,

    /// Dora dataflow YAML file path
    #[arg(short, long)]
    pub dataflow: Option<String>,
}

impl Args {
    /// Get the log filter string for env_logger
    pub fn log_filter(&self) -> &str {
        &self.log_level
    }
}

fn main() {
    // Parse command-line arguments
    let args = Args::parse();

    // Configure logging
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(args.log_filter()),
    )
    .init();

    log::info!("Starting Moxin TTS v{}", env!("CARGO_PKG_VERSION"));
    log::debug!("CLI args: {:?}", args);

    if let Some(ref dataflow) = args.dataflow {
        log::info!("Using dataflow: {}", dataflow);
    }

    // Store args for app access
    app::set_cli_args(args);

    // Start the application
    app::app_main();
}
