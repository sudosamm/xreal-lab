//! Sinal de quadro, porta 52996: magic `27 31`, corpo de 32 bytes, 120 mensagens/s.
//! Para ~12 s ao trocar o modo de tela.
//!
//! Corpo (verificado em captures/52996-acoes.bin, 16/11/2026): 8 bytes zero ·
//! `@8 u64` tempo em ns (mediana 8 337 750 ns entre quadros) · `@16 u64` contador (+1 por mensagem) · 8 bytes zero.

use crate::bytes::{need, u64_le};
use crate::{Frame, Magic, ProtoError};

pub const MAGIC: Magic = [0x27, 0x31];
pub const BODY_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vsync {
    pub timestamp_ns: u64,
    pub counter: u64,
}

pub fn parse(body: &[u8]) -> Result<Vsync, ProtoError> {
    need(body, BODY_LEN)?;
    Ok(Vsync { timestamp_ns: u64_le(body, 8), counter: u64_le(body, 16) })
}

pub fn parse_frame(f: &Frame) -> Result<Vsync, ProtoError> {
    if f.magic != MAGIC {
        return Err(ProtoError::UnexpectedMagic(f.magic));
    }
    parse(&f.body)
}
