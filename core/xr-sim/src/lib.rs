//! xr-sim — óculos simulado para testes e para desenvolver a interface sem o aparelho.
//!
//! Abre três portas em 127.0.0.1 (portas livres escolhidas pelo sistema):
//! - IMU: amostras paradas e niveladas a 1 kHz (+ magnetômetro a cada 5), no formato da 52998;
//! - vsync: 120 Hz, formato da 52996;
//! - controle: responde às leituras conhecidas, ANOTA todo ajuste recebido (e qualquer coisa
//!   inesperada) e manda temperaturas periódicas e eventos pedidos pelo teste.
//!
//! Os valores são falsos; nada aqui vem do aparelho do usuário.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::{interval, MissedTickBehavior};
use xr_proto::control::Readable;
use xr_proto::frame::{encode, FrameDecoder};
use xr_proto::pb::{read_varint, write_varint};
use xr_proto::{imu, vsync, Magic};

#[derive(Debug, Clone)]
pub struct SimConfig {
    pub sw_version: String,
    pub dsp_version: String,
    /// resposta do 27 1f (calibração)
    pub config_json: String,
    pub proximity: bool,
    pub eis: bool,
    /// aceleração parada, nos eixos do IMU (m/s²)
    pub accel_imu: [f32; 3],
    /// giroscópio parado (rad/s): o "bias" do simulado
    pub gyro_imu: [f32; 3],
    pub temp_period: Duration,
}

impl Default for SimConfig {
    fn default() -> Self {
        // nivelado na tela (0, −g, 0) e IMU girado 35° em x, como na calibração falsa
        let (s, c) = 35f32.to_radians().sin_cos();
        let g = 9.806_65f32;
        let q = [(17.5f64.to_radians()).sin(), 0.0, 0.0, (17.5f64.to_radians()).cos()];
        Self {
            sw_version: xr_proto::gate::TESTED_FIRMWARE[0].into(),
            dsp_version: "15.SIMULADO".into(),
            config_json: format!(
                r#"{{"FSN":"SIMULADO","display":{{"target_q_left_display":[{},{},{},{}]}}}}"#,
                q[0], q[1], q[2], q[3]
            ),
            proximity: true,
            eis: false,
            accel_imu: [0.0, -g * c, -g * s],
            gyro_imu: [0.0, 0.0, 0.0],
            temp_period: Duration::from_millis(500),
        }
    }
}

/// O que o simulado recebeu na porta de controle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Received {
    Get(Magic),
    /// ajuste: magic, valor e os bytes exatos
    Set { magic: Magic, value: u64, raw: Vec<u8> },
    Unexpected(Vec<u8>),
}

struct Shared {
    cfg: SimConfig,
    received: Mutex<Vec<Received>>,
    events: broadcast::Sender<Vec<u8>>,
    /// conexões abertas, para fechar todas quando o simulado é descartado
    conns: Mutex<Vec<tokio::task::AbortHandle>>,
}

pub struct Sim {
    pub imu_addr: SocketAddr,
    pub vsync_addr: SocketAddr,
    pub control_addr: SocketAddr,
    shared: Arc<Shared>,
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for Sim {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
        for c in self.shared.conns.lock().unwrap().drain(..) {
            c.abort();
        }
    }
}

impl Sim {
    pub async fn start(cfg: SimConfig) -> std::io::Result<Sim> {
        let (events, _) = broadcast::channel(64);
        let shared = Arc::new(Shared { cfg, received: Mutex::new(Vec::new()), events, conns: Mutex::new(Vec::new()) });
        let imu_l = TcpListener::bind("127.0.0.1:0").await?;
        let vsync_l = TcpListener::bind("127.0.0.1:0").await?;
        let control_l = TcpListener::bind("127.0.0.1:0").await?;
        let (imu_addr, vsync_addr, control_addr) = (imu_l.local_addr()?, vsync_l.local_addr()?, control_l.local_addr()?);
        let tasks = vec![
            tokio::spawn(accept_loop(imu_l, shared.clone(), Service::Imu)),
            tokio::spawn(accept_loop(vsync_l, shared.clone(), Service::Vsync)),
            tokio::spawn(accept_loop(control_l, shared.clone(), Service::Control)),
        ];
        Ok(Sim { imu_addr, vsync_addr, control_addr, shared, tasks })
    }

