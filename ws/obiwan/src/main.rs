use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use log::{debug, error, info, warn};

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

    /// The address to listen on.
    #[arg(short = 'l', long, default_value = "127.0.0.1:69")]
    listen_address: String,

    /// The directory to serve via TFTP.
    directory: PathBuf,
}

/// Try to revoke privileges. This may or may not succeed depending on
/// our privileges.
///
/// The returned path is refers to the passed directory and is
/// modified depending on whether we managed to actually change to a
/// new root directory.
fn drop_privileges(unprivileged_user: &str, directory: &Path) -> Result<PathBuf> {
    use nix::{
        errno::Errno,
        unistd::{chroot, geteuid, setuid, User},
    };

    // We need to lookup the user before chroot, otherwise the user db is gone.
    let unprivileged_uid = User::from_name(unprivileged_user)
        .context("Failed to lookup unprivileged user")?
        .ok_or_else(|| anyhow!("Failed to look up unprivileged user. Does it exist?"))?
        .uid;

    let new_root = match chroot(directory) {
        Ok(_) => {
            info!("Changed root directory to: {}", directory.display());
            Ok("/".into())
        }
        Err(Errno::EPERM) => {
            warn!("Can't drop filesystem privileges due to insufficient permissions. Start as root or with CAP_SYS_CHROOT, if this is desired.");
            Ok(directory.to_owned())
        }
        Err(e) => Err(e).context("Failed to chroot to directory"),
    }?;

    if geteuid().is_root() {
        setuid(unprivileged_uid).context("Failed to drop privileges")?;
        info!("Dropped privileges to user '{}'.", unprivileged_user);
    } else {
        info!(
            "Will not drop privileges to {}, because we are not running as root.",
            unprivileged_user
        );
    }

    Ok(new_root)
}

async fn server_main(socket: tokio::net::UdpSocket) -> Result<()> {
    loop {
        let mut buf = vec![0u8; 1 << 16];
        let (len, addr) = socket
            .recv_from(&mut buf)
            .await
            .context("Failed to read from UDP socket")?;

        println!("received {:?} bytes from {:?}", len, addr);
    }
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

    let socket =
        std::net::UdpSocket::bind(&args.listen_address).context("Failed to bind server port")?;
    debug!("Opened server socket: {:?}", socket);

    let _root_directory = drop_privileges(&args.unprivileged_user, &args.directory)?;

    // We use the simple scheduler of Tokio until it becomes a performance issue.
    let tokio_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to start I/O engine")?;

    tokio_runtime
        .block_on(async { server_main(tokio::net::UdpSocket::from_std(socket)?).await })?;

    info!("Graceful exit. Bye!");
    Ok(())
}
