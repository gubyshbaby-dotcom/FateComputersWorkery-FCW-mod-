use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use fcw_client::Session;
use fcw_proto::control::{self as pb, envelope, request, response};
use fcw_proto::keys;
use fcw_proto::stream as spb;
use fcw_proto::frame;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UnixStream;

#[derive(Parser, Debug)]
#[command(name = "fcw-probe", version, about = "FCW test client (control socket + QUIC)")]
struct Cli {
    /// Unix-сокет канала управления демона
    #[arg(long)]
    control: PathBuf,
    /// Адрес демона для QUIC
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    /// Общий таймаут ожиданий, секунды
    #[arg(long, default_value_t = 120)]
    timeout: u64,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Hello: версия, QUIC-порт, отпечаток сертификата, ускоритель
    Hello,
    /// Список образов
    Images,
    /// Создать или обновить машину
    Create {
        #[arg(long)]
        vm: String,
        #[arg(long)]
        image: String,
        #[arg(long, default_value_t = 1)]
        cpus: u32,
        #[arg(long, default_value_t = 0)]
        memory: u32,
        #[arg(long)]
        gpu: bool,
        #[arg(long)]
        sound: bool,
        #[arg(long)]
        network: bool,
        #[arg(long, default_value_t = 0)]
        disk_gb: u32,
        #[arg(long, default_value_t = 0)]
        width: u32,
        #[arg(long, default_value_t = 0)]
        height: u32,
        #[arg(long, default_value = "")]
        lang: String,
        #[arg(long, default_value = "")]
        kbd: String,
        #[arg(long)]
        reinstall: bool,
    },
    Start {
        #[arg(long)]
        vm: String,
    },
    Stop {
        #[arg(long)]
        vm: String,
        #[arg(long)]
        force: bool,
        #[arg(long, default_value_t = 0)]
        timeout_ms: u32,
    },
    Pause {
        #[arg(long)]
        vm: String,
    },
    Resume {
        #[arg(long)]
        vm: String,
    },
    Delete {
        #[arg(long)]
        vm: String,
    },
    /// Состояние машины (или всех)
    Status {
        #[arg(long)]
        vm: Option<String>,
    },
    /// Ждать состояния машины (running, stopped, paused, crashed, ...)
    WaitState {
        #[arg(long)]
        vm: String,
        #[arg(long)]
        state: String,
    },
    /// Выпустить токен
    Token {
        #[arg(long)]
        vm: String,
        #[arg(long, default_value_t = 7)]
        perms: u32,
    },
    /// Отозвать токены и привязки
    Revoke {
        #[arg(long, default_value = "")]
        vm: String,
        #[arg(long, default_value = "")]
        player: String,
    },
    /// Снимок экрана в PNG через QUIC, как его видит игрок
    Screenshot {
        #[arg(long)]
        vm: String,
        #[arg(long, default_value = "tty0")]
        screen: String,
        #[arg(long)]
        out: PathBuf,
        /// Ждать, пока на текстовом экране появится строка
        #[arg(long)]
        wait_text: Option<String>,
        /// Сколько мс без обновлений считать «экран успокоился»
        #[arg(long, default_value_t = 1500)]
        settle_ms: u64,
        /// Ещё сохранить текст экрана рядом (для tty)
        #[arg(long)]
        dump_text: bool,
    },
    /// Текст текстового экрана в stdout
    Text {
        #[arg(long)]
        vm: String,
        #[arg(long)]
        wait_text: Option<String>,
        #[arg(long, default_value_t = 1000)]
        settle_ms: u64,
    },
    /// Набрать текст (tty0: событием Text; gfx0: клавишами US)
    Type {
        #[arg(long)]
        vm: String,
        #[arg(long)]
        text: String,
        #[arg(long, default_value = "")]
        screen: String,
        /// Нажать Enter в конце
        #[arg(long)]
        enter: bool,
    },
    /// Нажать клавишу/сочетание: enter, ctrl+c, alt+f4, f1, up ...
    Key {
        #[arg(long)]
        vm: String,
        #[arg(long)]
        key: String,
        #[arg(long, default_value = "")]
        screen: String,
        #[arg(long, default_value_t = 1)]
        repeat: u32,
    },
    /// Переместить мышь и (необязательно) щёлкнуть
    Mouse {
        #[arg(long)]
        vm: String,
        #[arg(long)]
        x: i32,
        #[arg(long)]
        y: i32,
        /// left | right | middle
        #[arg(long)]
        click: Option<String>,
        #[arg(long, default_value_t = 0)]
        wheel: i32,
    },
    /// Строка JSON агенту гостя
    AgentSend {
        #[arg(long)]
        vm: String,
        #[arg(long)]
        json: String,
    },
    /// Уровни редстоуна на входах: 6 чисел через запятую
    Redstone {
        #[arg(long)]
        vm: String,
        #[arg(long)]
        levels: String,
    },
    /// Печатать события канала управления N секунд
    Events {
        #[arg(long, default_value_t = 10)]
        seconds: u64,
    },
    /// Скачать образ (tar.zst) и ждать завершения
    Fetch {
        #[arg(long)]
        image: String,
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "")]
        sha256: String,
    },
    /// Остановить демон
    Shutdown,
}