    pub fn received(&self) -> Vec<Received> {
        self.shared.received.lock().unwrap().clone()
    }

    /// Só os ajustes e o que não foi reconhecido (o que NÃO deveria chegar sem confirmação).
    pub fn writes(&self) -> Vec<Received> {
        self.received().into_iter().filter(|r| !matches!(r, Received::Get(_))).collect()
    }

    pub fn push_raw(&self, magic: Magic, body: &[u8]) {
        let _ = self.shared.events.send(encode(magic, body));
    }

    /// 27 2e: tecla (1 menu, 2 vol+, 3 vol−, 4 atalho), estado (1 apertado, 2 solto)
    pub fn push_key(&self, key: u32, state: u32) {
        let mut b = Vec::with_capacity(64);
        for v in [key, state, 1_800_000_000] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.resize(64, 0);
        self.push_raw(xr_proto::events::KEY_MAGIC, &b);
    }

    /// 27 12: ação (ex.: 7 = brilho −, parâmetro = novo nível)
    pub fn push_action(&self, code: u32, param: u32) {
        let mut b = Vec::with_capacity(64);
        b.extend_from_slice(&code.to_le_bytes());
        b.extend_from_slice(&param.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&5_000_000_000u64.to_le_bytes());
        b.resize(64, 0);
        self.push_raw(xr_proto::events::ACTION_MAGIC, &b);
    }
}

#[derive(Clone, Copy)]
enum Service {
    Imu,
    Vsync,
    Control,
}

async fn accept_loop(l: TcpListener, shared: Arc<Shared>, svc: Service) {
    while let Ok((s, _)) = l.accept().await {
        let _ = s.set_nodelay(true);
        let sh = shared.clone();
        let task = tokio::spawn(async move {
            let _ = match svc {
                Service::Imu => serve_imu(s, sh).await,
                Service::Vsync => serve_vsync(s).await,
                Service::Control => serve_control(s, sh).await,
            };
        });
        shared.conns.lock().unwrap().push(task.abort_handle());
    }
}

fn imu_body(ts: u64, kind: u32, cfg: &SimConfig) -> Vec<u8> {
    let mut b = vec![0u8; imu::BODY_LEN];
    b[8..16].copy_from_slice(&ts.to_le_bytes());
    b[24..28].copy_from_slice(&kind.to_le_bytes());
    let put = |b: &mut [u8], o: usize, v: [f32; 3]| {
        for (i, x) in v.iter().enumerate() {
            b[o + 4 * i..o + 4 * i + 4].copy_from_slice(&x.to_le_bytes());
        }
    };
    if kind == 11 {
        put(&mut b, 28, cfg.gyro_imu);
        put(&mut b, 40, cfg.accel_imu);
        put(&mut b, 52, [imu::MAG_NO_DATA; 3]);
        b[64..68].copy_from_slice(&41.5f32.to_le_bytes());
    } else {
        put(&mut b, 52, [20.0, -5.0, 30.0]);
    }
    b
}

async fn serve_imu(mut s: TcpStream, shared: Arc<Shared>) -> std::io::Result<()> {
    let mut tick = interval(Duration::from_millis(10));
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut ts: u64 = 10_000_000_000;
    let mut n: u64 = 0;
    loop {
        tick.tick().await;
        let mut out = Vec::with_capacity(20 * 134);
        for _ in 0..10 {
            ts += 1_000_000;
            n += 1;
            out.extend(encode(imu::MAGIC, &imu_body(ts, 11, &shared.cfg)));
            if n % 5 == 0 {
                out.extend(encode(imu::MAGIC, &imu_body(ts, 4, &shared.cfg)));
            }
        }
        s.write_all(&out).await?;
    }
}

