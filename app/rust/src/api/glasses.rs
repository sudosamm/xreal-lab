//! API vista pelo Flutter. Fina de propósito: a lógica fica em core/ (xr-client, xr-proto).
//!
//! Ajustes: só `prepare` → diálogo na interface → `confirm`. Não existe função para mandar bytes.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use flutter_rust_bridge::frb;
use tokio::runtime::Runtime;
use xr_client::{AdjustError, ClientConfig, ClientEvent, DeviceEvent, DeviceState, Endpoint, Glasses, Link};
use xr_proto::gate::{Adjustment, ButtonsMode, InputMode, Proposal, Undo};

use crate::frb_generated::StreamSink;

fn rt() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("xr-core")
            .enable_all()
            .build()
            .expect("runtime tokio")
    })
}

#[frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}

pub struct SessionConfig {
    /// vazio = padrão (169.254.2.1 e depois 169.254.1.1)
    pub hosts: Vec<String>,
    /// usa o óculos simulado (xr-sim) em vez do aparelho
    pub simulated: bool,
    /// arquivo JSONL onde cada ajuste enviado é registrado
    pub audit_path: Option<String>,
}

pub struct StateDto {
    pub control_host: Option<String>,
    pub imu_host: Option<String>,
    pub vsync_host: Option<String>,
    pub sw_version: Option<String>,
    pub dsp_version: Option<String>,
    pub proximity_enabled: Option<bool>,
    pub eis_enabled: Option<bool>,
    pub calibration_loaded: bool,
    /// 3 valores; NaN = ainda não recebido
    pub temperatures: Vec<f32>,
    pub brightness: Option<u8>,
    pub dimmer: Option<u8>,
    pub imu_hz: f32,
    pub vsync_hz: f32,
    pub adjustments_enabled: bool,
    pub adjustments_blocked: Option<String>,
    pub simulated: bool,
}

pub struct OrientationDto {
    pub yaw: f64,
    pub pitch: f64,
    pub roll: f64,
    pub rate_pitch: f64,
    pub rate_yaw: f64,
    pub rate_roll: f64,
    pub still: bool,
    pub temp_c: f32,
}

pub enum EventKind {
    Key,
    Action,
    Sensor,
    Unknown,
    AdjustmentOk,
    AdjustmentFailed,
}

pub struct EventDto {
    pub kind: EventKind,
    /// texto pronto para mostrar (mesmo do xr_control_peek.py)
    pub text: String,
    pub brightness: Option<u32>,
    pub dimmer: Option<u32>,
}

pub enum AdjustmentKind {
    Brightness,
    Dimmer,
    /// valor 0 = botões ativos, 1 = desativados
    Buttons,
    /// valor 0 = normal, 1 = lado a lado
    SideBySide,
}

pub struct AdjustmentDto {
    pub kind: AdjustmentKind,
    pub value: u8,
}

pub struct ProposalDto {
    pub id: u32,
    pub adjustment: AdjustmentDto,
    pub description: String,
    /// "desconhecido" quando não há leitura
    pub current: String,
    pub undo_text: String,
    /// ajuste que desfaz (também passa por prepare/confirm), se houver
    pub undo: Option<AdjustmentDto>,
    pub warnings: Vec<String>,
    pub expires_in_ms: u32,
}

fn host(l: &Link) -> Option<String> {
    match l {
        Link::Connected { host } => Some(host.clone()),
        Link::Disconnected => None,
    }
}

fn state_dto(s: &DeviceState, simulated: bool) -> StateDto {
    StateDto {
        control_host: host(&s.control),
        imu_host: host(&s.imu),
        vsync_host: host(&s.vsync),
        sw_version: s.sw_version.clone(),
        dsp_version: s.dsp_version.clone(),
        proximity_enabled: s.proximity_enabled,
        eis_enabled: s.eis_enabled,
        calibration_loaded: s.calibration_loaded,
        temperatures: s.temperatures.iter().map(|t| t.unwrap_or(f32::NAN)).collect(),
        brightness: s.brightness,
        dimmer: s.dimmer,
        imu_hz: s.imu_hz,
        vsync_hz: s.vsync_hz,
        adjustments_enabled: s.adjustments_enabled,
        adjustments_blocked: s.adjustments_blocked.clone(),
        simulated,
    }
}

fn to_adjustment(a: &AdjustmentDto) -> Adjustment {
    match a.kind {
        AdjustmentKind::Brightness => Adjustment::Brightness(a.value),
        AdjustmentKind::Dimmer => Adjustment::Dimmer(a.value),
        AdjustmentKind::Buttons => {
            Adjustment::Buttons(if a.value == 0 { ButtonsMode::Enabled } else { ButtonsMode::Disabled })
        }
        AdjustmentKind::SideBySide => {
            Adjustment::InputMode(if a.value == 0 { InputMode::Normal } else { InputMode::SideBySide })
        }
    }
}

fn from_adjustment(a: Adjustment) -> AdjustmentDto {
    match a {
        Adjustment::Brightness(v) => AdjustmentDto { kind: AdjustmentKind::Brightness, value: v },
        Adjustment::Dimmer(v) => AdjustmentDto { kind: AdjustmentKind::Dimmer, value: v },
        Adjustment::Buttons(m) => AdjustmentDto { kind: AdjustmentKind::Buttons, value: m as u8 },
        Adjustment::InputMode(m) => AdjustmentDto { kind: AdjustmentKind::SideBySide, value: m as u8 },
    }
}

