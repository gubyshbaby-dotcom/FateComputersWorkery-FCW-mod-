use anyhow::{anyhow, Context, Result};
use clap::Parser;
use fcw_daemon::accel::{self, Accel};
use fcw_daemon::args::Args;
use fcw_daemon::daemon::Daemon;
use fcw_daemon::localsock::LocalListener;
use fcw_daemon::vm::Env;
use fcw_daemon::{cert, control, logfwd, pb, quic};
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

fn main() -> std::process::ExitCode {
    let args = Args::parse();
    let (events, _) = broadcast::channel::<pb::Event>(1024);
    let filter = tracing_subscriber::EnvFilter::try_from_env("FCW_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr).with_ansi(false).with_target(false))
        .with(logfwd::ForwardLayer::new(events.clone()))
        .init();
    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().thread_name("fcw-daemon").build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("fcw-daemon: runtime: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let res = rt.block_on(run(args, events));
    rt.shutdown_timeout(Duration::from_secs(2));
    match res {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("fatal: {e:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn search_path(name: &str) -> Option<PathBuf> {
    let file = exe(name);
    let mut dirs: Vec<PathBuf> =
        std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if cfg!(windows) {
        dirs.push(PathBuf::from(r"C:\Program Files\qemu"));
    }
    if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    dirs.into_iter().map(|d| d.join(&file)).find(|p| p.is_file())
}

/// QEMU: --qemu, $FCW_QEMU, qemu-system-x86_64 в PATH.
fn find_qemu(arg: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(p) = arg {
        return Some(p);
    }
    if let Some(p) = std::env::var_os("FCW_QEMU").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    search_path("qemu-system-x86_64")
}

/// qemu-img рядом с QEMU, $FCW_QEMU_IMG или в PATH.
fn find_qemu_img(qemu: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = qemu.and_then(|q| q.parent()) {
        let p = dir.join(exe("qemu-img"));
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(p) = std::env::var_os("FCW_QEMU_IMG").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    search_path("qemu-img")
}

/// Идентичность файла (устройство + inode на Unix).
fn file_id(p: &Path) -> Option<(u64, u64)> {
    let m = std::fs::symlink_metadata(p).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some((m.dev(), m.ino()))
    }
    #[cfg(not(unix))]
    {
        m.modified()
            .ok()
            .map(|t| (t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0), m.len()))
    }
}

async fn run(args: Args, events: broadcast::Sender<pb::Event>) -> Result<()> {
    let accel_req = Accel::parse(&args.accel).ok_or_else(|| anyhow!("unknown --accel {:?}", args.accel))?;
    let qemu = find_qemu(args.qemu.clone());
    let qemu_img = find_qemu_img(qemu.as_deref());
    match &qemu {
        Some(q) => tracing::info!("qemu: {}", q.display()),
        None => tracing::warn!("qemu-system-x86_64 not found: machines will not start"),
    }
    let accel = match (accel_req, &qemu) {
        (Some(a), _) => a,
        (None, Some(q)) => accel::detect(q).await,
        (None, None) => Accel::Tcg,
    };
    tracing::info!("accelerator: {}", accel.name());

    std::fs::create_dir_all(&args.images_dir).with_context(|| format!("create {}", args.images_dir.display()))?;
    std::fs::create_dir_all(&args.vms_dir).with_context(|| format!("create {}", args.vms_dir.display()))?;
    let cert_dir = args
        .vms_dir
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| args.vms_dir.clone());
    let cert = cert::load_or_create(&cert_dir)?;

    let bind: SocketAddr = args.quic_bind.parse().with_context(|| format!("--quic-bind {:?}", args.quic_bind))?;
    let ep = quic::make_endpoint(bind, &cert).with_context(|| format!("QUIC bind {bind}"))?;
    let quic_port = ep.local_addr()?.port();

    let tz = fcw_daemon::vm::host_timezone();
    tracing::info!("host time zone: {}", tz.as_deref().unwrap_or("unknown"));
    let qemu_version = qemu.as_deref().and_then(fcw_daemon::net::qemu_version);
    if let Some((a, b, c)) = qemu_version {
        tracing::info!("qemu version: {a}.{b}.{c}");
    }
    let net = fcw_daemon::net::build(&args)?;
    let env = Arc::new(Env { images_dir: args.images_dir.clone(), qemu, qemu_img, accel, tz, qemu_version, net });
    let d = Daemon::new(env, args.vms_dir.clone(), args.max_vms, quic_port, cert.sha256_hex.clone(), events);

    if let Some(dir) = args.control.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).ok();
    }
    let listener = LocalListener::bind(&args.control).with_context(|| format!("bind {}", args.control.display()))?;
    let control_id = file_id(&args.control);

    let ready = serde_json::json!({
        "control": args.control.to_string_lossy(),
        "quic_port": quic_port,
        "cert_sha256": cert.sha256_hex,
        "accelerator": accel.name(),
    });
    {
        let mut out = std::io::stdout().lock();
        writeln!(out, "FCW-READY {ready}")?;
        out.flush()?;
    }
    tracing::info!("ready: control {}, QUIC port {}", args.control.display(), quic_port);

    tokio::spawn(control::serve(d.clone(), listener));
    tokio::spawn(quic::serve(d.clone(), ep.clone()));

    if !args.no_stdin_watch {
        let d2 = d.clone();
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut stdin = tokio::io::stdin();
            let mut buf = [0u8; 256];
            loop {
                match stdin.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
            tracing::info!("stdin closed, shutting down");
            d2.request_shutdown();
        });
    }
    {
        let d2 = d.clone();
        tokio::spawn(async move {
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};
                let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            #[cfg(not(unix))]
            {
                let _ = tokio::signal::ctrl_c().await;
            }
            tracing::info!("signal received, shutting down");
            d2.request_shutdown();
        });
    }

    let mut sd = d.shutdown_signal();
    let _ = sd.wait_for(|v| *v).await;
    tracing::info!("stopping machines");
    d.stop_all(Duration::from_secs(10)).await;
    ep.close(0u32.into(), b"daemon shutdown");
    // удаляем сокет, только если на его месте не появился чужой (новый демон)
    if file_id(&args.control) == control_id {
        let _ = std::fs::remove_file(&args.control);
    }
    tracing::info!("bye");
    Ok(())
}
