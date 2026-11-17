//! `xr` — ferramenta SÓ DE LEITURA para o XREAL One Pro (substitui os scripts Python de espiar).
//! Não existe comando de ajuste aqui, de propósito: ajustes só pela interface, com confirmação.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use xr_client::{ClientConfig, ClientEvent, DeviceState, Endpoint, Glasses};

const USO: &str = "\
xr — leitura do XREAL One Pro (nada é ajustado)

Uso:
  xr info                      versões, sensor de rosto, EIS, calibração
  xr listen [segundos]         teclas, ações (brilho, filtro), temperaturas (padrão 60 s)
  xr imu [segundos]            orientação ao vivo, 10x por segundo (padrão 30 s)
  xr record <pasta> [segundos] grava o fluxo bruto das 3 portas (sem enviar nada; padrão 30 s)

Opções (antes do comando):
  --host <ip>   usa só este endereço (padrão: 169.254.2.1 e depois 169.254.1.1)
  --sim         usa um óculos simulado local (para testar sem o aparelho)";

struct Args {
    host: Option<String>,
    sim: bool,
    cmd: Vec<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args { host: None, sim: false, cmd: Vec::new() };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        match x.as_str() {
            "--host" => a.host = Some(it.next().ok_or("--host precisa de um endereço")?),
            "--sim" => a.sim = true,
            "-h" | "--help" => return Err(String::new()),
            _ => a.cmd.push(x),
        }
    }
    if a.cmd.is_empty() {
        return Err(String::new());
    }
    Ok(a)
}

fn secs(arg: Option<&String>, default: u64) -> Result<Duration, String> {
    match arg {
        None => Ok(Duration::from_secs(default)),
        Some(s) => s.parse::<f64>().ok().filter(|v| *v > 0.0).map(Duration::from_secs_f64).ok_or(format!("tempo inválido: {s}")),
    }
}

fn sim_endpoint(sim: &xr_sim::Sim) -> Endpoint {
    Endpoint {
        host: "127.0.0.1".into(),
        imu_port: sim.imu_addr.port(),
        vsync_port: sim.vsync_addr.port(),
        control_port: sim.control_addr.port(),
    }
}

fn on_off(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => "ligado",
        Some(false) => "desligado",
        None => "?",
    }
}

async fn wait_ready(g: &Glasses, need_imu: bool) -> Result<DeviceState, String> {
    let mut rx = g.state();
    let r = tokio::time::timeout(
        Duration::from_secs(8),
        rx.wait_for(|s| s.control.is_connected() && s.sw_version.is_some() && (!need_imu || s.imu.is_connected())),
    )
    .await;
    match r {
        Ok(Ok(s)) => Ok(s.clone()),
        _ => Err("não consegui falar com o óculos (cabo USB ligado? rede 169.254.x ativa?)".into()),
    }
}

async fn cmd_info(g: &Glasses) -> Result<(), String> {
    wait_ready(g, false).await?;
    // a calibração é a última leitura feita ao conectar: depois dela o resto já chegou
    let mut rx = g.state();
    let _ = tokio::time::timeout(Duration::from_secs(5), rx.wait_for(|s| s.calibration_loaded)).await;
    let s = g.state().borrow().clone();
    let cal = s.calibration_loaded;
    let host = match &s.control {
        xr_client::Link::Connected { host } => host.clone(),
        _ => "?".into(),
    };
    println!("conectado em:     {host}");
    println!("firmware:         {}", s.sw_version.as_deref().unwrap_or("?"));
    println!("DSP:              {}", s.dsp_version.as_deref().unwrap_or("?"));
    println!("sensor de rosto:  {}", on_off(s.proximity_enabled));
    println!("EIS:              {}", on_off(s.eis_enabled));
    println!("calibração:       {}", if cal { "carregada (só em memória)" } else { "não carregada" });
    match s.adjustments_blocked {
        Some(_) if !xr_proto::gate::TESTED_FIRMWARE.contains(&s.sw_version.as_deref().unwrap_or("")) => {
            println!("ajustes:          bloqueados (firmware fora da lista de testados)")
        }
        _ => println!("ajustes:          firmware na lista de testados"),
    }
    Ok(())
}

async fn cmd_listen(g: &Glasses, dur: Duration) -> Result<(), String> {
    let mut ev = g.events();
    wait_ready(g, false).await?;
    println!("Escutando {:.0} s, sem enviar nada. Ctrl+C para sair.", dur.as_secs_f64());
    let t0 = Instant::now();
    let end = tokio::time::Instant::now() + dur;
    loop {
        tokio::select! {
            e = ev.recv() => match e {
                Ok(ClientEvent::Device(d)) => println!("[{:8.3}s] {}", t0.elapsed().as_secs_f64(), d.describe_pt()),
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => println!("({n} eventos perdidos)"),
                Err(_) => return Err("cliente encerrado".into()),
            },
            _ = tokio::time::sleep_until(end) => return Ok(()),
            _ = tokio::signal::ctrl_c() => return Ok(()),
        }
    }
}