pub fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
    match rt.block_on(run(cli)) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fcw-probe: {e:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

// ------------------------------------------------------------ канал управления

struct Control {
    s: UnixStream,
    next_id: u64,
    events: Vec<pb::Event>,
}

impl Control {
    async fn connect(path: &PathBuf) -> Result<Control> {
        let s = UnixStream::connect(path).await.with_context(|| format!("connect {}", path.display()))?;
        let mut c = Control { s, next_id: 1, events: Vec::new() };
        c.call(request::Kind::Hello(pb::Hello { client_name: "fcw-probe".into(), protocol_version: 1 })).await?;
        Ok(c)
    }

    async fn call(&mut self, kind: request::Kind) -> Result<pb::Response> {
        let id = self.next_id;
        self.next_id += 1;
        let env = pb::Envelope { id, body: Some(envelope::Body::Request(pb::Request { kind: Some(kind) })) };
        frame::write_async(&mut self.s, &env).await?;
        loop {
            let e: pb::Envelope = frame::read_async(&mut self.s).await?.ok_or_else(|| anyhow!("daemon closed"))?;
            match e.body {
                Some(envelope::Body::Response(r)) if e.id == id => return Ok(r),
                Some(envelope::Body::Event(ev)) => self.events.push(ev),
                _ => {}
            }
        }
    }

    async fn ok(&mut self, kind: request::Kind) -> Result<pb::Response> {
        let r = self.call(kind).await?;
        if !r.ok {
            bail!("{}", r.error);
        }
        Ok(r)
    }

    async fn next_event(&mut self, timeout: Duration) -> Result<Option<pb::Event>> {
        if !self.events.is_empty() {
            return Ok(Some(self.events.remove(0)));
        }
        match tokio::time::timeout(timeout, frame::read_async::<_, pb::Envelope>(&mut self.s)).await {
            Err(_) => Ok(None),
            Ok(r) => match r?.and_then(|e| e.body) {
                Some(envelope::Body::Event(ev)) => Ok(Some(ev)),
                _ => Ok(None),
            },
        }
    }

    async fn hello(&mut self) -> Result<pb::HelloResult> {
        let r = self.ok(request::Kind::Hello(pb::Hello { client_name: "fcw-probe".into(), protocol_version: 1 })).await?;
        match r.kind {
            Some(response::Kind::Hello(h)) => Ok(h),
            _ => bail!("bad hello response"),
        }
    }

    async fn vm(&mut self, vm: &str) -> Result<pb::VmInfo> {
        let r = self.ok(request::Kind::GetVm(pb::GetVm { vm_id: vm.into() })).await?;
        match r.kind {
            Some(response::Kind::Vm(v)) => Ok(v),
            _ => bail!("bad GetVm response"),
        }
    }

    async fn token(&mut self, vm: &str, perms: u32) -> Result<String> {
        let r = self
            .ok(request::Kind::IssueToken(pb::IssueToken {
                vm_id: vm.into(),
                player_uuid: "00000000-0000-0000-0000-00000000f00d".into(),
                player_name: "fcw-probe".into(),
                permissions: perms,
                ttl_seconds: 60,
            }))
            .await?;
        match r.kind {
            Some(response::Kind::Token(t)) => Ok(t.token),
            _ => bail!("bad IssueToken response"),
        }
    }
}

fn state_name(s: i32) -> &'static str {
    match pb::VmState::try_from(s).unwrap_or(pb::VmState::Unknown) {
        pb::VmState::Unknown => "unknown",
        pb::VmState::Stopped => "stopped",
        pb::VmState::Starting => "starting",
        pb::VmState::Running => "running",
        pb::VmState::Paused => "paused",
        pb::VmState::Stopping => "stopping",
        pb::VmState::Crashed => "crashed",
    }
}

