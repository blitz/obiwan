mod path;
mod tftp;

use std::{
    io::SeekFrom,
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use log::{debug, error, info, trace, warn};
use path::normalize;
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt},
    runtime::Handle,
};

#[allow(dead_code)]
const SESSION_TIMEOUT: Duration = Duration::from_secs(5);

// The default block size of a TFTP connection.
const DEFAULT_BLKSIZE: usize = 512;

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

/// Sets the port of a socket address to zero. This is useful to let
/// the OS choose the port number for us.
fn clear_port(mut addr: SocketAddr) -> SocketAddr {
    addr.set_port(0);
    addr
}

async fn send_packet(socket: &tokio::net::UdpSocket, packet: tftp::Packet) -> Result<()> {
    trace!("{packet:?}");
    socket.send(&packet.to_vec()).await?;

    Ok(())
}

async fn send_error(
    socket: &tokio::net::UdpSocket,
    error_code: u16,
    error_msg: &str,
) -> Result<()> {
    send_packet(
        socket,
        tftp::Packet::Error {
            error_code,
            error_msg: error_msg.to_owned(),
        },
    )
    .await
}

async fn send_data(socket: &tokio::net::UdpSocket, block: u16, data: &[u8]) -> Result<()> {
    send_packet(
        socket,
        tftp::Packet::Data {
            block,
            data: data.to_owned(),
        },
    )
    .await
}

async fn send_file(socket: tokio::net::UdpSocket, mut file: tokio::fs::File) -> Result<()> {
    let mut buf: [u8; DEFAULT_BLKSIZE] = [0; DEFAULT_BLKSIZE];

    let block: u16 = 0;

    // Block number and maximal block size are 16-bit, so the overflow
    // should never happen. But in case we mess up the packet parsing
    // of these potentially malicious values, it's better to be
    // defensive.

    file.seek(SeekFrom::Start(
        u64::from(block)
            .checked_mul(u64::try_from(DEFAULT_BLKSIZE).unwrap())
            .ok_or_else(|| anyhow!("Integer overflow."))?,
    ))
    .await?;

    let len = file.read(&mut buf).await?;
    let buf = &buf[0..len];

    send_data(&socket, block, buf).await?;

    Ok(())
}

async fn handle_read(socket: tokio::net::UdpSocket, root: &Path, path: &Path) -> Result<()> {
    match tokio::fs::File::open(root.join(
        normalize(path).ok_or_else(|| anyhow!("Failed to normalize path: {}", path.display()))?,
    ))
    .await
    {
        Ok(file) if file.metadata().await?.is_file() => send_file(socket, file).await,
        Ok(_) => {
            send_error(&socket, 0, "Can't open a directory").await?;

            Ok(())
        }
        Err(e) => {
            send_error(&socket, 0, &e.to_string()).await?;

            Ok(())
        }
    }
}

async fn handle_connection(
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
    root: &Path,
    initial_request: tftp::Packet,
) -> Result<()> {
    debug!("{remote_addr}: Establishing new connection.");
    trace!("{remote_addr}: {initial_request:?}");

    let socket = tokio::net::UdpSocket::bind(clear_port(local_addr)).await?;
    debug!("{remote_addr}: Local address: {}", socket.local_addr()?);

    socket.connect(remote_addr).await?;

    match initial_request {
        tftp::Packet::Rrq {
            filename,
            mode: _,
            options: _,
        } => handle_read(socket, root, &filename).await?,
        tftp::Packet::Wrq {
            filename,
            mode,
            options: _,
        } => {
            warn!(
                "{remote_addr}: Write {} in {:?} mode denied. This server only supports reading.",
                filename.display(),
                mode
            );

            send_packet(
                &socket,
                tftp::Packet::Error {
                    error_code: 2, // TODO Access violation
                    error_msg: "This server only supports reading files".to_owned(),
                },
            )
            .await?;
        }

        request => {
            warn!("{remote_addr}: Invalid initial request: {request:?}");

            send_error(
                &socket,
                4, // TODO Illegal operation
                "Only read or write requests can start connections",
            )
            .await?;
        }
    }

    Ok(())
}

async fn server_main(runtime: &Handle, socket: tokio::net::UdpSocket, root: &Path) -> Result<()> {
    let local_addr = socket.local_addr()?;
    let mut buf = vec![0u8; 1 << 16];

    loop {
        let (len, remote_addr) = socket
            .recv_from(&mut buf)
            .await
            .context("Failed to read from UDP socket")?;

        match tftp::Packet::try_from(&buf[0..len]) {
            Ok(packet) => {
                let root = root.to_owned();
                runtime.spawn(async move {
                    if let Err(e) = handle_connection(local_addr, remote_addr, &root, packet).await
                    {
                        error!("Connection to {remote_addr} died due to an error: {e}");
                    }
                });
            }
            Err(e) => warn!("Ignoring packet: {e}"),
        }
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

    // Because we create the socket without Tokio, we need to make
    // sure it is non-blocking. Otherwise, Tokio will hang when
    // reading from it and not schedule other tasks.
    socket.set_nonblocking(true)?;

    debug!("Opened server socket: {:?}", socket);

    let root_directory = drop_privileges(&args.unprivileged_user, &args.directory)?;

    let tokio_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to start I/O engine")?;

    tokio_runtime.block_on(async {
        server_main(
            tokio_runtime.handle(),
            tokio::net::UdpSocket::from_std(socket)?,
            &root_directory,
        )
        .await
    })?;

    info!("Graceful exit. Bye!");
    Ok(())
}
