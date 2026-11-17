//! Compara com a lógica do imu_peek.py (tools/gen_golden.py) e roda o filtro na captura local.

use std::path::PathBuf;

use xr_fusion::{tilt_deg, Calibration, DVec3, FilterConfig, Fusion};
use xr_proto::{frame, imu};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures")
}

#[test]
fn rotacao_igual_ao_python() {
    let cfg = std::fs::read_to_string(fixtures().join("config-falsa.json")).unwrap();
    let cal = Calibration::from_json_str(&cfg).unwrap();
    let rot = xr_fusion::DisplayRotation::from_xyzw(cal.display_q_left);
    let gold: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixtures().join("golden/rotacao.json")).unwrap()).unwrap();
    for c in gold.as_array().unwrap() {
        let v = |k: &str| {
            let a = c[k].as_array().unwrap();
            DVec3::new(a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), a[2].as_f64().unwrap())
        };
        let out = rot.to_display(v("in"));
        assert!((out - v("out")).length() < 1e-9, "{out:?} != {:?}", v("out"));
        let (p, r) = tilt_deg(out);
        assert!((p - c["arfagem"].as_f64().unwrap()).abs() < 1e-7);
        assert!((r - c["rolagem"].as_f64().unwrap()).abs() < 1e-7);
    }
}

/// Usa notes/config-onepro.json e captures/imu-movimentos.bin (privados) se existirem.
#[test]
fn filtro_na_captura_local() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (cfg, cap) = (root.join("notes/config-onepro.json"), root.join("captures/imu-movimentos.bin"));
    if !cfg.exists() || !cap.exists() {
        eprintln!("calibração ou captura local ausente: teste pulado");
        return;
    }
    let cal = Calibration::from_json_str(&std::fs::read_to_string(cfg).unwrap()).unwrap();
    assert!(cal.gyro_bias.is_some() && !cal.gyro_bias_temp.is_empty());
    let mut fu = Fusion::new(Some(cal), FilterConfig::default());
    let frames = frame::decode_all(&std::fs::read(cap).unwrap()).unwrap();
    let (mut n, mut diffs) = (0usize, Vec::new());
    for f in &frames {
        let s = imu::parse_frame(f).unwrap();
        if let Some(o) = fu.update(&s) {
            assert!(o.q.is_finite() && o.yaw_deg.is_finite());
            n += 1;
            if o.still {
                let a = fu.rotation().to_display(DVec3::new(f64::from(s.accel[0]), f64::from(s.accel[1]), f64::from(s.accel[2])));
                let (p, r) = tilt_deg(a);
                diffs.push((o.pitch_deg - p).abs().max((o.roll_deg - r).abs()));
            }
        }
    }
    assert!(n > 50_000, "saídas: {n}");
    diffs.sort_by(f64::total_cmp);
    if !diffs.is_empty() {
        let med = diffs[diffs.len() / 2];
        eprintln!("amostras paradas: {}, diferença mediana filtro × acelerômetro: {med:.2}°", diffs.len());
        assert!(med < 2.0);
    }
}

/// Óculos parado sobre a mesa (de cabeça para baixo), 16/11/2026: o bias aprendido deve bater
/// com a média do giroscópio, e a guinada não pode derivar depois de aprender.
#[test]
fn parado_na_mesa_local() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (cfg, cap) = (root.join("notes/config-onepro.json"), root.join("captures/imu-parado.bin"));
    if !cfg.exists() || !cap.exists() {
        eprintln!("captura parada ausente: teste pulado");
        return;
    }
    let cal = Calibration::from_json_str(&std::fs::read_to_string(cfg).unwrap()).unwrap();
    let samples: Vec<imu::ImuSample> = frame::decode_all(&std::fs::read(cap).unwrap())
        .unwrap()
        .iter()
        .map(|f| imu::parse_frame(f).unwrap())
        .filter(|s| s.kind == imu::ImuKind::GyroAccel)
        .collect();
    let n = samples.len() as f64;
    let mean = samples.iter().fold(DVec3::ZERO, |acc, s| {
        acc + DVec3::new(f64::from(s.gyro[0]), f64::from(s.gyro[1]), f64::from(s.gyro[2]))
    }) / n;

    let mut fu = Fusion::new(Some(cal), FilterConfig::default());
    let (mut yaw_after_10s, mut last, mut still, mut total) = (None, None, 0usize, 0usize);
    let t0 = samples[0].timestamp_ns;
    for s in &samples {
        if let Some(o) = fu.update(s) {
            // a captura tem duas batidas leves (aos 38 s e aos 136 s)
            total += 1;
            still += usize::from(o.still);
            if yaw_after_10s.is_none() && s.timestamp_ns - t0 > 10_000_000_000 {
                yaw_after_10s = Some(o.yaw_deg);
            }
            last = Some(o);
        }
    }
    assert!(still as f64 > 0.95 * total as f64, "parado só em {still} de {total}");
    let (bias, learned) = fu.gyro_bias(35.0);
    assert!(learned);
    eprintln!("média {mean:?}, aprendido {bias:?}");
    assert!((bias - mean).length() < 2e-4, "bias aprendido {bias:?} longe da média {mean:?}");
    let last = last.unwrap();
    let drift = last.yaw_deg - yaw_after_10s.unwrap();
    let secs = (last.timestamp_ns - t0) as f64 * 1e-9 - 10.0;
    eprintln!("deriva da guinada: {drift:.3}° em {secs:.0} s; arfagem {:.1}°, rolagem {:.1}°", last.pitch_deg, last.roll_deg);
    assert!(drift.abs() < 1.0);
}
