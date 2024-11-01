mod root;
mod util;

use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::{ffi, fs, io, process};

use anyhow::{bail, format_err, Context, Result};
use chrono::Utc;
use clap::{Args, Parser, Subcommand};
use rand::distributions::{Alphanumeric, DistString};
use root::Root;
use tracing::{debug, error, warn};
use tracing_subscriber::EnvFilter;

const LOG_TARGET: &str = "fs_dir_cache";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Opts {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Args)]
/// Acquire a lock on cache key subdir in a given cache root
/// directory. Waits if key is already locked.
struct LockOpts {
    /// Root cache dir
    ///
    /// Dir that will hold all the cache key subdirs
    #[arg(long, env = "FS_DIR_CACHE_ROOT")]
    root: PathBuf,

    /// An id of a lock to use for `unlock`
    #[arg(long, env = "FS_DIR_CACHE_LOCK_ID")]
    lock_id: String,

    /// Name of the cache
    ///
    /// Base part of the unique key identifying cache subdir
    #[arg(long, env = "FS_DIR_CACHE_KEY_NAME")]
    key_name: String,

    /// A string to hash into the final cache subdir id
    ///
    /// Can be passed multiple times (order is significant).
    #[arg(long)]
    key_str: Vec<String>,

    /// A path to a file to hash the content of into the final cache
    /// subdir id
    ///
    /// Can be passed multiple times (order is significant).
    #[arg(long)]
    key_file: Vec<PathBuf>,

    /// Unlock automatically after given amount of seconds, in case cleanup
    /// never happens
    #[arg(long)]
    #[arg(long, env = "FS_DIR_CACHE_LOCK_TIMEOUT_SECS")]
    timeout_secs: u64,
}

#[derive(Args, Debug)]
/// Unlock the cache key dir
struct UnlockOpts {
    /// Cache key dir
    #[arg(long)]
    dir: PathBuf,

    /// Lock used during `unlock`
    #[arg(long, env = "FS_DIR_CACHE_LOCK_ID")]
    lock_id: String,
}

#[derive(Args)]
/// Garbage collect cache keys
struct GC {
    /// Root cache dir
    #[arg(long, env = "FS_DIR_CACHE_ROOT")]
    root: PathBuf,

    #[command(subcommand)]
    mode: GCModeCommand,
}

#[derive(Args)]
struct ExecOpts {
    #[clap(flatten)]
    opts: LockOpts,

    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    exec: Vec<ffi::OsString>,
}

#[derive(Subcommand)]
enum Commands {
    Lock(LockOpts),
    Unlock(UnlockOpts),
    Exec(ExecOpts),
    GC(GC),
}

#[derive(Subcommand)]
enum GCModeCommand {
    /// Delete all cache subdirectories used last more than N
    Unused {
        #[arg(long)]
        seconds: u64,
    },
}

fn main() -> Result<()> {
    init_logging();
    let opts = Opts::parse();

    match opts.command {
        Commands::Lock(lock_opts) => println!("{}", lock(lock_opts, None)?.display()),
        Commands::Unlock(unlock_opts) => {
            unlock(unlock_opts)?;
        }
        Commands::GC(gc_options) => gc(gc_options)?,
        Commands::Exec(exec_opts) => run_exec(exec_opts)?,
    }

    Ok(())
}

fn run_exec(ExecOpts { opts, exec }: ExecOpts) -> Result<()> {
    if exec.is_empty() {
        bail!("Missing command");
    }
    let cmd_str = exec
        .join(&ffi::OsString::from(" "))
        .to_string_lossy()
        .to_string();

    let root = std::fs::canonicalize(&opts.root)?;

    let sock_path = root.join(PathBuf::from(format!(
        "lock-{}",
        Alphanumeric.sample_string(&mut rand::thread_rng(), 10)
    )));

    debug!(
        target: LOG_TARGET,
        sock_path = %sock_path.display(),
        "Binding liveness socket"
    );
    let _socket = UnixListener::bind(&sock_path)?;

    assert!(UnixStream::connect(&sock_path).is_ok());

    let exec_dir = lock(opts, Some(sock_path.clone()))?;

    fs::create_dir_all(&exec_dir)?;

    debug!(
        target: LOG_TARGET,
        cmd = ?exec, ?exec_dir, "Executing user command"
    );
    if !process::Command::new(&exec[0])
        .args(&exec[1..])
        .current_dir(exec_dir)
        .status()
        .context("Executing user command failed")?
        .success()
    {
        error!(cmd = %cmd_str, "User command failed");
        bail!("User command failed");
    }

    if let Err(err) = fs::remove_file(&sock_path) {
        warn!(%err, sock_path=%sock_path.display(), "Error removing liveness socket")
    }

    Ok(())
}