fn print_vm(v: &pb::VmInfo) {
    println!(
        "{} state={} image={} cpus={} mem={} gpu={} uptime={}s agent={} controller={:?} screens=[{}]{}",
        v.vm_id,
        state_name(v.state),
        v.image_id,
        v.cpus,
        v.memory_mb,
        v.gpu,
        v.uptime_ms / 1000,
        v.agent_connected,
        v.controller_player,
        v.screens.iter().map(|s| format!("{}:{}x{}", s.screen_id, s.width, s.height)).collect::<Vec<_>>().join(","),
        if v.last_error.is_empty() { String::new() } else { format!(" error={:?}", v.last_error) }
    );
}

fn print_event(ev: &pb::Event) {
    use pb::event::Kind;
    match &ev.kind {
        Some(Kind::VmState(s)) => println!("event vm_state {} {} {}", s.vm_id, state_name(s.state), s.reason),
        Some(Kind::AgentMessage(m)) => println!("event agent {} {}", m.vm_id, m.json),
        Some(Kind::RedstoneOutput(r)) => println!("event redstone {} side={} level={}", r.vm_id, r.side, r.level),
        Some(Kind::ImageProgress(p)) => println!(
            "event image {} {}/{} finished={} {}",
            p.image_id, p.done_bytes, p.total_bytes, p.finished, p.error
        ),
        Some(Kind::Log(l)) => println!("event log [{}] {}", l.level, l.message),
        None => {}
    }
}

// ------------------------------------------------------------ QUIC

struct Quic {
    s: Arc<Session>,
    vm: String,
}

