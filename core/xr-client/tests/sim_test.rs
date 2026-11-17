//! Cliente contra o óculos simulado (xr-sim). Nada aqui usa o aparelho de verdade.

use std::time::Duration;

use tokio::sync::watch;
use xr_client::{AdjustError, ClientConfig, ClientEvent, DeviceEvent, DeviceState, Endpoint, Glasses, Link};
use xr_proto::events::{Action, Key, KeyState};
use xr_proto::gate::{Adjustment, GateError, InputMode, Undo};
use xr_sim::{Received, Sim, SimConfig};

fn config(sim: &Sim, audit: Option<std::path::PathBuf>) -> ClientConfig {
    ClientConfig {
        endpoints: vec![
            // primeiro um endereço que recusa, para testar a troca de endereço
            Endpoint { host: "127.0.0.1".into(), imu_port: 1, vsync_port: 1, control_port: 1 },
            Endpoint {
                host: "127.0.0.1".into(),
                imu_port: sim.imu_addr.port(),
                vsync_port: sim.vsync_addr.port(),
                control_port: sim.control_addr.port(),
            },
        ],
        retry_delay: Duration::from_millis(100),
        audit_path: audit,
        ..ClientConfig::default()
    }
}

async fn wait_state(rx: &mut watch::Receiver<DeviceState>, what: &str, f: impl Fn(&DeviceState) -> bool) -> DeviceState {
    let r = tokio::time::timeout(Duration::from_secs(5), async { rx.wait_for(|s| f(s)).await.map(|s| s.clone()) }).await;
    match r {
        Ok(Ok(s)) => s,
        _ => panic!("esperando {what}; estado: {:?}", *rx.borrow()),
    }
}

fn ready(s: &DeviceState) -> bool {
    s.control.is_connected() && s.sw_version.is_some() && s.calibration_loaded && s.imu.is_connected()
}

#[tokio::test(flavor = "multi_thread")]
async fn conecta_le_e_calcula_orientacao() {
    let sim = Sim::start(SimConfig::default()).await.unwrap();
    let g = Glasses::start(config(&sim, None));
    let mut st = g.state();
    let s = wait_state(&mut st, "conexão", ready).await;
    assert_eq!(s.sw_version.as_deref(), Some(xr_proto::gate::TESTED_FIRMWARE[0]));
    assert_eq!(s.dsp_version.as_deref(), Some("15.SIMULADO"));
    assert_eq!(s.proximity_enabled, Some(true));
    assert_eq!(s.eis_enabled, Some(false));
    assert_eq!(s.control, Link::Connected { host: "127.0.0.1".into() });
    assert!(!s.adjustments_enabled);

    // taxas e temperaturas
    let s = wait_state(&mut st, "taxas", |s| s.imu_hz > 500.0 && s.vsync_hz > 60.0 && s.temperatures[2].is_some()).await;
    assert!(s.imu_hz < 1500.0 && s.vsync_hz < 200.0, "{s:?}");

    // simulado nivelado com o IMU girado 35°: com a calibração, a tela fica nivelada
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let o = g.orientation().borrow().expect("orientação");
    assert!(o.pitch_deg.abs() < 1.0 && o.roll_deg.abs() < 1.0, "{o:?}");

    // só leituras chegaram ao simulado; o id do óculos não foi pedido
    assert!(sim.writes().is_empty(), "{:?}", sim.writes());
    assert!(!sim.received().contains(&Received::Get([0x27, 0x29])));
}

