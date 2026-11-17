//! IMU, porta 52998: magic `28 36`, corpo de 128 bytes, ~1000 mensagens/s (+ magnetômetro ~400/s).
//!
//! Posições no corpo (as do imu_peek.py menos os 6 bytes de cabeçalho):
//! `@8 u64` tempo em ns · `@24 u32` tipo (11 giroscópio+acelerômetro, 4 magnetômetro) ·
//! `@28 3×f32` giroscópio (rad/s) · `@40 3×f32` acelerômetro (m/s²) ·
//! `@52 3×f32` magnetômetro (provavelmente µT; -3200 = sem dado) · `@64 f32` temperatura (°C, no tipo 11)

use crate::bytes::{need, u32_le, u64_le, f32_le, vec3_le};
use crate::{Frame, Magic, ProtoError};

pub const MAGIC: Magic = [0x28, 0x36];
pub const BODY_LEN: usize = 128;
/// Valor do magnetômetro quando a mensagem não traz leitura dele.
pub const MAG_NO_DATA: f32 = -3200.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImuKind {
    GyroAccel,
    Mag,
    Other(u32),
}

impl ImuKind {
    pub fn from_raw(v: u32) -> Self {
        match v {
            11 => Self::GyroAccel,
            4 => Self::Mag,
            o => Self::Other(o),
        }
    }

    pub fn raw(self) -> u32 {
        match self {
            Self::GyroAccel => 11,
            Self::Mag => 4,
            Self::Other(o) => o,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImuSample {
    pub timestamp_ns: u64,
    pub kind: ImuKind,
    /// rad/s, eixos do IMU (antes da rotação da calibração)
    pub gyro: [f32; 3],
    /// m/s², eixos do IMU
    pub accel: [f32; 3],
    pub mag: [f32; 3],
    pub temp_c: f32,
}

pub fn parse(body: &[u8]) -> Result<ImuSample, ProtoError> {
    need(body, BODY_LEN)?;
    Ok(ImuSample {
        timestamp_ns: u64_le(body, 8),
        kind: ImuKind::from_raw(u32_le(body, 24)),
        gyro: vec3_le(body, 28),
        accel: vec3_le(body, 40),
        mag: vec3_le(body, 52),
        temp_c: f32_le(body, 64),
    })
}

pub fn parse_frame(f: &Frame) -> Result<ImuSample, ProtoError> {
    if f.magic != MAGIC {
        return Err(ProtoError::UnexpectedMagic(f.magic));
    }
    parse(&f.body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpo_curto() {
        assert!(matches!(parse(&[0; 64]), Err(ProtoError::Short { .. })));
    }

    #[test]
    fn campos_nas_posicoes() {
        let mut b = vec![0u8; BODY_LEN];
        b[8..16].copy_from_slice(&123_456_789u64.to_le_bytes());
        b[24..28].copy_from_slice(&11u32.to_le_bytes());
        b[28..32].copy_from_slice(&1.5f32.to_le_bytes());
        b[44..48].copy_from_slice(&(-9.8f32).to_le_bytes());
        b[60..64].copy_from_slice(&MAG_NO_DATA.to_le_bytes());
        b[64..68].copy_from_slice(&42.0f32.to_le_bytes());
        let s = parse(&b).unwrap();
        assert_eq!(s.timestamp_ns, 123_456_789);
        assert_eq!(s.kind, ImuKind::GyroAccel);
        assert_eq!(s.gyro, [1.5, 0.0, 0.0]);
        assert_eq!(s.accel, [0.0, -9.8, 0.0]);
        assert_eq!(s.mag[2], MAG_NO_DATA);
        assert_eq!(s.temp_c, 42.0);
    }
}