impl Quic {
    async fn open(c: &mut Control, host: &str, vm: &str, perms: u32, timeout: Duration) -> Result<Quic> {
        let h = c.hello().await?;
        let token = c.token(vm, perms).await?;
        let s = Session::connect(host, h.quic_port as u16, &h.cert_sha256);
        let deadline = Instant::now() + timeout;
        while s.state() == fcw_client::session::STATE_CONNECTING {
            if Instant::now() > deadline {
                bail!("QUIC connect timeout");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        if s.state() != fcw_client::session::STATE_READY {
            bail!("QUIC: {}", s.error());
        }
        s.attach(&token);
        loop {
            if Instant::now() > deadline {
                bail!("attach timeout");
            }
            while let Some(ev) = s.poll_event() {
                let v: serde_json::Value = serde_json::from_str(&ev)?;
                if v["type"] == "attach" {
                    if v["ok"] == true {
                        return Ok(Quic { s, vm: vm.to_string() });
                    }
                    bail!("attach failed: {}", v["error"]);
                }
                if v["type"] == "error" && v["fatal"] == true {
                    bail!("session error: {}", v["message"]);
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn close(self) {
        self.s.close();
        let t = Instant::now();
        while !self.s.finished() && t.elapsed() < Duration::from_secs(3) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Открыть вид и дождаться устойчивой картинки.
    async fn view(
        &self,
        screen: &str,
        wait_text: Option<&str>,
        settle: Duration,
        timeout: Duration,
    ) -> Result<Arc<fcw_client::View>> {
        let v = self.s.open_view(&self.vm, screen, 30);
        let deadline = Instant::now() + timeout;
        let mut last_updates = 0;
        let mut last_change = Instant::now();
        loop {
            if self.s.state() != fcw_client::session::STATE_READY {
                bail!("session: {}", self.s.error());
            }
            let i = v.info();
            if i.updates != last_updates {
                last_updates = i.updates;
                last_change = Instant::now();
            }
            let text_ok = match wait_text {
                Some(t) => v.text_contents().contains(t),
                None => true,
            };
            if i.updates > 0 && i.width > 0 && text_ok && last_change.elapsed() >= settle {
                return Ok(v);
            }
            if Instant::now() > deadline {
                if i.updates == 0 || i.width == 0 {
                    bail!("no frame received from {screen}");
                }
                if !text_ok {
                    eprintln!("--- screen ---\n{}--------------", v.text_contents());
                    bail!("text {:?} did not appear", wait_text.unwrap_or(""));
                }
                return Ok(v);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn key(&self, screen: &str, k: u32, down: bool) {
        self.s.send_input(&self.vm, screen, spb::input_event::Kind::Key(spb::Key { evdev: k, down }));
    }
}

fn write_png(path: &PathBuf, w: u32, h: u32, rgba: &[u8]) -> Result<()> {
    let f = std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header()?;
    wr.write_image_data(rgba)?;
    Ok(())
}

fn parse_combo(s: &str) -> Result<Vec<u32>> {
    s.split('+').map(|p| keys::evdev_from_name(p).ok_or_else(|| anyhow!("unknown key {p:?}"))).collect()
}

async fn run(cli: Cli) -> Result<()> {
    let timeout = Duration::from_secs(cli.timeout);
    let mut c = Control::connect(&cli.control).await?;
    use request::Kind as K;
    match cli.cmd {
        Cmd::Hello => {
            let h = c.hello().await?;
            println!(
                "daemon {} protocol {} quic_port {} cert {} accel {} max_vms {} cpus {} mem {}MB",
                h.daemon_version,
                h.protocol_version,
                h.quic_port,
                h.cert_sha256,
                h.accelerator,
                h.max_vms,
                h.host_cpus,
                h.host_memory_mb
            );
        }
        Cmd::Images => {
            let r = c.ok(K::ListImages(pb::ListImages {})).await?;
            if let Some(response::Kind::Images(l)) = r.kind {
                for i in l.images {
                    println!(
                        "{} {:?} {} installed={} desktop={:?} mem={}/{}",
                        i.id, i.name, i.version, i.installed, i.desktop, i.min_memory_mb, i.recommended_memory_mb
                    );
                }
            }
        }
        Cmd::Create { vm, image, cpus, memory, gpu, sound, network, disk_gb, width, height, lang, kbd, reinstall } => {
            let r = c
                .ok(K::CreateVm(pb::CreateVm {
                    vm_id: vm,
                    image_id: image,
                    cpus,
                    memory_mb: memory,
                    gpu,
                    disk_gb,
                    lang,
                    kbd_layout: kbd,
                    sound,
                    network,
                    reinstall,
                    display_width: width,
                    display_height: height,
                }))
                .await?;
            if let Some(response::Kind::Vm(v)) = r.kind {
                print_vm(&v);
            }
        }
        Cmd::Start { vm } => {
            let r = c.ok(K::StartVm(pb::StartVm { vm_id: vm })).await?;
            if let Some(response::Kind::Vm(v)) = r.kind {
                print_vm(&v);
            }
        }
        Cmd::Stop { vm, force, timeout_ms } => {
            let r = c.ok(K::StopVm(pb::StopVm { vm_id: vm, force, timeout_ms })).await?;
            if let Some(response::Kind::Vm(v)) = r.kind {
                print_vm(&v);
            }
        }
        Cmd::Pause { vm } => {
            c.ok(K::PauseVm(pb::PauseVm { vm_id: vm.clone() })).await?;
            print_vm(&c.vm(&vm).await?);
        }
        Cmd::Resume { vm } => {
            c.ok(K::ResumeVm(pb::ResumeVm { vm_id: vm.clone() })).await?;
            print_vm(&c.vm(&vm).await?);
        }
        Cmd::Delete { vm } => {
            c.ok(K::DeleteVm(pb::DeleteVm { vm_id: vm })).await?;
            println!("deleted");
        }
        Cmd::Status { vm } => match vm {
            Some(vm) => print_vm(&c.vm(&vm).await?),
            None => {
                let r = c.ok(K::ListVms(pb::ListVms {})).await?;
                if let Some(response::Kind::Vms(l)) = r.kind {
                    for v in &l.vms {
                        print_vm(v);
                    }
                }
            }
        },
        Cmd::WaitState { vm, state } => {
            let deadline = Instant::now() + timeout;
            loop {
                let v = c.vm(&vm).await?;
                if state_name(v.state) == state {
                    print_vm(&v);
                    break;
                }
                if Instant::now() > deadline {
                    print_vm(&v);
                    bail!("timeout waiting for state {state}");
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
        Cmd::Token { vm, perms } => println!("{}", c.token(&vm, perms).await?),
        Cmd::Revoke { vm, player } => {
            c.ok(K::RevokeTokens(pb::RevokeTokens { vm_id: vm, player_uuid: player })).await?;
        }
        Cmd::Screenshot { vm, screen, out, wait_text, settle_ms, dump_text } => {
            let q = Quic::open(&mut c, &cli.host, &vm, pb::Permission::PermView as u32, timeout).await?;
            let v = q.view(&screen, wait_text.as_deref(), Duration::from_millis(settle_ms), timeout).await?;
            let (w, h, rgba) = v.snapshot();
            write_png(&out, w, h, &rgba)?;
            println!("{} {}x{} seq={}", out.display(), w, h, v.info().seq);
            if dump_text {
                let txt = out.with_extension("txt");
                std::fs::write(&txt, v.text_contents())?;
                println!("{}", txt.display());
            }
            q.close().await;
        }
        Cmd::Text { vm, wait_text, settle_ms } => {
            let q = Quic::open(&mut c, &cli.host, &vm, pb::Permission::PermView as u32, timeout).await?;
            let v = q.view("tty0", wait_text.as_deref(), Duration::from_millis(settle_ms), timeout).await?;
            print!("{}", v.text_contents());
            q.close().await;
        }
        Cmd::Type { vm, text, screen, enter } => {
            let info = c.vm(&vm).await?;
            let screen = if screen.is_empty() { if info.gpu { "gfx0" } else { "tty0" }.to_string() } else { screen };
            let q = Quic::open(&mut c, &cli.host, &vm, 7, timeout).await?;
            if screen.starts_with("tty") {
                q.s.send_input(&vm, &screen, spb::input_event::Kind::Text(spb::Text { utf8: text.clone() }));
            } else {
                for ch in text.chars() {
                    let Some((k, shift)) = keys::char_to_evdev_us(ch) else {
                        eprintln!("skipping {ch:?}: no US key");
                        continue;
                    };
                    if shift {
                        q.key(&screen, keys::KEY_LEFTSHIFT, true);
                    }
                    q.key(&screen, k, true);
                    q.key(&screen, k, false);
                    if shift {
                        q.key(&screen, keys::KEY_LEFTSHIFT, false);
                    }
                }
            }
            if enter {
                q.key(&screen, keys::KEY_ENTER, true);
                q.key(&screen, keys::KEY_ENTER, false);
            }
            q.close().await;
            println!("typed {} chars into {vm}/{screen}", text.chars().count());
        }
        Cmd::Key { vm, key, screen, repeat } => {
            let combo = parse_combo(&key)?;
            let q = Quic::open(&mut c, &cli.host, &vm, 7, timeout).await?;
            for _ in 0..repeat {
                for k in &combo {
                    q.key(&screen, *k, true);
                }
                for k in combo.iter().rev() {
                    q.key(&screen, *k, false);
                }
            }
            q.close().await;
            println!("sent {key}");
        }
        Cmd::Mouse { vm, x, y, click, wheel } => {
            let q = Quic::open(&mut c, &cli.host, &vm, 7, timeout).await?;
            q.s.send_pointer(&vm, x, y);
            if let Some(b) = click {
                let mask = match b.as_str() {
                    "left" => 1,
                    "right" => 2,
                    "middle" => 4,
                    other => bail!("unknown button {other}"),
                };
                q.s.send_buttons(&vm, mask, x, y);
                q.s.send_buttons(&vm, 0, x, y);
            }
            if wheel != 0 {
                q.s.send_wheel(&vm, 0, wheel, x, y);
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
            q.close().await;
        }
        Cmd::AgentSend { vm, json } => {
            c.ok(K::AgentSend(pb::AgentSend { vm_id: vm, json })).await?;
        }
        Cmd::Redstone { vm, levels } => {
            let levels: Vec<u32> = levels.split(',').map(|s| s.trim().parse()).collect::<Result<_, _>>()?;
            c.ok(K::SetRedstoneInputs(pb::SetRedstoneInputs { vm_id: vm, levels })).await?;
        }
        Cmd::Events { seconds } => {
            let deadline = Instant::now() + Duration::from_secs(seconds);
            while Instant::now() < deadline {
                if let Some(ev) = c.next_event(deadline - Instant::now()).await? {
                    print_event(&ev);
                }
            }
        }
        Cmd::Fetch { image, url, sha256 } => {
            c.ok(K::FetchImage(pb::FetchImage { image_id: image.clone(), url, sha256 })).await?;
            let deadline = Instant::now() + timeout;
            loop {
                if Instant::now() > deadline {
                    bail!("fetch timeout");
                }
                let Some(ev) = c.next_event(Duration::from_secs(1)).await? else { continue };
                if let Some(pb::event::Kind::ImageProgress(p)) = &ev.kind {
                    if p.image_id == image {
                        print_event(&ev);
                        if p.finished {
                            if !p.error.is_empty() {
                                bail!("{}", p.error);
                            }
                            break;
                        }
                    }
                }
            }
        }
        Cmd::Shutdown => {
            c.ok(K::Shutdown(pb::Shutdown {})).await?;
            println!("daemon is shutting down");
        }
    }
    Ok(())
}
