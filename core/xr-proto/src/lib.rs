//! xr-proto — formato das mensagens do XREAL One Pro.
//!
//! Sem rede e sem async: só bytes entrando e valores saindo. Código próprio do xreal-lab,
//! portado de `tools/imu_peek.py` e `tools/xr_control_peek.py`.
//!
//! As três portas usam o mesmo enquadramento (`magic (2) | tamanho u32 BE | corpo`):
//! - 52998: IMU ([`imu`])
//! - 52996: sinal de quadro ([`vsync`])
//! - 52999: controle ([`control`], [`events`]); ajustes só pela trava ([`gate`]).
#![forbid(unsafe_code)]

pub mod control;
pub mod error;
pub mod events;
pub mod frame;
pub mod gate;
pub mod imu;
pub mod pb;
pub mod vsync;

pub use error::ProtoError;
pub use frame::{Frame, FrameDecoder, Magic};

pub(crate) mod bytes {
    use crate::ProtoError;

    pub fn need(buf: &[u8], len: usize) -> Result<(), ProtoError> {
        if buf.len() < len {
            Err(ProtoError::Short { expected: len, got: buf.len() })
        } else {
            Ok(())
        }
    }

    pub fn u32_le(b: &[u8], o: usize) -> u32 {
        u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    }

    pub fn u32_be(b: &[u8], o: usize) -> u32 {
        u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    }

    pub fn u64_le(b: &[u8], o: usize) -> u64 {
        let mut a = [0u8; 8];
        a.copy_from_slice(&b[o..o + 8]);
        u64::from_le_bytes(a)
    }

    pub fn f32_le(b: &[u8], o: usize) -> f32 {
        f32::from_bits(u32_le(b, o))
    }

    pub fn vec3_le(b: &[u8], o: usize) -> [f32; 3] {
        [f32_le(b, o), f32_le(b, o + 4), f32_le(b, o + 8)]
    }

    pub fn hex(b: &[u8]) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(b.len() * 2);
        for x in b {
            let _ = write!(s, "{x:02x}");
        }
        s
    }
}