fn proposal_dto(p: Proposal) -> ProposalDto {
    let (undo_text, undo) = match p.undo {
        Undo::Send(a) => (format!("voltar para {}", a.describe_pt()), Some(from_adjustment(a))),
        Undo::Manual(s) => (s, None),
    };
    ProposalDto {
        id: u32::try_from(p.id).unwrap_or(u32::MAX),
        adjustment: from_adjustment(p.adjustment),
        description: p.description,
        current: p.current.map_or_else(|| "desconhecido".into(), |a| a.describe_pt()),
        undo_text,
        undo,
        warnings: p.warnings,
        expires_in_ms: p.expires_at.saturating_duration_since(Instant::now()).as_millis() as u32,
    }
}

fn event_dto(e: &ClientEvent) -> EventDto {
    match e {
        ClientEvent::Device(d) => {
            let (kind, brightness, dimmer) = match d {
                DeviceEvent::Key { .. } => (EventKind::Key, None, None),
                DeviceEvent::Action { action, .. } => {
                    (EventKind::Action, action.brightness_level(), action.dimmer_level())
                }
                DeviceEvent::Sensor { .. } => (EventKind::Sensor, None, None),
                DeviceEvent::Unknown { .. } => (EventKind::Unknown, None, None),
            };
            EventDto { kind, text: d.describe_pt(), brightness, dimmer }
        }
        ClientEvent::AdjustmentSent { adjustment, result } => match result {
            Ok(()) => EventDto {
                kind: EventKind::AdjustmentOk,
                text: format!("ajuste enviado: {}", adjustment.describe_pt()),
                brightness: None,
                dimmer: None,
            },
            Err(e) => EventDto {
                kind: EventKind::AdjustmentFailed,
                text: format!("ajuste {}: {e}", adjustment.describe_pt()),
                brightness: None,
                dimmer: None,
            },
        },
    }
}

#[frb(opaque)]
pub struct Session {
    glasses: Glasses,
    simulated: bool,
    _sim: Option<xr_sim::Sim>,
}

impl Session {
    pub fn start(config: SessionConfig) -> Result<Session, String> {
        let sim = if config.simulated {
            Some(rt().block_on(xr_sim::Sim::start(xr_sim::SimConfig::default())).map_err(|e| e.to_string())?)
        } else {
            None
        };
        let endpoints = match &sim {
            Some(s) => vec![Endpoint {
                host: "127.0.0.1".into(),
                imu_port: s.imu_addr.port(),
                vsync_port: s.vsync_addr.port(),
                control_port: s.control_addr.port(),
            }],
            None if !config.hosts.is_empty() => config.hosts.iter().map(|h| Endpoint::glasses(h)).collect(),
            None => ClientConfig::default().endpoints,
        };
        let cfg = ClientConfig { endpoints, audit_path: config.audit_path.map(Into::into), ..ClientConfig::default() };
        let _guard = rt().enter();
        Ok(Session { glasses: Glasses::start(cfg), simulated: config.simulated, _sim: sim })
    }

    /// Estado do aparelho, no máximo 10 vezes por segundo.
    pub fn watch_state(&self, sink: StreamSink<StateDto>) {
        let mut rx = self.glasses.state();
        let simulated = self.simulated;
        rt().spawn(async move {
            loop {
                let dto = state_dto(&rx.borrow_and_update(), simulated);
                if sink.add(dto).is_err() || rx.changed().await.is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
    }

    /// Orientação da cabeça, ~60 vezes por segundo.
    pub fn watch_orientation(&self, sink: StreamSink<OrientationDto>) {
        let mut rx = self.glasses.orientation();
        rt().spawn(async move {
            while rx.changed().await.is_ok() {
                let o = *rx.borrow_and_update();
                if let Some(o) = o {
                    let dto = OrientationDto {
                        yaw: o.yaw_deg,
                        pitch: o.pitch_deg,
                        roll: o.roll_deg,
                        rate_pitch: o.rate_dps.x,
                        rate_yaw: o.rate_dps.y,
                        rate_roll: o.rate_dps.z,
                        still: o.still,
                        temp_c: o.temp_c,
                    };
                    if sink.add(dto).is_err() {
                        break;
                    }
                }
            }
        });
    }

    /// Teclas, ações (brilho, filtro), temperaturas e resultados de ajustes.
    pub fn watch_events(&self, sink: StreamSink<EventDto>) {
        let mut rx = self.glasses.events();
        rt().spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(e) => {
                        if sink.add(event_dto(&e)).is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        });
    }

    /// Android: rede do óculos (`Network.getNetworkHandle()`), ou `None` para a padrão.
    #[frb(sync)]
    pub fn set_network(&self, handle: Option<i64>) {
        self.glasses.set_network(handle.and_then(|h| u64::try_from(h).ok()).filter(|h| *h != 0));
    }

    #[frb(sync)]
    pub fn recenter(&self) {
        self.glasses.recenter();
    }

    pub fn set_adjustments_enabled(&self, on: bool) {
        rt().block_on(self.glasses.set_adjustments_enabled(on));
    }

    /// Etapa 1: descreve o ajuste. Nada é enviado ao óculos.
    pub fn prepare(&self, adjustment: AdjustmentDto) -> Result<ProposalDto, String> {
        rt().block_on(self.glasses.prepare(to_adjustment(&adjustment))).map(proposal_dto).map_err(|e| e.to_string())
    }

    /// Etapa 2: chamada SÓ pelo botão "Confirmar" do diálogo.
    pub fn confirm(&self, proposal_id: u32) -> Result<String, String> {
        rt().block_on(self.glasses.confirm(u64::from(proposal_id)))
            .map(|a| format!("{} aplicado", a.describe_pt()))
            .map_err(|e: AdjustError| e.to_string())
    }

    pub fn cancel(&self) {
        rt().block_on(self.glasses.cancel());
    }
}
