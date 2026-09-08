use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, value_parser};

use devclean::cleanup::execute::execute_cleanup_plan;
use devclean::cleanup::{CleanupType, build_cleanup_plan};
use devclean::error::{DevcleanError, Result};
use devclean::fs::{ScanOptions, scan_path_with_options};
use devclean::ui::spinner::Spinner;
use devclean::ui::{
    OutputFormat, ReportOptions, render_cleanup_execution, render_cleanup_plan, render_scan_report,
};

#[derive(Debug, Parser)]
#[command(
    name = "devclean",
    version,
    about = "Find and explicitly clean reclaimable development artifacts"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Scan for supported development artifacts without deleting anything.
    Scan {
        /// Directory to scan recursively.
        path: PathBuf,

        /// Print the largest individual artifacts after the summary.
        #[arg(long)]
        details: bool,

        /// Maximum number of individual artifacts to print with --details.
        #[arg(long, default_value_t = 20, value_parser = value_parser!(usize))]
        limit: usize,

        /// Filter artifacts older than specified days (e.g. 30, 30d, 2w).
        #[arg(long, value_parser = parse_duration_days)]
        older_than: Option<u64>,

        /// Output format (text or json).
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
    },
    /// Preview or execute cleanup for explicitly selected artifact types.
    #[command(
        after_help = "Safety: clean is a dry run unless --execute is supplied. At least one --type is required.\nSupported types: node_modules, target, .next, .nuxt, .gradle, .venv, venv, __pycache__, dist, build"
    )]
    Clean {
        /// Directory to scan recursively.
        path: PathBuf,

        /// Artifact type eligible for cleanup. Required; repeat to select multiple types.
        #[arg(long = "type", value_name = "TYPE")]
        artifact_types: Vec<CleanupType>,

        /// Filter artifacts older than specified days (e.g. 30, 30d, 2w).
        #[arg(long, value_parser = parse_duration_days)]
        older_than: Option<u64>,

        /// Delete planned regenerable artifacts. Without this flag, clean only prints a plan.
        #[arg(long)]
        execute: bool,
    },
    /// Print detailed version and target platform information.
    Version,
    /// Self-update devclean to the latest release binary.
    Update,
}

fn parse_duration_days(val: &str) -> std::result::Result<u64, String> {
    let val = val.trim();
    if let Some(days) = val.strip_suffix('d') {
        days.parse::<u64>()
            .map_err(|_| format!("invalid days format: '{val}'"))
    } else if let Some(weeks) = val.strip_suffix('w') {
        weeks
            .parse::<u64>()
            .map(|w| w * 7)
            .map_err(|_| format!("invalid weeks format: '{val}'"))
    } else {
        val.parse::<u64>()
            .map_err(|_| format!("invalid days format: '{val}'"))
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Scan {
            path,
            details,
            limit,
            older_than,
            format,
        } => {
            let options = ScanOptions {
                min_age_days: older_than,
                now: None,
            };
            let scan = scan_with_spinner(&path, &options)?;
            print!(
                "{}",
                render_scan_report(
                    &scan,
                    ReportOptions {
                        details,
                        detail_limit: limit,
                        home_dir: home_dir(),
                        now: None,
                        format,
                    },
                )
            );
        }
        Commands::Clean {
            path,
            artifact_types,
            older_than,
            execute,
        } => {
            if artifact_types.is_empty() {
                return Err(DevcleanError::msg(format!(
                    "clean requires at least one --type. Supported types: {}",
                    devclean::cleanup::supported_type_names_display()
                )));
            }

            let home_dir = home_dir();
            let options = ScanOptions {
                min_age_days: older_than,
                now: None,
            };
            let scan = scan_with_spinner(&path, &options)?;
            let plan = build_cleanup_plan(&scan, &artifact_types, &path, home_dir.as_deref());
            if plan.has_unsafe_entries() {
                print!("{}", render_cleanup_plan(&plan, home_dir.as_deref()));
                return Err(DevcleanError::msg("cleanup plan contains unsafe paths"));
            }

            if execute {
                let execution = execute_cleanup_plan(&plan, &path, home_dir.as_deref());
                print!(
                    "{}",
                    render_cleanup_execution(&plan, &execution, home_dir.as_deref())
                );
                if !execution.failed.is_empty() {
                    return Err(DevcleanError::msg(
                        "cleanup completed with deletion failures",
                    ));
                }
            } else {
                print!("{}", render_cleanup_plan(&plan, home_dir.as_deref()));
            }
        }
        Commands::Version => {
            println!(
                "devclean {} ({}-{})\nLicense: MIT\nRepository: https://github.com/engnhn/devclean",
                env!("CARGO_PKG_VERSION"),
                target_arch(),
                target_os()
            );
        }
        Commands::Update => {
            println!("==> Updating devclean to the latest release...");
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg("curl -fsSL https://raw.githubusercontent.com/engnhn/devclean/main/install.sh | sh")
                .status()
                .map_err(|e| DevcleanError::msg(format!("failed to execute update script: {e}")))?;

            if !status.success() {
                return Err(DevcleanError::msg("update process failed"));
            }
        }
    }

    Ok(())
}

fn target_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    }
}

fn target_os() -> &'static str {
    if cfg!(target_os = "linux") {
        "unknown-linux-gnu"
    } else {
        "unknown"
    }
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE");

    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");

    home.map(PathBuf::from).filter(|path| path.is_absolute())
}

fn scan_with_spinner(path: &Path, options: &ScanOptions) -> Result<devclean::fs::ScanResult> {
    let spinner = Spinner::scan(path);
    let scan = scan_path_with_options(path, options);
    drop(spinner);
    scan
}
