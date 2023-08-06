mod path;
mod simple_fs;
mod simple_proto;
mod tftp;
mod tftp_proto;

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use log::{debug, error, info, trace, warn, LevelFilter};
use tokio::{runtime::Handle, time::timeout};

use crate::{
    simple_proto::{ConnectionStatus, Event, SimpleUdpProtocol},
    tftp_proto::Connection,
};

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
        libc::{prctl, PR_SET_NO_NEW_PRIVS},
        unistd::{chroot, geteuid, setuid, User},
    };

    // prctl has no clear safety requirements, but we use it as the C
    // man page intends it to be used.
    match unsafe { prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } {
        0 => info!("Applied NO_NEW_PRIVS."),
        e => warn!("Failed to apply NO_NEW_PRIVS. Error: {e}"),
    }

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

async fn recv_packet(
    socket: &tokio::net::UdpSocket,
    recv_timeout: Duration,
) -> Result<Option<tftp::Packet>> {
    let mut buf = vec![0u8; 1 << 16];

    match timeout(recv_timeout, socket.recv(&mut buf)).await {
        Ok(res) => Some(res?),
        Err(_) => None,
    }
    .map(|len| tftp::Packet::try_from(&buf[0..len]).context("Failed to parse incoming packet"))
    .transpose()
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

    let mut con = Connection::new(root);
    let mut packet = Some(initial_request);

    loop {
        let response = con
            .handle_event(match packet {
                Some(p) => Event::PacketReceived(p),
                None => Event::Timeout,
            })
            .await?;

        if let Some(p) = response.packet {
            send_packet(&socket, p).await?;
        }

        match response.next_status {
            ConnectionStatus::Terminated => break,
            ConnectionStatus::WaitingForPacket(timeout) => {
                packet = recv_packet(&socket, timeout).await?;
            }
        }
    }

    debug!("{remote_addr}: Connection terminated.");
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

    simplelog::SimpleLogger::init(
        match args.verbose {
            0 => LevelFilter::Warn,
            1 => LevelFilter::Info,
            2 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        },
        simplelog::Config::default(),
    )?;

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
