use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create or reuse a cache subdirectory in a given cache root. Wait if
    /// already locked.
    Lock {
        /// Root cache dir
        ///
        /// Directory that will hold all the cache subdirectories
        #[arg(long, env = "FS_DIR_CACHE_ROOT")]
        root: PathBuf,

        /// An id of a lock to use for `unlock`
        #[arg(long, env = "FS_DIR_CACHE_LOCK_ID")]
        lock_id: String,

        /// Name of the cache
        ///
        /// Base part of the unique key identifying cache subdirectory
        #[arg(long, env = "FS_DIR_CACHE_KEY_NAME")]
        key_name: String,

        /// A string to hash into the final cache subdirectory id
        ///
        /// Can be passed multiple times. Not guaranteed to be stable
        /// across `fs-dir-cache` versions.
        #[arg(long)]
        key_str: Vec<String>,

        /// Unlock automatically after given amount of seconds, in case cleanup
        /// never happens
        #[arg(long)]
        #[arg(long, env = "FS_DIR_CACHE_LOCK_TIMEOUT_SECS")]
        timeout_secs: u32,
    },

    /// Unlock the cache subdirectory
    Unlock {
        /// Cache directory
        #[arg(long)]
        dir: String,

        /// Lock used during `unlock`
        #[arg(long, env = "FS_DIR_CACHE_LOCK_ID")]
        lock_id: String,
    },
    /// Garbage collect
    GC {
        /// Root cache dir
        #[arg(long, env = "FS_DIR_CACHE_ROOT")]
        root: PathBuf,

        #[command(subcommand)]
        mode: GCModeCommand,
    },
}

#[derive(Subcommand)]
enum GCModeCommand {
    /// Delete all cache subdirectories used last more than N
    Unused {
        #[arg(long)]
        seconds: u64,
    },
}

fn main() {
    init_logging();
    let _cli = Cli::parse();

    todo!();
}

fn init_logging() {
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stderr) // Print to stderr
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");
}
