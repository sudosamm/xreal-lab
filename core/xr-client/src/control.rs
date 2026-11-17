//! Tarefa do canal de controle (52999): leituras, eventos e os ajustes liberados pela trava.

use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, sleep_until, Instant};
use xr_fusion::Calibration;
use xr_proto::control::{encode_get, parse_value, split_response, Readable, Value};
use xr_proto::events::{self, Event};
use xr_proto::frame::FrameDecoder;
use xr_proto::gate::{Adjustment, Gate, Outgoing, Proposal};
use xr_proto::{Frame, Magic};

use crate::{audit, connect_any, AdjustError, ClientEvent, Cmd, Link, Shared};

/// Lidas ao conectar. O id do óculos (27 29) fica de fora de propósito.
const ON_CONNECT: [Readable; 5] = [
    Readable::SwVersion,
    Readable::DspVersion,
    Readable::ProximityEnabled,
    Readable::EisEnabled,
    Readable::Config,
];

type AdjustReply = oneshot::Sender<Result<Adjustment, AdjustError>>;

struct Job {
    out: Outgoing,
    proposal: Option<Proposal>,
    reply: AdjustReply,
}

enum InFlight {
    Read(Readable, Instant),
    Adjust(Job, Instant),
}

impl InFlight {
    fn magic(&self) -> Magic {
        match self {
            Self::Read(r, _) => r.magic(),
            Self::Adjust(j, _) => j.out.magic(),
        }
    }

    fn deadline(&self) -> Instant {
        match self {
            Self::Read(_, d) | Self::Adjust(_, d) => *d,
        }
    }
}

/// Copia o estado da trava para o DeviceState. Devolve se mudou.
fn apply_gate(st: &mut crate::DeviceState, gate: &Gate) -> bool {
    let enabled = gate.enabled();
    let blocked = gate.check_ready().err().map(|e| e.to_string());
    let changed = st.adjustments_enabled != enabled || st.adjustments_blocked != blocked;
    st.adjustments_enabled = enabled;
    st.adjustments_blocked = blocked;
    changed
}

fn sync_gate(sh: &Shared, gate: &Gate) {
    sh.state.send_if_modified(|st| apply_gate(st, gate));
}

fn finish(sh: &Shared, job: Job, result: Result<(), AdjustError>) {
    let adj = job.out.adjustment();
    let text = match &result {
        Ok(()) => "ok".to_string(),
        Err(e) => e.to_string(),
    };
    let firmware = sh.state.borrow().sw_version.clone();
    audit::write(sh.cfg.audit_path.as_deref(), firmware.as_deref(), adj, job.out.bytes(), job.proposal.as_ref(), &text);
    let _ = sh.events.send(ClientEvent::AdjustmentSent {
        adjustment: adj,
        result: result.clone().map_err(|e| e.to_string()),
    });
    let _ = job.reply.send(result.map(|()| adj));
}

/// Comandos que não dependem da conexão. Devolve um envio liberado pela trava, se houver.
fn handle_cmd(sh: &Shared, gate: &mut Gate, cmd: Cmd) -> Option<Job> {
    let now = std::time::Instant::now();
    let job = match cmd {
        Cmd::SetAdjustments(on) => {
            gate.set_enabled(on);
            None
        }
        Cmd::Prepare(adj, reply) => {
            let _ = reply.send(gate.prepare(adj, now).map_err(AdjustError::from));
            None
        }
        Cmd::Confirm(id, reply) => {
            let proposal = gate.pending().cloned();
            match gate.confirm(id, now) {
                Ok(out) => Some(Job { out, proposal, reply }),
                Err(e) => {
                    let _ = reply.send(Err(e.into()));
                    None
                }
            }
        }
        Cmd::Cancel => {
            gate.cancel();
            None
        }
    };
    sync_gate(sh, gate);
    job
}

/// Espera `fut` atendendo comandos (sem conexão nenhum ajuste sai: a trava exige firmware lido).
async fn wait_with_cmds<F: std::future::Future>(
    sh: &Shared,
    gate: &mut Gate,
    cmd_rx: &mut mpsc::Receiver<Cmd>,
    fut: F,
) -> ControlFlow<(), F::Output> {
    tokio::pin!(fut);
    loop {
        tokio::select! {
            r = &mut fut => return ControlFlow::Continue(r),
            c = cmd_rx.recv() => match c {
                None => return ControlFlow::Break(()),
                Some(c) => {
                    if let Some(job) = handle_cmd(sh, gate, c) {
                        finish(sh, job, Err(AdjustError::Rejected("não enviado: sem conexão".into())));
                    }
                }
            },
        }
    }
}

