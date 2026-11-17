//! Compara o Rust com os resultados da lógica Python (gerados por tools/gen_golden.py).

use std::path::PathBuf;

use serde_json::Value as J;
use xr_proto::control::{self, Readable, Value};
use xr_proto::events::{self, Event};
use xr_proto::{frame, imu, vsync};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures")
}

fn read_json(rel: &str) -> J {
    let p = fixtures().join(rel);
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).unwrap()
}

fn bits3(v: [f32; 3]) -> Vec<u64> {
    v.iter().map(|x| u64::from(x.to_bits())).collect()
}

fn check_imu(sample: &imu::ImuSample, g: &J, ctx: &str) {
    assert_eq!(sample.timestamp_ns, g["ts"].as_u64().unwrap(), "{ctx}");
    assert_eq!(u64::from(sample.kind.raw()), g["kind"].as_u64().unwrap(), "{ctx}");
    let arr = |k: &str| -> Vec<u64> { g[k].as_array().unwrap().iter().map(|x| x.as_u64().unwrap()).collect() };
    assert_eq!(bits3(sample.gyro), arr("gyro"), "{ctx}");
    assert_eq!(bits3(sample.accel), arr("accel"), "{ctx}");
    assert_eq!(bits3(sample.mag), arr("mag"), "{ctx}");
    assert_eq!(u64::from(sample.temp_c.to_bits()), g["temp"].as_u64().unwrap(), "{ctx}");
}

#[test]
fn imu_sintetico() {
    let data = std::fs::read(fixtures().join("imu-sintetico.bin")).unwrap();
    let gold = read_json("golden/imu-sintetico.json");
    let gold = gold.as_array().unwrap();
    let frames = frame::decode_all(&data).unwrap();
    assert_eq!(frames.len(), gold.len());
    for (i, (f, g)) in frames.iter().zip(gold).enumerate() {
        check_imu(&imu::parse_frame(f).unwrap(), g, &format!("pacote {i}"));
    }
}

#[test]
fn vsync_sintetico() {
    let data = std::fs::read(fixtures().join("vsync-sintetico.bin")).unwrap();
    let gold = read_json("golden/vsync-sintetico.json");
    let frames = frame::decode_all(&data).unwrap();
    assert_eq!(frames.len(), gold.as_array().unwrap().len());
    for (f, g) in frames.iter().zip(gold.as_array().unwrap()) {
        let v = vsync::parse_frame(f).unwrap();
        assert_eq!(v.timestamp_ns, g["ts"].as_u64().unwrap());
        assert_eq!(v.counter, g["counter"].as_u64().unwrap());
    }
}

#[test]
fn pedidos_de_leitura_iguais_ao_python() {
    let gold = read_json("golden/pedidos-leitura.json");
    for r in Readable::ALL {
        let got: String = control::encode_get(r).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(got, gold[r.name()].as_str().unwrap(), "{}", r.name());
    }
}

#[test]
fn controle_sintetico() {
    let data = std::fs::read(fixtures().join("controle-sintetico.bin")).unwrap();
    let gold = read_json("golden/controle-sintetico.json");
    let frames = frame::decode_all(&data).unwrap();
    let gold = gold.as_array().unwrap();
    assert_eq!(frames.len(), gold.len());
    for (f, g) in frames.iter().zip(gold) {
        if g["tipo"] == "resposta" {
            let r = Readable::from_magic(f.magic).expect("magic de leitura");
            assert_eq!(r.name(), g["nome"].as_str().unwrap());
            let (txid, payload) = control::split_response(&f.body).unwrap();
            assert_eq!(txid, 1);
            let v = control::parse_value(r.kind(), payload).unwrap();
            match (&v, &g["valor"]) {
                (Value::Text(s), J::String(want)) => assert_eq!(s, want),
                (Value::Bool(b), J::Bool(want)) => assert_eq!(b, want),
                other => panic!("valor diferente: {other:?}"),
            }
        } else {
            let e = events::decode(f);
            assert_eq!(e.describe_pt(), g["descricao"].as_str().unwrap());
            if f.magic == events::KEY_MAGIC || f.magic == events::ACTION_MAGIC || f.magic == events::SENSOR_MAGIC {
                assert!(!matches!(e, Event::Unknown { .. }), "{e:?}");
            }
        }
    }
}

/// Usa captures/ (privado) só se a referência local tiver sido gerada.
#[test]
fn imu_captura_local() {
    let gold_path = fixtures().join("local/imu-movimentos.json");
    let cap = fixtures().join("../../captures/imu-movimentos.bin");
    if !gold_path.exists() || !cap.exists() {
        eprintln!("captura local ausente: teste pulado");
        return;
    }
    let gold = read_json("local/imu-movimentos.json");
    let frames = frame::decode_all(&std::fs::read(cap).unwrap()).unwrap();
    assert_eq!(frames.len() as u64, gold["total"].as_u64().unwrap());
    let step = gold["passo"].as_u64().unwrap() as usize;
    let samples = gold["amostras"].as_array().unwrap();
    for (i, g) in samples.iter().enumerate() {
        check_imu(&imu::parse_frame(&frames[i * step]).unwrap(), g, &format!("pacote {}", i * step));
    }
    let mut por_tipo = std::collections::BTreeMap::<String, u64>::new();
    for f in &frames {
        *por_tipo.entry(imu::parse_frame(f).unwrap().kind.raw().to_string()).or_default() += 1;
    }
    for (k, v) in gold["por_tipo"].as_object().unwrap() {
        assert_eq!(por_tipo.get(k).copied(), v.as_u64(), "tipo {k}");
    }
}