#[tokio::test(flavor = "multi_thread")]
async fn eventos_e_brilho_visto_nos_botoes() {
    let sim = Sim::start(SimConfig::default()).await.unwrap();
    let g = Glasses::start(config(&sim, None));
    let mut ev = g.events();
    let mut st = g.state();
    wait_state(&mut st, "conexão", ready).await;

    sim.push_key(3, 1);
    sim.push_action(7, 3);
    let mut got_key = false;
    let mut got_action = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while !(got_key && got_action) {
        let e = tokio::time::timeout_at(deadline, ev.recv()).await.expect("eventos").unwrap();
        match e {
            ClientEvent::Device(DeviceEvent::Key { key: Key::VolumeDown, state: KeyState::Pressed, .. }) => got_key = true,
            ClientEvent::Device(DeviceEvent::Action { action: Action::BrightnessDown(3), .. }) => got_action = true,
            _ => {}
        }
    }
    let s = wait_state(&mut st, "brilho", |s| s.brightness == Some(3)).await;
    assert_eq!(s.brightness, Some(3));
    sim.push_action(12, 2);
    wait_state(&mut st, "filtro", |s| s.dimmer == Some(2)).await;

    // o brilho visto vira o "valor atual" da proposta
    g.set_adjustments_enabled(true).await;
    let p = g.prepare(Adjustment::Brightness(6)).await.unwrap();
    assert_eq!(p.current, Some(Adjustment::Brightness(3)));
    assert_eq!(p.undo, Undo::Send(Adjustment::Brightness(3)));
    let p = g.prepare(Adjustment::Dimmer(0)).await.unwrap();
    assert_eq!(p.undo, Undo::Send(Adjustment::Dimmer(2)));
    g.cancel().await;
    assert!(sim.writes().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn nada_e_enviado_sem_confirmacao() {
    let sim = Sim::start(SimConfig::default()).await.unwrap();
    let audit = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("ajustes-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&audit);
    let g = Glasses::start(config(&sim, Some(audit.clone())));
    let mut st = g.state();
    wait_state(&mut st, "conexão", ready).await;

    // modo ajustes desligado
    assert_eq!(g.prepare(Adjustment::Brightness(5)).await, Err(AdjustError::Gate(GateError::Disabled)));
    // confirmar sem proposta
    assert_eq!(g.confirm(1).await, Err(AdjustError::Gate(GateError::NoProposal)));

    g.set_adjustments_enabled(true).await;
    wait_state(&mut st, "modo ajustes", |s| s.adjustments_enabled && s.adjustments_blocked.is_none()).await;
    assert_eq!(g.prepare(Adjustment::Dimmer(7)).await.map(|_| ()), Err(AdjustError::Gate(GateError::OutOfRange("filtro 7 (0, 1 ou 2)".into()))));

    let p = g.prepare(Adjustment::InputMode(InputMode::SideBySide)).await.unwrap();
    assert!(!p.warnings.is_empty());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(sim.writes().is_empty(), "preparar não pode enviar nada");

    // confirmar um id errado consome a proposta sem enviar
    assert_eq!(g.confirm(p.id + 100).await, Err(AdjustError::Gate(GateError::WrongProposal)));
    assert_eq!(g.confirm(p.id).await, Err(AdjustError::Gate(GateError::NoProposal)));
    assert!(sim.writes().is_empty());

    let p = g.prepare(Adjustment::Brightness(5)).await.unwrap();
    assert_eq!(g.confirm(p.id).await, Ok(Adjustment::Brightness(5)));
    assert_eq!(
        sim.writes(),
        vec![Received::Set {
            magic: [0x27, 0x1c],
            value: 5,
            raw: vec![0x27, 0x1c, 0, 0, 0, 8, 0x80, 0, 0, 1, 0x1a, 0x02, 0x08, 0x05]
        }]
    );
    wait_state(&mut st, "brilho enviado", |s| s.brightness == Some(5)).await;

    let log = std::fs::read_to_string(&audit).unwrap();
    let lines: Vec<serde_json::Value> = log.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["ajuste"], "brilho 5");
    assert_eq!(lines[0]["resultado"], "ok");
    assert_eq!(lines[0]["bytes"], "271c00000008800000011a020805");
    let _ = std::fs::remove_file(&audit);
}

#[tokio::test(flavor = "multi_thread")]
async fn firmware_nao_testado_bloqueia() {
    let sim = Sim::start(SimConfig { sw_version: "2.0.0.OUTRO".into(), ..SimConfig::default() }).await.unwrap();
    let g = Glasses::start(config(&sim, None));
    let mut st = g.state();
    wait_state(&mut st, "conexão", ready).await;
    g.set_adjustments_enabled(true).await;
    let s = wait_state(&mut st, "bloqueio", |s| s.adjustments_enabled && s.adjustments_blocked.is_some()).await;
    assert!(s.adjustments_blocked.unwrap().contains("2.0.0.OUTRO"));
    assert!(matches!(
        g.prepare(Adjustment::Brightness(5)).await,
        Err(AdjustError::Gate(GateError::FirmwareNotTested(_)))
    ));
    assert!(sim.writes().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn queda_desliga_o_modo_ajustes() {
    let sim = Sim::start(SimConfig::default()).await.unwrap();
    let g = Glasses::start(config(&sim, None));
    let mut st = g.state();
    wait_state(&mut st, "conexão", ready).await;
    g.set_adjustments_enabled(true).await;
    let p = g.prepare(Adjustment::Brightness(2)).await.unwrap();
    drop(sim);
    let s = wait_state(&mut st, "queda", |s| !s.control.is_connected()).await;
    assert!(!s.adjustments_enabled);
    assert_eq!(s.sw_version, None);
    // a proposta morreu junto
    let r = g.confirm(p.id).await;
    assert!(matches!(r, Err(AdjustError::Gate(_))), "{r:?}");
}
