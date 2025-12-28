use clap::Parser;
use color_eyre::eyre::{Result, WrapErr};
use confique::Config;
use std::fmt::Debug;
use std::path::PathBuf;
use std::fs;

mod library;
use library::{Library, SortPolicy};

use tracing::{debug, debug_span, info, instrument};
use tracing_error::ErrorLayer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Parser)]
struct Cli {
    /// Path to configuration file.
    #[arg(long, default_value = "po.toml")]
    config: PathBuf,

    // Clap <-> Confique integration to let cli args be used as config attrs
    #[command(flatten)]
    cli_config: <AppConfig as Config>::Layer,
}

#[derive(Config, Debug)]
#[config(layer_attr(derive(clap::Args)))]
struct AppConfig {
    /// Input paths, not searched recursively
    #[config(layer_attr(arg(long)))]
    inputs: Vec<PathBuf>,

    /// Output root
    #[config(layer_attr(arg(long)))]
    output: PathBuf,

    /// Extensions to capture within the input paths
    #[config(layer_attr(arg(long)))]
    extensions: Vec<String>,

    /// The policy to use when organising files
    #[config(layer_attr(arg(long)))]
    sort_policy: SortPolicy
}

fn init_logging() -> Result<()> {
    color_eyre::install()?;

    let timer = time::format_description::parse(
        "[year]-[month padding:zero]-[day padding:zero] [hour]:[minute]:[second]",
    )
        .expect("time format to be valid");
    
    let time_offset = time::UtcOffset::current_local_offset()
        .unwrap_or(time::UtcOffset::UTC);
    let timer = fmt::time::OffsetTime::new(time_offset, timer);

    let fmt_layer = fmt::layer()
        .with_ansi(true)
        .with_level(true)
        .with_target(false)
        .with_thread_names(false)
        .with_timer(timer)
        .compact();

    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("debug"))
        .unwrap();

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .with(ErrorLayer::default())
        .try_init()?;

    Ok(())
}

#[instrument]
fn ensure_directory(path: &PathBuf) -> Result<()> {
    debug!("ensuring path");
    
    if !path.exists() {
        debug!("path did not exist, creating it");
        fs::create_dir_all(path)?;
    }

    Ok(())
}

#[instrument]
fn search_input_path(input: &PathBuf, extensions: &[String]) -> Result<Vec<PathBuf>> {
    info!("searching input");

    let mut captured = vec![];
    
    let paths = fs::read_dir(input)?;
    for path in paths {
        let p = path?.path();
        let span = debug_span!("file_filter", file = p.to_str());
        let _enter = span.enter();
        
        let ext = p.extension();
        if let Some(ext) = ext {
            if extensions.contains(&ext.to_string_lossy().to_string()) {
                debug!("capturing file");
                captured.push(p);
            } else {
                debug!("ignoring file");
            }
        } else {
            debug!("no extension for file");
        }
    }
    
    debug!("captured {} files", captured.len());
    Ok(captured)
}

fn main() -> Result<()> {
    init_logging()?;
    
    info!("starting up!");

    let cli = Cli::parse();
    let config = AppConfig::builder()
        .preloaded(cli.cli_config)
        .file(cli.config)
        .load()
        .wrap_err("failed to load app config")?;

    info!("config loaded: {:#?}", config);

    for input in &config.inputs {
        ensure_directory(input)?;
    }
    
    ensure_directory(&config.output)?;
    info!("searching inputs for files");

    let mut captured = vec![];
    for input in &config.inputs {
        captured.extend(search_input_path(input, &config.extensions)?);
    }

    info!("captured {} files from {} inputs", captured.len(), config.inputs.len());

    let mut library = Library::read_from_disk(config.output.clone())?;
    
    debug!("loaded library: {:#?}", library);
    let new_files = library.process_inputs(&captured)?;
    
    info!("got {} new files: {:#?}", new_files.len(), new_files);
    library.sort_files(new_files, config.sort_policy.clone())?;
    
    library.persist_to_disk()?;
    
    Ok(())
}