pub async fn run(sh: Arc<Shared>, mut cmd_rx: mpsc::Receiver<Cmd>) {
    let mut gate = Gate::new();
    sync_gate(&sh, &gate);
    loop {
        let conn = match wait_with_cmds(&sh, &mut gate, &mut cmd_rx, connect_any(&sh, |e| e.control_port)).await {
            ControlFlow::Break(()) => return,
            ControlFlow::Continue(c) => c,
        };
        if let Some((s, host)) = conn {
            sh.state.send_modify(|st| st.control = Link::Connected { host });
            let flow = session(&sh, &mut gate, &mut cmd_rx, s).await;
            gate.set_firmware(None);
            gate.set_enabled(false);
            sh.state.send_modify(|st| {
                st.control = Link::Disconnected;
                st.sw_version = None;
                st.dsp_version = None;
                st.proximity_enabled = None;
                st.eis_enabled = None;
                st.temperatures = [None; 3];
                st.brightness = None;
                st.dimmer = None;
                // na mesma atualização: a interface nunca vê "desconectado" com ajustes ligados
                apply_gate(st, &gate);
            });
            if flow.is_break() {
                return;
            }
        }
        if wait_with_cmds(&sh, &mut gate, &mut cmd_rx, sleep(sh.cfg.retry_delay)).await.is_break() {
            return;
        }
    }
}

async fn session(sh: &Shared, gate: &mut Gate, cmd_rx: &mut mpsc::Receiver<Cmd>, s: TcpStream) -> ControlFlow<()> {
    let (mut rd, mut wr) = s.into_split();
    let mut reads: VecDeque<Readable> = ON_CONNECT.into_iter().collect();
    let mut jobs: VecDeque<Job> = VecDeque::new();
    let mut inflight: Option<InFlight> = None;
    let mut dec = FrameDecoder::new();
    let mut buf = vec![0u8; 64 * 1024];
    let timeout = sh.cfg.request_timeout;

    let fail_all = |sh: &Shared, inflight: Option<InFlight>, jobs: VecDeque<Job>| {
        if let Some(InFlight::Adjust(job, _)) = inflight {
            finish(sh, job, Err(AdjustError::ConnectionLost));
        }
        for job in jobs {
            finish(sh, job, Err(AdjustError::Rejected("não enviado: conexão perdida".into())));
        }
    };

    loop {
        // um pedido por vez; ajustes confirmados passam na frente das leituras
        if inflight.is_none() {
            let next = if let Some(job) = jobs.pop_front() {
                Some((job.out.bytes().to_vec(), InFlight::Adjust(job, Instant::now() + timeout)))
            } else {
                reads.pop_front().map(|r| (encode_get(r), InFlight::Read(r, Instant::now() + timeout)))
            };
            if let Some((bytes, what)) = next {
                if let Err(e) = wr.write_all(&bytes).await {
                    tracing::info!("controle: {e}");
                    let lost = match what {
                        InFlight::Adjust(job, _) => {
                            // a escrita falhou: pode ter saído parcialmente
                            Some(InFlight::Adjust(job, Instant::now()))
                        }
                        InFlight::Read(..) => None,
                    };
                    fail_all(sh, lost, jobs);
                    return ControlFlow::Continue(());
                }
                inflight = Some(what);
            }
        }
        let deadline = inflight.as_ref().map_or_else(|| Instant::now() + Duration::from_secs(3600), InFlight::deadline);
        tokio::select! {
            n = rd.read(&mut buf) => {
                let n = match n {
                    Ok(0) | Err(_) => {
                        fail_all(sh, inflight.take(), std::mem::take(&mut jobs));
                        return ControlFlow::Continue(());
                    }
                    Ok(n) => n,
                };
                dec.push(&buf[..n]);
                loop {
                    match dec.next_frame() {
                        Ok(Some(f)) => handle_frame(sh, gate, &mut inflight, f),
                        Ok(None) => break,
                        Err(e) => {
                            tracing::warn!("controle fora de sincronia: {e}");
                            fail_all(sh, inflight.take(), std::mem::take(&mut jobs));
                            return ControlFlow::Continue(());
                        }
                    }
                }
            }
            c = cmd_rx.recv() => match c {
                None => {
                    fail_all(sh, inflight.take(), std::mem::take(&mut jobs));
                    return ControlFlow::Break(());
                }
                Some(c) => {
                    if let Some(job) = handle_cmd(sh, gate, c) {
                        jobs.push_back(job);
                    }
                }
            },
            _ = sleep_until(deadline), if inflight.is_some() => {
                match inflight.take() {
                    Some(InFlight::Read(r, _)) => tracing::warn!("sem resposta para {}", r.name()),
                    Some(InFlight::Adjust(job, _)) => finish(sh, job, Err(AdjustError::Timeout)),
                    None => {}
                }
            }
        }
    }
}

