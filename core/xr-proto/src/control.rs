//! Canal de controle, porta 52999: leituras e respostas.
//!
//! Pedido: `magic | tamanho | id de transação u32 BE | corpo`. Leitura: corpo `18 00`.
//! Resposta: `id de transação u32 BE | 22 <len> <tag> <valor>`.
//! O id é sempre `1 | 0x80000000`, como nos scripts já testados com o óculos; um pedido por vez.
//!
//! Ajustes NÃO saem daqui: a montagem dos bytes de ajuste é privada do crate e só a
//! trava ([`crate::gate`]) a usa, depois da confirmação do usuário.

use crate::bytes::{need, u32_be};
use crate::frame::encode;
use crate::pb::{read_varint, write_varint};
use crate::{Magic, ProtoError};

pub const TXID_REQUEST: u32 = 1 | 0x8000_0000;
pub const GET_BODY: [u8; 2] = [0x18, 0x00];

/// Tudo o que o app pode LER. Lista fechada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Readable {
    SwVersion,
    DspVersion,
    /// identificador do aparelho: dado pessoal, não registrar nem publicar
    GlassesId,
    /// configuração/calibração de fábrica (~200 KB): contém número de série
    Config,
    ProximityEnabled,
    EisEnabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Text,
    Bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    Bool(bool),
}

impl Readable {
    pub const ALL: [Readable; 6] = [
        Self::SwVersion,
        Self::DspVersion,
        Self::GlassesId,
        Self::Config,
        Self::ProximityEnabled,
        Self::EisEnabled,
    ];

    pub const fn magic(self) -> Magic {
        match self {
            Self::SwVersion => [0x27, 0x1d],
            Self::DspVersion => [0x27, 0x2d],
            Self::GlassesId => [0x27, 0x29],
            Self::Config => [0x27, 0x1f],
            Self::ProximityEnabled => [0x27, 0x18],
            Self::EisEnabled => [0x28, 0xa6],
        }
    }

    pub const fn kind(self) -> ValueKind {
        match self {
            Self::ProximityEnabled | Self::EisEnabled => ValueKind::Bool,
            _ => ValueKind::Text,
        }
    }

    /// Valores que não devem ir para registros, telas de depuração nem capturas publicadas.
    pub const fn is_private(self) -> bool {
        matches!(self, Self::GlassesId | Self::Config)
    }

    /// Mesmos nomes do xr_control_peek.py.
    pub const fn name(self) -> &'static str {
        match self {
            Self::SwVersion => "sw_version",
            Self::DspVersion => "dsp_version",
            Self::GlassesId => "glasses_id",
            Self::Config => "config",
            Self::ProximityEnabled => "proximity_enabled",
            Self::EisEnabled => "eis_enabled",
        }
    }

    pub fn from_magic(m: Magic) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.magic() == m)
    }
}

fn request(magic: Magic, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(4 + payload.len());
    body.extend_from_slice(&TXID_REQUEST.to_be_bytes());
    body.extend_from_slice(payload);
    encode(magic, &body)
}

/// Bytes de um pedido de leitura, iguais aos do xr_control_peek.get().
pub fn encode_get(r: Readable) -> Vec<u8> {
    request(r.magic(), &GET_BODY)
}

/// Bytes de um ajuste: corpo `1a <len> 08 <valor varint>`. Só a trava chama.
pub(crate) fn encode_set(magic: Magic, value: u64) -> Vec<u8> {
    let mut inner = vec![0x08];
    write_varint(&mut inner, value);
    let mut payload = vec![0x1a];
    write_varint(&mut payload, inner.len() as u64);
    payload.extend_from_slice(&inner);
    request(magic, &payload)
}

/// Separa o id de transação do resto do corpo de uma resposta.
pub fn split_response(body: &[u8]) -> Result<(u32, &[u8]), ProtoError> {
    need(body, 4)?;
    Ok((u32_be(body, 0), &body[4..]))
}

/// Interpreta o valor de uma resposta (sem o id de transação). Mesma lógica de parse_value() do Python.
pub fn parse_value(kind: ValueKind, payload: &[u8]) -> Result<Value, ProtoError> {
    if kind == ValueKind::Bool && payload == [0x22, 0x00] {
        return Ok(Value::Bool(false));
    }
    if payload.first() != Some(&0x22) {
        return Err(ProtoError::BadResponse(crate::bytes::hex(&payload[..payload.len().min(32)])));
    }
    let mut i = 1;
    let _len = read_varint(payload, &mut i)?;
    // tag do valor: não conferida, como no script
    if i >= payload.len() {
        return Err(ProtoError::BadResponse("resposta sem valor".into()));
    }
    i += 1;
    match kind {
        ValueKind::Text => {
            let n = read_varint(payload, &mut i)? as usize;
            let end = i.checked_add(n).filter(|&e| e <= payload.len()).ok_or(ProtoError::Short {
                expected: i.saturating_add(n),
                got: payload.len(),
            })?;
            Ok(Value::Text(String::from_utf8_lossy(&payload[i..end]).into_owned()))
        }
        ValueKind::Bool => Ok(Value::Bool(read_varint(payload, &mut i)? != 0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pedido_de_leitura() {
        assert_eq!(
            crate::bytes::hex(&encode_get(Readable::SwVersion)),
            "271d00000006800000011800"
        );
    }

    #[test]
    fn ajuste_com_valor_zero_e_explicito() {
        assert_eq!(crate::bytes::hex(&encode_set([0x27, 0x1c], 0)), "271c00000008800000011a020800");
        assert_eq!(crate::bytes::hex(&encode_set([0x27, 0x1c], 9)), "271c00000008800000011a020809");
    }

    #[test]
    fn valores() {
        assert_eq!(parse_value(ValueKind::Bool, &[0x22, 0x00]).unwrap(), Value::Bool(false));
        assert_eq!(parse_value(ValueKind::Bool, &[0x22, 0x02, 0x08, 0x01]).unwrap(), Value::Bool(true));
        assert_eq!(
            parse_value(ValueKind::Text, &[0x22, 0x04, 0x0a, 0x02, b'o', b'k']).unwrap(),
            Value::Text("ok".into())
        );
        assert!(parse_value(ValueKind::Text, &[0x22, 0x04, 0x0a, 0x09, b'o']).is_err());
        assert!(parse_value(ValueKind::Text, &[0x10]).is_err());
        assert!(parse_value(ValueKind::Bool, &[0x22, 0x02]).is_err());
    }
}
