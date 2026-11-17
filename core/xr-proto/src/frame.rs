//! Enquadramento comum às portas 52996, 52998 e 52999: `magic (2) | tamanho u32 BE | corpo`.
//! O tamanho cobre tudo depois do cabeçalho.

use crate::ProtoError;

pub type Magic = [u8; 2];

pub const HEADER_LEN: usize = 6;
/// A configuração de fábrica (27 1f) tem ~200 KB; acima disso o fluxo perdeu a sincronia.
pub const MAX_BODY_LEN: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub magic: Magic,
    pub body: Vec<u8>,
}

pub fn encode(magic: Magic, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + body.len());
    out.extend_from_slice(&magic);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
    out
}

/// Junta pedaços vindos do socket e devolve mensagens inteiras.
/// Depois de [`ProtoError::BodyTooLarge`] o chamador deve descartar a conexão (ou chamar [`reset`](Self::reset)).
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
    start: usize,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, data: &[u8]) {
        if self.start > 0 && self.start * 2 >= self.buf.len() {
            self.buf.drain(..self.start);
            self.start = 0;
        }
        self.buf.extend_from_slice(data);
    }

    pub fn buffered(&self) -> usize {
        self.buf.len() - self.start
    }

    pub fn reset(&mut self) {
        self.buf.clear();
        self.start = 0;
    }

    pub fn next_frame(&mut self) -> Result<Option<Frame>, ProtoError> {
        let avail = &self.buf[self.start..];
        if avail.len() < HEADER_LEN {
            return Ok(None);
        }
        let magic = [avail[0], avail[1]];
        let len = crate::bytes::u32_be(avail, 2) as usize;
        if len > MAX_BODY_LEN {
            return Err(ProtoError::BodyTooLarge(len));
        }
        if avail.len() < HEADER_LEN + len {
            return Ok(None);
        }
        let body = avail[HEADER_LEN..HEADER_LEN + len].to_vec();
        self.start += HEADER_LEN + len;
        Ok(Some(Frame { magic, body }))
    }
}

/// Decodifica um bloco inteiro (captura gravada). Bytes que sobram no fim são ignorados.
pub fn decode_all(data: &[u8]) -> Result<Vec<Frame>, ProtoError> {
    let mut d = FrameDecoder::new();
    d.push(data);
    let mut out = Vec::new();
    while let Some(f) = d.next_frame()? {
        out.push(f);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ida_e_volta() {
        let raw = encode([0x27, 0x1d], b"abc");
        assert_eq!(raw, [0x27, 0x1d, 0, 0, 0, 3, b'a', b'b', b'c']);
        let f = decode_all(&raw).unwrap();
        assert_eq!(f, vec![Frame { magic: [0x27, 0x1d], body: b"abc".to_vec() }]);
    }

    #[test]
    fn pedacos_de_um_byte() {
        let mut raw = encode([1, 2], &[9; 10]);
        raw.extend(encode([3, 4], &[]));
        let mut d = FrameDecoder::new();
        let mut got = Vec::new();
        for b in &raw {
            d.push(std::slice::from_ref(b));
            while let Some(f) = d.next_frame().unwrap() {
                got.push(f);
            }
        }
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].body, vec![9; 10]);
        assert_eq!(got[1].magic, [3, 4]);
        assert_eq!(d.buffered(), 0);
    }

    #[test]
    fn tamanho_absurdo() {
        let mut d = FrameDecoder::new();
        d.push(&[0x27, 0x1d, 0xff, 0xff, 0xff, 0xff]);
        assert!(matches!(d.next_frame(), Err(ProtoError::BodyTooLarge(_))));
    }
}