async fn cmd_imu(g: &Glasses, dur: Duration) -> Result<(), String> {
    wait_ready(g, true).await?;
    let orient = g.orientation();
    let state = g.state();
    let end = tokio::time::Instant::now() + dur;
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            _ = tick.tick() => {
                let s = state.borrow().clone();
                match *orient.borrow() {
                    Some(o) => println!(
                        "arfagem {:6.1}° guinada {:7.1}° rolagem {:6.1}° | °/s {:6.1} {:6.1} {:6.1} | {} | IMU {:4.0}/s vsync {:3.0}/s | {:5.2} °C",
                        o.pitch_deg, o.yaw_deg, o.roll_deg, o.rate_dps.x, o.rate_dps.y, o.rate_dps.z,
                        if o.still { "parado " } else { "movendo" }, s.imu_hz, s.vsync_hz, o.temp_c
                    ),
                    None => println!("(aguardando a IMU)"),
                }
            }
            _ = tokio::time::sleep_until(end) => return Ok(()),
            _ = tokio::signal::ctrl_c() => return Ok(()),
        }
    }
}

/// Grava o que chega em cada porta, sem mandar nenhum byte.
async fn cmd_record(endpoints: &[Endpoint], dir: PathBuf, dur: Duration) -> Result<(), String> {
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    type PortOf = fn(&Endpoint) -> u16;
    let ports: [(&str, PortOf); 3] =
        [("imu", |e| e.imu_port), ("vsync", |e| e.vsync_port), ("controle", |e| e.control_port)];
    let mut tasks = Vec::new();
    for (name, port) in ports {
        let mut stream = None;
        for ep in endpoints {
            if let Ok(Ok(s)) =
                tokio::time::timeout(Duration::from_secs(2), tokio::net::TcpStream::connect((ep.host.as_str(), port(ep)))).await
            {
                stream = Some(s);
                break;
            }
        }
        let mut s = stream.ok_or(format!("não conectei na porta de {name}"))?;
        let path = dir.join(format!("{name}.bin"));
        tasks.push(tokio::spawn(async move {
            let mut f = tokio::fs::File::create(&path).await.map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 64 * 1024];
            let mut total = 0usize;
            let end = tokio::time::Instant::now() + dur;
            loop {
                tokio::select! {
                    n = s.read(&mut buf) => {
                        let n = n.map_err(|e| e.to_string())?;
                        if n == 0 { break; }
                        f.write_all(&buf[..n]).await.map_err(|e| e.to_string())?;
                        total += n;
                    }
                    _ = tokio::time::sleep_until(end) => break,
                }
            }
            f.flush().await.map_err(|e| e.to_string())?;
            Ok::<_, String>((name, total))
        }));
    }
    println!("Gravando {:.0} s em {} (nada é enviado ao óculos)...", dur.as_secs_f64(), dir.display());
    for t in tasks {
        let (name, total) = t.await.map_err(|e| e.to_string())??;
        println!("  {name}.bin: {total} bytes");
    }
    println!("As gravações são dados do seu aparelho: não publique sem revisar.");
    Ok(())
}

async fn run(a: Args) -> Result<(), String> {
    let sim = if a.sim { Some(xr_sim::Sim::start(xr_sim::SimConfig::default()).await.map_err(|e| e.to_string())?) } else { None };
    let endpoints = match (&sim, &a.host) {
        (Some(s), _) => vec![sim_endpoint(s)],
        (None, Some(h)) => vec![Endpoint::glasses(h)],
        (None, None) => ClientConfig::default().endpoints,
    };
    let cmd = a.cmd[0].as_str();
    if cmd == "record" {
        let dir = a.cmd.get(1).ok_or("record precisa de uma pasta")?;
        return cmd_record(&endpoints, PathBuf::from(dir), secs(a.cmd.get(2), 30)?).await;
    }
    let g = Glasses::start(ClientConfig { endpoints, ..ClientConfig::default() });
    match cmd {
        "info" => cmd_info(&g).await,
        "listen" => cmd_listen(&g, secs(a.cmd.get(1), 60)?).await,
        "imu" => cmd_imu(&g, secs(a.cmd.get(1), 30)?).await,
        other => Err(format!("comando desconhecido: {other}\n\n{USO}")),
    }
}

#[tokio::main]
async fn main() {
    let a = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("{e}\n");
            }
            println!("{USO}");
            std::process::exit(2);
        }
    };
    if let Err(e) = run(a).await {
        eprintln!("erro: {e}");
        std::process::exit(1);
    }
}
