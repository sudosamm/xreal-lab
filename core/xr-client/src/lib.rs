//! xr-client — fala com o XREAL One Pro pela rede USB (portas 52996, 52998, 52999).
//!
//! - três tarefas tokio (controle, IMU, vsync) que religam sozinhas;
//! - estado agregado em [`DeviceState`] (`watch`), eventos em [`ClientEvent`] (`broadcast`),
//!   orientação em [`Orientation`] (`watch`, no máximo a cada `orientation_period`);
//! - ajustes SÓ pela trava de `xr_proto::gate`: [`Glasses::prepare`] e depois [`Glasses::confirm`];
//!   cada envio vai para o registro JSONL (`audit_path`).
//!
//! O que é lido: versões, sensor de rosto, EIS e a calibração (guardada só em memória, nunca
//! registrada). O id do óculos não é lido.

mod audit;
mod control;
mod rate;
mod streams;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use xr_fusion::{Calibration, FilterConfig};
use xr_proto::gate::{Adjustment, GateError, Proposal};

pub use xr_fusion::Orientation;
pub use xr_proto::events::Event as DeviceEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub imu_port: u16,
    pub vsync_port: u16,
    pub control_port: u16,
}

impl Endpoint {
    pub fn glasses(host: &str) -> Self {
        Self { host: host.into(), imu_port: 52998, vsync_port: 52996, control_port: 52999 }
    }
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// tentados em ordem: NCM (169.254.2.1, a do Beam) e ECM (169.254.1.1)
    pub endpoints: Vec<Endpoint>,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub retry_delay: Duration,
    /// sem dados da IMU por este tempo = conexão perdida
    pub imu_silence: Duration,
    pub orientation_period: Duration,
    pub filter: FilterConfig,
    /// registro JSONL dos ajustes enviados (`None` = não registrar)
    pub audit_path: Option<PathBuf>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            endpoints: vec![Endpoint::glasses("169.254.2.1"), Endpoint::glasses("169.254.1.1")],
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(5),
            retry_delay: Duration::from_secs(2),
            imu_silence: Duration::from_secs(5),
            orientation_period: Duration::from_millis(16),
            filter: FilterConfig::default(),
            audit_path: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Link {
    #[default]
    Disconnected,
    Connected {
        host: String,
    },
}

impl Link {
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceState {
    pub control: Link,
    pub imu: Link,
    pub vsync: Link,
    pub sw_version: Option<String>,
    pub dsp_version: Option<String>,
    pub proximity_enabled: Option<bool>,
    pub eis_enabled: Option<bool>,
    pub calibration_loaded: bool,
    /// 27 8a, índices 0–2 (°C, provável)
    pub temperatures: [Option<f32>; 3],
    /// último nível de brilho conhecido (evento 27 12 ou ajuste enviado por este app)
    pub brightness: Option<u8>,
    /// último nível do filtro eletrocrômico conhecido (0/1/2)
    pub dimmer: Option<u8>,
    pub imu_hz: f32,
    pub vsync_hz: f32,
    pub last_vsync_counter: Option<u64>,
    pub adjustments_enabled: bool,
    /// por que os ajustes estão bloqueados agora, se estiverem
    pub adjustments_blocked: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClientEvent {
    Device(DeviceEvent),
    AdjustmentSent { adjustment: Adjustment, result: Result<(), String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AdjustError {
    #[error(transparent)]
    Gate(#[from] GateError),
    #[error("sem resposta do óculos; o ajuste pode ou não ter sido aplicado")]
    Timeout,
    #[error("conexão perdida durante o ajuste; o ajuste pode ou não ter sido aplicado")]
    ConnectionLost,
    #[error("o óculos recusou: {0}")]
    Rejected(String),
    #[error("cliente encerrado")]
    Closed,
}

pub(crate) enum Cmd {
    SetAdjustments(bool),
    Prepare(Adjustment, oneshot::Sender<Result<Proposal, AdjustError>>),
    Confirm(u64, oneshot::Sender<Result<Adjustment, AdjustError>>),
    Cancel,
}

pub(crate) struct Shared {
    pub cfg: ClientConfig,
    pub state: watch::Sender<DeviceState>,
    pub events: broadcast::Sender<ClientEvent>,
    pub orientation: watch::Sender<Option<Orientation>>,
    pub calibration: watch::Sender<Option<Arc<Calibration>>>,
    pub recenter: AtomicU64,
    /// Android: `Network.getNetworkHandle()` da rede do óculos (0 = rede padrão)
    pub network: AtomicU64,
}

pub struct Glasses {
    shared: Arc<Shared>,
    cmd: mpsc::Sender<Cmd>,
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for Glasses {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

impl Glasses {
    /// Começa a conectar. Precisa estar dentro de um runtime tokio.
    pub fn start(cfg: ClientConfig) -> Self {
        let shared = Arc::new(Shared {
            cfg,
            state: watch::channel(DeviceState::default()).0,
            events: broadcast::channel(256).0,
            orientation: watch::channel(None).0,
            calibration: watch::channel(None).0,
            recenter: AtomicU64::new(0),
            network: AtomicU64::new(0),
        });
        let (cmd, cmd_rx) = mpsc::channel(16);
        let tasks = vec![
            tokio::spawn(control::run(shared.clone(), cmd_rx)),
            tokio::spawn(streams::run_imu(shared.clone())),
            tokio::spawn(streams::run_vsync(shared.clone())),
        ];
        Self { shared, cmd, tasks }
    }

    pub fn state(&self) -> watch::Receiver<DeviceState> {
        self.shared.state.subscribe()
    }

    pub fn events(&self) -> broadcast::Receiver<ClientEvent> {
        self.shared.events.subscribe()
    }

    pub fn orientation(&self) -> watch::Receiver<Option<Orientation>> {
        self.shared.orientation.subscribe()
    }

    /// Android: liga as próximas conexões à rede do óculos (`Network.getNetworkHandle()`).
    /// `None` volta para a rede padrão. Conexões já abertas não mudam; as que caírem
    /// religam na rede nova. Fora do Android não tem efeito.
    pub fn set_network(&self, handle: Option<u64>) {
        self.shared.network.store(handle.unwrap_or(0), Ordering::Relaxed);
    }

    /// Zera a guinada na direção atual.
    pub fn recenter(&self) {
        self.shared.recenter.fetch_add(1, Ordering::Relaxed);
    }

    /// Liga ou desliga o "modo ajustes" (começa desligado e desliga ao perder a conexão).
    pub async fn set_adjustments_enabled(&self, on: bool) {
        let _ = self.cmd.send(Cmd::SetAdjustments(on)).await;
    }

    /// Etapa 1: descreve o ajuste, o valor atual conhecido e como desfazer. Nada é enviado.
    pub async fn prepare(&self, adj: Adjustment) -> Result<Proposal, AdjustError> {
        let (tx, rx) = oneshot::channel();
        self.cmd.send(Cmd::Prepare(adj, tx)).await.map_err(|_| AdjustError::Closed)?;
        rx.await.map_err(|_| AdjustError::Closed)?
    }

    /// Etapa 2: só deve ser chamada pela ação explícita do usuário (botão "Confirmar").
    pub async fn confirm(&self, proposal_id: u64) -> Result<Adjustment, AdjustError> {
        let (tx, rx) = oneshot::channel();
        self.cmd.send(Cmd::Confirm(proposal_id, tx)).await.map_err(|_| AdjustError::Closed)?;
        rx.await.map_err(|_| AdjustError::Closed)?
    }

    pub async fn cancel(&self) {
        let _ = self.cmd.send(Cmd::Cancel).await;
    }
}

/// Tenta cada endereço em ordem. Devolve o socket e o host que respondeu.
pub(crate) async fn connect_any(sh: &Shared, port: impl Fn(&Endpoint) -> u16) -> Option<(TcpStream, String)> {
    let net = sh.network.load(Ordering::Relaxed);
    for ep in &sh.cfg.endpoints {
        let addr = format!("{}:{}", ep.host, port(ep));
        match tokio::time::timeout(sh.cfg.connect_timeout, connect_one(&addr, net)).await {
            Ok(Ok(s)) => {
                let _ = s.set_nodelay(true);
                return Some((s, ep.host.clone()));
            }
            Ok(Err(e)) => tracing::debug!("{addr}: {e}"),
            Err(_) => tracing::debug!("{addr}: tempo esgotado"),
        }
    }
    None
}

async fn connect_one(addr: &str, network: u64) -> std::io::Result<TcpStream> {
    if network == 0 {
        return TcpStream::connect(addr).await;
    }
    let sa = tokio::net::lookup_host(addr)
        .await?
        .next()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "endereço inválido"))?;
    let sock = if sa.is_ipv4() { tokio::net::TcpSocket::new_v4()? } else { tokio::net::TcpSocket::new_v6()? };
    bind_to_network(&sock, network)?;
    sock.connect(sa).await
}

/// No Android, um app comum usa a rede padrão (Wi-Fi); a rede USB do óculos não tem
/// internet e não é a padrão. `android_setsocknetwork` (libandroid, API 23+) liga o
/// socket à rede escolhida antes de conectar.
#[cfg(target_os = "android")]
fn bind_to_network(sock: &tokio::net::TcpSocket, network: u64) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    #[link(name = "android")]
    extern "C" {
        fn android_setsocknetwork(network: u64, fd: std::os::raw::c_int) -> std::os::raw::c_int;
    }
    // SAFETY: o descritor é válido enquanto `sock` existe; a função só lê os dois argumentos.
    let r = unsafe { android_setsocknetwork(network, sock.as_raw_fd()) };
    if r == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "android"))]
fn bind_to_network(_sock: &tokio::net::TcpSocket, _network: u64) -> std::io::Result<()> {
    Ok(()) // fora do Android a rota do sistema já resolve
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fora do Android, o caminho com rede escolhida usa TcpSocket e conecta normalmente.
    #[tokio::test]
    async fn conecta_com_rede_escolhida() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap().to_string();
        let accept = tokio::spawn(async move { l.accept().await.map(|_| ()) });
        let s = connect_one(&addr, 7).await.unwrap();
        assert_eq!(s.peer_addr().unwrap().to_string(), addr);
        accept.await.unwrap().unwrap();
    }
}