async fn serve_vsync(mut s: TcpStream) -> std::io::Result<()> {
    let mut tick = interval(Duration::from_nanos(8_333_333));
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let (mut ts, mut counter) = (700_000_000_000u64, 6000u64);
    loop {
        tick.tick().await;
        ts += 8_333_333;
        counter += 1;
        let mut b = vec![0u8; vsync::BODY_LEN];
        b[8..16].copy_from_slice(&ts.to_le_bytes());
        b[16..24].copy_from_slice(&counter.to_le_bytes());
        s.write_all(&encode(vsync::MAGIC, &b)).await?;
    }
}

fn str_value(text: &str) -> Vec<u8> {
    let mut inner = vec![0x0a];
    write_varint(&mut inner, text.len() as u64);
    inner.extend_from_slice(text.as_bytes());
    let mut out = vec![0x22];
    write_varint(&mut out, inner.len() as u64);
    out.extend(inner);
    out
}

fn bool_value(v: bool) -> Vec<u8> {
    if v {
        vec![0x22, 0x02, 0x08, 0x01]
    } else {
        vec![0x22, 0x00]
    }
}

fn response(magic: Magic, value: &[u8]) -> Vec<u8> {
    let mut body = 1u32.to_be_bytes().to_vec();
    body.extend_from_slice(value);
    encode(magic, &body)
}

/// Interpreta um pedido. Devolve a resposta, se houver.
fn handle_request(shared: &Shared, magic: Magic, body: &[u8], raw: Vec<u8>) -> Option<Vec<u8>> {
    let cfg = &shared.cfg;
    let mut rec = shared.received.lock().unwrap();
    let payload = body.get(4..).unwrap_or(&[]);
    if payload == [0x18, 0x00] {
        rec.push(Received::Get(magic));
        let value = match Readable::from_magic(magic)? {
            Readable::SwVersion => str_value(&cfg.sw_version),
            Readable::DspVersion => str_value(&cfg.dsp_version),
            Readable::GlassesId => str_value("SIMULADO-0000"),
            Readable::Config => str_value(&cfg.config_json),
            Readable::ProximityEnabled => bool_value(cfg.proximity),
            Readable::EisEnabled => bool_value(cfg.eis),
        };
        return Some(response(magic, &value));
    }
    // ajuste: 1a <len> 08 <valor>
    if payload.first() == Some(&0x1a) && payload.len() >= 4 {
        let mut i = 1;
        if read_varint(payload, &mut i).is_ok() && payload.get(i) == Some(&0x08) {
            i += 1;
            if let Ok(value) = read_varint(payload, &mut i) {
                rec.push(Received::Set { magic, value, raw });
                return Some(response(magic, &[0x22, 0x00]));
            }
        }
    }
    rec.push(Received::Unexpected(raw));
    None
}

async fn serve_control(s: TcpStream, shared: Arc<Shared>) -> std::io::Result<()> {
    let (mut rd, mut wr) = s.into_split();
    let mut events = shared.events.subscribe();
    let mut temps = interval(shared.cfg.temp_period);
    temps.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut dec = FrameDecoder::new();
    let mut buf = vec![0u8; 8192];
    let mut t = 0f32;
    loop {
        tokio::select! {
            n = rd.read(&mut buf) => {
                let n = n?;
                if n == 0 {
                    return Ok(());
                }
                dec.push(&buf[..n]);
                while let Some(f) = dec.next_frame().map_err(std::io::Error::other)? {
                    let raw = encode(f.magic, &f.body);
                    if let Some(resp) = handle_request(&shared, f.magic, &f.body, raw) {
                        wr.write_all(&resp).await?;
                    }
                }
            }
            ev = events.recv() => {
                if let Ok(ev) = ev {
                    wr.write_all(&ev).await?;
                }
            }
            _ = temps.tick() => {
                t += 0.1;
                for (idx, base) in [(0u8, 50.0f32), (1, 44.0), (2, 36.0)] {
                    let mut b = vec![0x1a, if idx == 0 { 5 } else { 7 }];
                    if idx != 0 {
                        b.extend_from_slice(&[0x08, idx]);
                    }
                    b.push(0x15);
                    b.extend_from_slice(&(base + t).to_le_bytes());
                    wr.write_all(&encode(xr_proto::events::SENSOR_MAGIC, &b)).await?;
                }
            }
        }
    }
}