fn handle_frame(sh: &Shared, gate: &mut Gate, inflight: &mut Option<InFlight>, f: Frame) {
    let is_reply = inflight.as_ref().is_some_and(|i| i.magic() == f.magic) && f.body.len() >= 4;
    if is_reply {
        let Ok((_txid, payload)) = split_response(&f.body) else { return };
        match inflight.take() {
            Some(InFlight::Read(r, _)) => match parse_value(r.kind(), payload) {
                Ok(v) => apply_read(sh, gate, r, v),
                Err(e) => tracing::warn!("resposta de {} não entendida: {e}", r.name()),
            },
            Some(InFlight::Adjust(job, _)) => {
                // resposta esperada: 22 00 (propriedade vazia)
                let result = if payload == [0x22, 0x00] {
                    let adj = job.out.adjustment();
                    gate.record_sent(adj);
                    match adj {
                        Adjustment::Brightness(v) => sh.state.send_modify(|st| st.brightness = Some(v)),
                        Adjustment::Dimmer(v) => sh.state.send_modify(|st| st.dimmer = Some(v)),
                        _ => {}
                    }
                    Ok(())
                } else {
                    let hex: String = payload.iter().take(32).map(|b| format!("{b:02x}")).collect();
                    Err(AdjustError::Rejected(format!("resposta {hex}")))
                };
                finish(sh, job, result);
            }
            None => {}
        }
        return;
    }
    let ev = events::decode(&f);
    match &ev {
        Event::Sensor { index, value } if *index < 3 => {
            let (i, v) = (*index as usize, *value);
            sh.state.send_modify(|st| st.temperatures[i] = Some(v));
        }
        Event::Action { action, .. } => {
            if let Some(level) = action.brightness_level().and_then(|l| u8::try_from(l).ok()).filter(|l| *l <= 9) {
                gate.observe(Adjustment::Brightness(level));
                sh.state.send_modify(|st| st.brightness = Some(level));
            }
            if let Some(level) = action.dimmer_level().and_then(|l| u8::try_from(l).ok()).filter(|l| *l <= 2) {
                gate.observe(Adjustment::Dimmer(level));
                sh.state.send_modify(|st| st.dimmer = Some(level));
            }
        }
        _ => {}
    }
    let _ = sh.events.send(ClientEvent::Device(ev));
}

fn apply_read(sh: &Shared, gate: &mut Gate, r: Readable, v: Value) {
    match (r, v) {
        (Readable::SwVersion, Value::Text(s)) => {
            gate.set_firmware(Some(s.clone()));
            sh.state.send_modify(|st| {
                st.sw_version = Some(s);
                apply_gate(st, gate);
            });
        }
        (Readable::DspVersion, Value::Text(s)) => sh.state.send_modify(|st| st.dsp_version = Some(s)),
        (Readable::ProximityEnabled, Value::Bool(b)) => sh.state.send_modify(|st| st.proximity_enabled = Some(b)),
        (Readable::EisEnabled, Value::Bool(b)) => sh.state.send_modify(|st| st.eis_enabled = Some(b)),
        (Readable::Config, Value::Text(json)) => match Calibration::from_json_str(&json) {
            Ok(c) => {
                sh.calibration.send_replace(Some(Arc::new(c)));
                sh.state.send_modify(|st| st.calibration_loaded = true);
            }
            // o conteúdo nunca vai para o log: contém o número de série
            Err(e) => tracing::warn!("calibração não entendida: {e}"),
        },
        (r, _) => tracing::warn!("valor inesperado para {}", r.name()),
    }
}
