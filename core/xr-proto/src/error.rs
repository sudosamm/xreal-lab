use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProtoError {
    #[error("mensagem curta: esperava pelo menos {expected} bytes, veio {got}")]
    Short { expected: usize, got: usize },
    #[error("tamanho de corpo absurdo ({0} bytes): fluxo fora de sincronia")]
    BodyTooLarge(usize),
    #[error("magic inesperado: {0:02x?}")]
    UnexpectedMagic([u8; 2]),
    #[error("varint inválido")]
    BadVarint,
    #[error("resposta inesperada: {0}")]
    BadResponse(String),
}
