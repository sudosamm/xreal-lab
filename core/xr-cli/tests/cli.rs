//! Roda o binário `xr` contra o óculos simulado embutido (`--sim`).

use std::process::Command;

fn xr(args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_xr")).args(args).output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr))
}

#[test]
fn info_no_simulado() {
    let (ok, out) = xr(&["--sim", "info"]);
    assert!(ok, "{out}");
    assert!(out.contains(xr_proto::gate::TESTED_FIRMWARE[0]), "{out}");
    assert!(out.contains("15.SIMULADO") && out.contains("carregada"), "{out}");
}

#[test]
fn imu_no_simulado() {
    let (ok, out) = xr(&["--sim", "imu", "2"]);
    assert!(ok, "{out}");
    let last = out.lines().rev().find(|l| l.starts_with("arfagem")).expect("linhas de orientação");
    assert!(last.contains("parado"), "{last}");
}

#[test]
fn listen_no_simulado() {
    let (ok, out) = xr(&["--sim", "listen", "1.5"]);
    assert!(ok, "{out}");
    assert!(out.contains("provável temperatura"), "{out}");
}

#[test]
fn record_no_simulado() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("rec-{}", std::process::id()));
    let (ok, out) = xr(&["--sim", "record", dir.to_str().unwrap(), "1"]);
    assert!(ok, "{out}");
    let imu = std::fs::read(dir.join("imu.bin")).unwrap();
    let frames = xr_proto::frame::decode_all(&imu).unwrap();
    assert!(frames.len() > 500, "{}", frames.len());
    assert!(std::fs::metadata(dir.join("controle.bin")).unwrap().len() > 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sem_comando_mostra_uso() {
    let (ok, out) = xr(&[]);
    assert!(!ok && out.contains("nada é ajustado"));
}