fn gc(gc_options: GC) -> Result<()> {
    match gc_options.mode {
        GCModeCommand::Unused { seconds } => {
            let mut root = Root::new(&gc_options.root)?;

            let now = Utc::now();
            let deadline = now
                .checked_sub_signed(chrono::Duration::seconds(
                    i64::try_from(seconds).map_err(|_e| anyhow::format_err!("Timeout overflow"))?,
                ))
                .ok_or_else(|| anyhow::format_err!("Timeout overflow"))?;

            debug!(
                target: LOG_TARGET,
                %now, %deadline, "Looking for unused keys"
            );

            root.with_lock(|root| {
                let mut data = root.load_data()?;

                let to_delete =  data
                    .keys
                    .iter()
                    .filter(|(key, v)| {
                        debug!(
                            target: LOG_TARGET,
                            key, last_locked = %v.last_lock, locked_until = %v.locked_until, "Checking key"
                        );
                        !v.is_locked(now) && v.is_last_used_before(deadline)
                    })
                    .map(|(k, _v)| k.to_owned()).collect::<Vec<_>>();

                  for key in to_delete   {
                    let key_dir = root.key_dir_path(&key);
                    if key_dir.try_exists()? {
                        debug!(
                            target: LOG_TARGET,
                            key_dir = %key_dir.display(), "Deleting key dir"
                        );
                        fs::remove_dir_all(&key_dir).with_context(|| "Failed to delete")?;
                    } else {
                        debug!(
                            target: LOG_TARGET,
                            key_dir = %key_dir.display(), "Does not exist"
                        )
                    }
                    data.keys.remove(&key);
                    root.store_data(&data)?;
                    println!("{}", key_dir.display());
                }

                Ok(())
            })
        }
    }
}

fn lock(lock_opts: LockOpts, socket_path: Option<PathBuf>) -> Result<PathBuf> {
    let mut root = Root::new(&lock_opts.root)?;

    let key = format!("{}-{}", lock_opts.key_name, get_cache_key(&lock_opts)?);
    root.with_lock(|root| {
        root.lock_key(
            &key,
            &lock_opts.lock_id,
            lock_opts.timeout_secs,
            socket_path,
        )
    })
}

fn unlock(unlock_opts: UnlockOpts) -> Result<()> {
    let (root_dir, key) = split_key_dir_path(&unlock_opts.dir)?;
    let mut root = Root::new(root_dir)?;

    root.with_lock(|root| root.unlock_key(&key, unlock_opts.lock_id))
}

fn split_key_dir_path(dir: &Path) -> Result<(PathBuf, String)> {
    let key = dir
        .file_name()
        .ok_or_else(|| format_err!("Path ends with invalid component: {}", dir.display()))?
        .to_str()
        .ok_or_else(|| format_err!("Path contains invalid characters: {}", dir.display()))?
        .to_owned();

    let parent = dir
        .parent()
        .ok_or_else(|| format_err!("Can't figure out parent: {}", dir.display()))?
        .to_owned();

    Ok((parent, key))
}

fn get_cache_key(lock_opts: &LockOpts) -> Result<String, anyhow::Error> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(lock_opts.key_name.as_bytes());
    for key_str in &lock_opts.key_str {
        hasher.update(key_str.as_bytes());
    }
    for key_file in &lock_opts.key_file {
        let mut reader = fs::File::open(key_file)
            .with_context(|| format!("Failed to open {}", key_file.display()))?;
        io::copy(&mut reader, &mut hasher)
            .with_context(|| format!("Failed to read {}", key_file.display()))?;
    }

    Ok(hasher.finalize().to_hex().to_string())
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
