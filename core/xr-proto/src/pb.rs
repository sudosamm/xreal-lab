//! O mínimo de protobuf que o canal de controle usa: varints.

use crate::ProtoError;

pub fn read_varint(buf: &[u8], pos: &mut usize) -> Result<u64, ProtoError> {
    let mut v: u64 = 0;
    let mut shift = 0u32;
    while shift < 64 {
        let b = *buf.get(*pos).ok_or(ProtoError::BadVarint)?;
        *pos += 1;
        v |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Ok(v);
        }
        shift += 7;
    }
    Err(ProtoError::BadVarint)
}

pub fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ida_e_volta() {
        for v in [0u64, 1, 127, 128, 300, 16_383, 16_384, u32::MAX as u64, u64::MAX] {
            let mut out = Vec::new();
            write_varint(&mut out, v);
            let mut pos = 0;
            assert_eq!(read_varint(&out, &mut pos).unwrap(), v);
            assert_eq!(pos, out.len());
        }
    }

    #[test]
    fn bytes_conhecidos() {
        let mut out = Vec::new();
        write_varint(&mut out, 300);
        assert_eq!(out, [0xac, 0x02]);
    }

    #[test]
    fn truncado() {
        let mut pos = 0;
        assert_eq!(read_varint(&[0x80], &mut pos), Err(ProtoError::BadVarint));
    }
}
