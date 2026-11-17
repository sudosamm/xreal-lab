//! Trava de ajustes. É o ÚNICO caminho para montar bytes que mudam o estado do óculos.
//!
//! Regras de segurança do projeto:
//! - lista fechada de ajustes voláteis ([`Adjustment`]), com faixas validadas;
//! - a trava começa desligada; o usuário liga o "modo ajustes" a cada sessão;
//! - só para firmware testado ([`TESTED_FIRMWARE`]);
//! - duas etapas: [`Gate::prepare`] mostra o que vai acontecer e como desfazer;
//!   [`Gate::confirm`] (vindo do toque do usuário) libera os bytes. A proposta vale
//!   [`PROPOSAL_TTL`] e só pode ser usada uma vez; existe no máximo uma pendente.
//!
//! [`Outgoing`] não tem construtor público: fora deste crate não há como fabricar bytes de ajuste.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::control::encode_set;
use crate::Magic;

pub const PROPOSAL_TTL: Duration = Duration::from_secs(30);

/// Versões de software do óculos em que os ajustes foram conferidos.
pub const TESTED_FIRMWARE: &[&str] = &["1.9.1.20260914175625"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ButtonsMode {
    Enabled = 0,
    Disabled = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputMode {
    Normal = 0,
    SideBySide = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Adjustment {
    /// 27 1c, 0–9
    Brightness(u8),
    /// 27 27, filtro eletrocrômico 0/1/2
    Dimmer(u8),
    /// 28 29, modo de cena
    Buttons(ButtonsMode),
    /// 28 22; a tela pode ficar ~12 s apagada
    InputMode(InputMode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdjustmentKind {
    Brightness,
    Dimmer,
    Buttons,
    InputMode,
}

impl Adjustment {
    pub fn kind(&self) -> AdjustmentKind {
        match self {
            Self::Brightness(_) => AdjustmentKind::Brightness,
            Self::Dimmer(_) => AdjustmentKind::Dimmer,
            Self::Buttons(_) => AdjustmentKind::Buttons,
            Self::InputMode(_) => AdjustmentKind::InputMode,
        }
    }

    pub fn validate(&self) -> Result<(), GateError> {
        match *self {
            Self::Brightness(v) if v > 9 => Err(GateError::OutOfRange(format!("brilho {v} (0–9)"))),
            Self::Dimmer(v) if v > 2 => Err(GateError::OutOfRange(format!("filtro {v} (0, 1 ou 2)"))),
            _ => Ok(()),
        }
    }

    fn magic(&self) -> Magic {
        match self {
            Self::Brightness(_) => [0x27, 0x1c],
            Self::Dimmer(_) => [0x27, 0x27],
            Self::Buttons(_) => [0x28, 0x29],
            Self::InputMode(_) => [0x28, 0x22],
        }
    }

    fn wire_value(&self) -> u64 {
        match *self {
            Self::Brightness(v) | Self::Dimmer(v) => u64::from(v),
            Self::Buttons(m) => m as u64,
            Self::InputMode(m) => m as u64,
        }
    }

    pub fn describe_pt(&self) -> String {
        match self {
            Self::Brightness(v) => format!("brilho {v}"),
            Self::Dimmer(v) => format!("filtro eletrocrômico {v}"),
            Self::Buttons(ButtonsMode::Enabled) => "botões do óculos ativos".into(),
            Self::Buttons(ButtonsMode::Disabled) => "botões do óculos desativados".into(),
            Self::InputMode(InputMode::Normal) => "modo de tela normal".into(),
            Self::InputMode(InputMode::SideBySide) => "modo de tela lado a lado".into(),
        }
    }

    fn warnings_pt(&self) -> Vec<String> {
        match self {
            Self::InputMode(_) => vec!["A tela pode ficar apagada por cerca de 12 s.".into()],
            Self::Buttons(ButtonsMode::Disabled) => vec![
                "Os botões do óculos param de responder; para reativar, use este app.".into(),
                "Deve voltar ao normal ao religar o óculos (ainda não confirmado).".into(),
            ],
            _ => vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Undo {
    /// desfazer enviando este ajuste (também pela trava)
    Send(Adjustment),
    /// desfazer à mão
    Manual(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub id: u64,
    pub adjustment: Adjustment,
    pub description: String,
    /// `None` = valor atual desconhecido (ainda não há leitura para estes ajustes)
    pub current: Option<Adjustment>,
    pub undo: Undo,
    pub warnings: Vec<String>,
    pub expires_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GateError {
    #[error("modo ajustes desligado")]
    Disabled,
    #[error("versão do firmware ainda não lida")]
    FirmwareUnknown,
    #[error("firmware {0} não está na lista de testados; ajustes bloqueados")]
    FirmwareNotTested(String),
    #[error("valor fora da faixa: {0}")]
    OutOfRange(String),
    #[error("nenhuma proposta pendente")]
    NoProposal,
    #[error("a proposta confirmada não é a pendente")]
    WrongProposal,
    #[error("a proposta venceu; prepare de novo")]
    Expired,
}

/// Bytes liberados pela trava para UM envio.
#[derive(Debug, PartialEq, Eq)]
pub struct Outgoing {
    adjustment: Adjustment,
    bytes: Vec<u8>,
}

impl Outgoing {
    pub fn adjustment(&self) -> Adjustment {
        self.adjustment
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// magic que a resposta do óculos deve ter
    pub fn magic(&self) -> Magic {
        self.adjustment.magic()
    }
}

#[derive(Debug, Default)]
pub struct Gate {
    enabled: bool,
    firmware: Option<String>,
    next_id: u64,
    pending: Option<Proposal>,
    last_sent: HashMap<AdjustmentKind, Adjustment>,
}

impl Gate {
    /// Começa desligada e sem firmware conhecido.
    pub fn new() -> Self {
        Self { next_id: 1, ..Self::default() }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.pending = None;
        }
    }

    /// Chamada ao conectar (versão lida com 27 1d) e com `None` ao desconectar.
    /// Trocar de firmware ou de conexão apaga a proposta pendente e o histórico.
    pub fn set_firmware(&mut self, version: Option<String>) {
        if self.firmware != version {
            self.pending = None;
            self.last_sent.clear();
        }
        self.firmware = version;
    }

    pub fn check_ready(&self) -> Result<(), GateError> {
        if !self.enabled {
            return Err(GateError::Disabled);
        }
        match &self.firmware {
            None => Err(GateError::FirmwareUnknown),
            Some(v) if !TESTED_FIRMWARE.contains(&v.as_str()) => Err(GateError::FirmwareNotTested(v.clone())),
            Some(_) => Ok(()),
        }
    }

    pub fn pending(&self) -> Option<&Proposal> {
        self.pending.as_ref()
    }

    /// Último valor conhecido nesta conexão: enviado por este app ou visto num evento.
    pub fn last_sent(&self, kind: AdjustmentKind) -> Option<Adjustment> {
        self.last_sent.get(&kind).copied()
    }

    pub fn prepare(&mut self, adjustment: Adjustment, now: Instant) -> Result<Proposal, GateError> {
        self.check_ready()?;
        adjustment.validate()?;
        let current = self.last_sent(adjustment.kind());
        let undo = match (current, adjustment) {
            (Some(prev), _) => Undo::Send(prev),
            (None, Adjustment::InputMode(_)) => Undo::Send(Adjustment::InputMode(InputMode::Normal)),
            (None, Adjustment::Buttons(_)) => Undo::Send(Adjustment::Buttons(ButtonsMode::Enabled)),
            (None, Adjustment::Brightness(_)) => {
                Undo::Manual("valor anterior desconhecido: ajuste o brilho pelo menu do óculos".into())
            }
            (None, Adjustment::Dimmer(_)) => {
                Undo::Manual("valor anterior desconhecido: ajuste o filtro pelo menu do óculos".into())
            }
        };
        let p = Proposal {
            id: self.next_id,
            adjustment,
            description: format!("Mudar para {}", adjustment.describe_pt()),
            current,
            undo,
            warnings: adjustment.warnings_pt(),
            expires_at: now + PROPOSAL_TTL,
        };
        self.next_id += 1;
        self.pending = Some(p.clone());
        Ok(p)
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// Consome a proposta pendente em qualquer caso (uso único, mesmo se der erro).
    pub fn confirm(&mut self, id: u64, now: Instant) -> Result<Outgoing, GateError> {
        let p = self.pending.take().ok_or(GateError::NoProposal)?;
        if p.id != id {
            return Err(GateError::WrongProposal);
        }
        if now >= p.expires_at {
            return Err(GateError::Expired);
        }
        self.check_ready()?;
        p.adjustment.validate()?;
        let a = p.adjustment;
        Ok(Outgoing { adjustment: a, bytes: encode_set(a.magic(), a.wire_value()) })
    }

    /// O cliente chama depois que o óculos respondeu ao envio.
    pub fn record_sent(&mut self, adjustment: Adjustment) {
        self.last_sent.insert(adjustment.kind(), adjustment);
    }

    /// Valor visto num evento do óculos (ex.: 27 12 com o nível de brilho depois de mexer nos botões).
    /// Passa a ser o "valor atual" e o "desfazer" da próxima proposta do mesmo tipo.
    pub fn observe(&mut self, adjustment: Adjustment) {
        if adjustment.validate().is_ok() {
            self.last_sent.insert(adjustment.kind(), adjustment);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::hex;

    fn ready() -> Gate {
        let mut g = Gate::new();
        g.set_firmware(Some(TESTED_FIRMWARE[0].into()));
        g.set_enabled(true);
        g
    }

    #[test]
    fn comeca_desligada() {
        let mut g = Gate::new();
        g.set_firmware(Some(TESTED_FIRMWARE[0].into()));
        assert_eq!(g.prepare(Adjustment::Brightness(5), Instant::now()), Err(GateError::Disabled));
    }

    #[test]
    fn exige_firmware_testado() {
        let mut g = Gate::new();
        g.set_enabled(true);
        assert_eq!(g.prepare(Adjustment::Brightness(5), Instant::now()), Err(GateError::FirmwareUnknown));
        g.set_firmware(Some("9.9.9".into()));
        assert!(matches!(
            g.prepare(Adjustment::Brightness(5), Instant::now()),
            Err(GateError::FirmwareNotTested(_))
        ));
    }

    #[test]
    fn faixas() {
        let mut g = ready();
        let t = Instant::now();
        assert!(matches!(g.prepare(Adjustment::Brightness(10), t), Err(GateError::OutOfRange(_))));
        assert!(matches!(g.prepare(Adjustment::Dimmer(3), t), Err(GateError::OutOfRange(_))));
        assert!(g.pending().is_none());
    }

    #[test]
    fn confirmacao_libera_bytes_uma_vez() {
        let mut g = ready();
        let t = Instant::now();
        let p = g.prepare(Adjustment::Brightness(5), t).unwrap();
        assert_eq!(p.current, None);
        assert!(matches!(p.undo, Undo::Manual(_)));
        let out = g.confirm(p.id, t + Duration::from_secs(1)).unwrap();
        assert_eq!(hex(out.bytes()), "271c00000008800000011a020805");
        assert_eq!(g.confirm(p.id, t), Err(GateError::NoProposal));
    }

    #[test]
    fn proposta_vence() {
        let mut g = ready();
        let t = Instant::now();
        let p = g.prepare(Adjustment::Dimmer(1), t).unwrap();
        assert_eq!(g.confirm(p.id, t + PROPOSAL_TTL), Err(GateError::Expired));
        assert_eq!(g.confirm(p.id, t), Err(GateError::NoProposal));
    }

    #[test]
    fn id_errado_consome() {
        let mut g = ready();
        let t = Instant::now();
        let p = g.prepare(Adjustment::Dimmer(1), t).unwrap();
        assert_eq!(g.confirm(p.id + 1, t), Err(GateError::WrongProposal));
        assert_eq!(g.confirm(p.id, t), Err(GateError::NoProposal));
    }

    #[test]
    fn nova_proposta_substitui_a_anterior() {
        let mut g = ready();
        let t = Instant::now();
        let a = g.prepare(Adjustment::Brightness(1), t).unwrap();
        let b = g.prepare(Adjustment::Brightness(2), t).unwrap();
        assert_eq!(g.confirm(a.id, t), Err(GateError::WrongProposal));
        let _ = b;
    }

    #[test]
    fn desligar_ou_trocar_firmware_apaga_pendente() {
        let mut g = ready();
        let t = Instant::now();
        let p = g.prepare(Adjustment::Brightness(1), t).unwrap();
        g.set_enabled(false);
        g.set_enabled(true);
        assert_eq!(g.confirm(p.id, t), Err(GateError::NoProposal));
        let p = g.prepare(Adjustment::Brightness(1), t).unwrap();
        g.set_firmware(None);
        assert_eq!(g.confirm(p.id, t), Err(GateError::NoProposal));
    }

    #[test]
    fn desfazer_usa_o_ultimo_enviado() {
        let mut g = ready();
        let t = Instant::now();
        let p = g.prepare(Adjustment::InputMode(InputMode::SideBySide), t).unwrap();
        assert_eq!(p.undo, Undo::Send(Adjustment::InputMode(InputMode::Normal)));
        assert_eq!(p.warnings.len(), 1);
        let out = g.confirm(p.id, t).unwrap();
        assert_eq!(hex(out.bytes()), "282200000008800000011a020801");
        g.record_sent(out.adjustment());
        g.record_sent(Adjustment::Brightness(3));
        let p = g.prepare(Adjustment::Brightness(7), t).unwrap();
        assert_eq!(p.current, Some(Adjustment::Brightness(3)));
        assert_eq!(p.undo, Undo::Send(Adjustment::Brightness(3)));
    }

    #[test]
    fn valor_observado_vira_atual() {
        let mut g = ready();
        g.observe(Adjustment::Brightness(12)); // fora da faixa: ignorado
        g.observe(Adjustment::Brightness(9));
        let p = g.prepare(Adjustment::Brightness(4), Instant::now()).unwrap();
        assert_eq!(p.current, Some(Adjustment::Brightness(9)));
        assert_eq!(p.undo, Undo::Send(Adjustment::Brightness(9)));
    }

    #[test]
    fn bytes_dos_quatro_ajustes() {
        let t = Instant::now();
        let cases = [
            (Adjustment::Brightness(0), "271c00000008800000011a020800"),
            (Adjustment::Dimmer(2), "272700000008800000011a020802"),
            (Adjustment::Buttons(ButtonsMode::Disabled), "282900000008800000011a020801"),
            (Adjustment::InputMode(InputMode::Normal), "282200000008800000011a020800"),
        ];
        for (adj, want) in cases {
            let mut g = ready();
            let p = g.prepare(adj, t).unwrap();
            assert_eq!(hex(g.confirm(p.id, t).unwrap().bytes()), want, "{adj:?}");
        }
    }
}
