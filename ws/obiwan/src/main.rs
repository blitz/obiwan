use anyhow::{anyhow, Context, Result};
use clap::Parser;
use log::{debug, info};

/// A simple TFTP server for PXE booting
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Silence all output.
    #[structopt(short = 'q')]
    quiet: bool,

    /// Verbose mode. Specify multiple times to increase verbosity.
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Enable timestampts
    #[arg(short = 't', long)]
    timestamps: bool,

    /// The user to drop privileges to when started as root.
    #[arg(long, default_value = "nobody")]
    unprivileged_user: String,

    /// The port to listen on.
    #[arg(short = 'p', long, default_value = "69")]
    port: u16,
}

fn drop_privileges_if_root(unprivileged_user: &str) -> Result<()> {
    use nix::unistd::{geteuid, setuid, User};

    if !geteuid().is_root() {
        info!("Not running as root. Good!");
        return Ok(());
    }

    setuid(
        User::from_name(unprivileged_user)
            .context("Failed to lookup unprivileged user")?
            .ok_or_else(|| anyhow!("Failed to look up unprivileged user. Does it exist?"))?
            .uid,
    )
    .context("Failed to drop privileges")?;

    info!("Dropped privileges to user '{}'.", unprivileged_user);
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    stderrlog::new()
        .module(module_path!())
        .quiet(args.quiet)
        .verbosity(usize::from(1 + args.verbose)) // Default to printing warnings and errors
        .timestamp(if args.timestamps {
            stderrlog::Timestamp::Microsecond
        } else {
            stderrlog::Timestamp::Off
        })
        .init()?;

    info!("Hello!");
    debug!("Command line parameters: {:?}", args);

    drop_privileges_if_root(&args.unprivileged_user)?;

    Ok(())
}
