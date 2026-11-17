use anyhow::{anyhow, Context, Result};
use clap::Parser;
use fcw_daemon::accel::{self, Accel};
use fcw_daemon::args::Args;
use fcw_daemon::daemon::Daemon;
use fcw_daemon::localsock::LocalListener;
use fcw_daemon::qemu::QemuLocator;
use fcw_daemon::vm::Env;
use fcw_daemon::{cert, control, logfwd, pb, quic};
use std::io::Write;
use std::net::SocketAddr;
use std::path::Path;
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
    args.validate().map_err(|e| anyhow!(e))?;
    // секрет TCP-канала: читаем или создаём до всего остального
    let tcp = match (&args.control_tcp, &args.token_file) {
        (Some(addr), Some(path)) => {
            let addr: SocketAddr = addr.parse().with_context(|| format!("--control-tcp {addr:?}"))?;
            let (token, created) = fcw_daemon::auth::load_or_create_token(path)?;
            if created {
                tracing::info!("created control token file {} (copy it to the mod config)", path.display());
            } else {
                tracing::info!("control token: {}", path.display());
            }
            Some((addr, Arc::new(token)))
        }
        _ => None,
    };
    let accel_req = Accel::parse(&args.accel).ok_or_else(|| anyhow!("unknown --accel {:?}", args.accel))?;
    // переносимый QEMU: --qemu-dir, по умолчанию <images-dir>/../qemu
    let qemu_dir = args.qemu_dir.clone().unwrap_or_else(|| {
        args.images_dir.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).join("qemu")
    });
    let locator = QemuLocator::from_env(args.qemu.clone(), qemu_dir, accel_req);
    let found = locator.find().await;
    match &found {
        Some(q) => tracing::info!(
            "qemu: {} ({}{}), qemu-img: {}",
            q.qemu.display(),
            if q.version_str.is_empty() { "unknown version" } else { &q.version_str },
            if q.bundled { ", portable" } else { "" },
            q.qemu_img.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "not found".into())
        ),
        None => tracing::warn!(
            "qemu-system-x86_64 not found (portable QEMU dir: {}): machines will not start until QEMU is installed",
            locator.dir.display()
        ),
    }
    let accel = match &found {
        Some(q) => q.accel,
        None => locator.accel_for(None).await,
    };
    tracing::info!("accelerator: {}", accel.name());
    if accel == Accel::Tcg {
        if let Some(why) = accel::kvm_problem() {
            tracing::warn!("no KVM, machines run in software emulation (slow): {why}");
        }
    }

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
    let net = fcw_daemon::net::build(&args)?;
    let screen = args.screen_config();
    tracing::info!(
        "screen stream per viewer: {}, up to {} fps",
        if screen.max_bytes_per_sec == 0 {
            "no bitrate limit".to_string()
        } else {
            format!("{:.1} Mbit/s", screen.max_bytes_per_sec as f64 / 125_000.0)
        },
        screen.max_fps
    );
    let env = Arc::new(Env { images_dir: args.images_dir.clone(), qemu: locator, tz, net, screen });
    let d = Daemon::new(env, args.vms_dir.clone(), args.max_vms, quic_port, cert.sha256_hex.clone(), events);

    // Unix-сокет (локальный мод); в режиме службы необязателен
    let mut unix_listener = None;
    let mut control_id = None;
    if let Some(path) = &args.control {
        if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).ok();
        }
        unix_listener = Some(LocalListener::bind(path).with_context(|| format!("bind {}", path.display()))?);
        control_id = file_id(path);
    }

    let control_str = match (&args.control, &tcp) {
        (Some(p), _) => p.to_string_lossy().to_string(),
        (None, Some((a, _))) => format!("tcp://{a}"),
        (None, None) => String::new(),
    };
    let mut ready = serde_json::json!({
        "control": control_str,
        "quic_port": quic_port,
        "cert_sha256": cert.sha256_hex,
        "accelerator": accel.name(),
    });
    if let Some((a, _)) = &tcp {
        ready["control_tcp"] = serde_json::json!(a.to_string());
    }
    {
        let mut out = std::io::stdout().lock();
        writeln!(out, "FCW-READY {ready}")?;
        out.flush()?;
    }
    tracing::info!(
        "ready: control {}{}, QUIC port {}{}",
        control_str,
        tcp.as_ref().filter(|_| args.control.is_some()).map(|(a, _)| format!(" + tcp://{a}")).unwrap_or_default(),
        quic_port,
        if args.service { " (service mode)" } else { "" }
    );

    if let Some(l) = unix_listener {
        tokio::spawn(control::serve(d.clone(), l));
    }
    // фатальная ошибка bind TCP завершает демон (systemd перезапустит)
    let (tcp_err_tx, mut tcp_err_rx) = tokio::sync::mpsc::unbounded_channel::<std::io::Error>();
    if let Some((addr, token)) = tcp {
        let policy = control::ConnPolicy::tcp(token, args.service);
        let d2 = d.clone();
        tokio::spawn(async move {
            if let Err(e) = control::serve_tcp(d2, addr, policy).await {
                let _ = tcp_err_tx.send(e);
            }
        });
    }
    tokio::spawn(quic::serve(d.clone(), ep.clone()));

    if !args.no_stdin_watch && !args.service {
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
    let mut fatal = None;
    tokio::select! {
        _ = sd.wait_for(|v| *v) => {}
        Some(e) = tcp_err_rx.recv() => {
            tracing::error!("control TCP {}: {e}", args.control_tcp.as_deref().unwrap_or(""));
            fatal = Some(anyhow!("control TCP bind failed: {e}"));
        }
    }
    tracing::info!("stopping machines");
    d.stop_all(Duration::from_secs(10)).await;
    ep.close(0u32.into(), b"daemon shutdown");
    // удаляем сокет, только если на его месте не появился чужой (новый демон)
    if let Some(path) = &args.control {
        if file_id(path) == control_id {
            let _ = std::fs::remove_file(path);
        }
    }
    tracing::info!("bye");
    match fatal {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
