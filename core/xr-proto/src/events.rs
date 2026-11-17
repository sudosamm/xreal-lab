//! Eventos não pedidos da porta 52999 (teclas, gestos, temperaturas). Só escuta.
//! Interpretação de 16/11/2026: teclas, nível de brilho e nível do filtro confirmados; temperaturas prováveis.

use crate::bytes::{hex, u32_le, u64_le, f32_le};
use crate::pb::read_varint;
use crate::{Frame, Magic};

pub const KEY_MAGIC: Magic = [0x27, 0x2e];
pub const ACTION_MAGIC: Magic = [0x27, 0x12];
pub const SENSOR_MAGIC: Magic = [0x27, 0x8a];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// botão inferior
    Menu,
    /// rocker frontal
    VolumeUp,
    /// rocker traseiro
    VolumeDown,
    /// botão superior
    Shortcut,
    Other(u32),
}

impl Key {
    pub fn from_raw(v: u32) -> Self {
        match v {
            1 => Self::Menu,
            2 => Self::VolumeUp,
            3 => Self::VolumeDown,
            4 => Self::Shortcut,
            o => Self::Other(o),
        }
    }

    pub fn label_pt(self) -> String {
        match self {
            Self::Menu => "botão inferior (menu)".into(),
            Self::VolumeUp => "rocker frontal (volume +)".into(),
            Self::VolumeDown => "rocker traseiro (volume −)".into(),
            Self::Shortcut => "botão superior (atalho)".into(),
            Self::Other(o) => o.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Pressed,
    Released,
    Other(u32),
}

impl KeyState {
    pub fn from_raw(v: u32) -> Self {
        match v {
            1 => Self::Pressed,
            2 => Self::Released,
            o => Self::Other(o),
        }
    }

    pub fn label_pt(self) -> String {
        match self {
            Self::Pressed => "apertado".into(),
            Self::Released => "solto".into(),
            Self::Other(o) => o.to_string(),
        }
    }
}

/// Ação reportada pelo 27 12 (escuta de 16/11/2026, ver notes/protocolo-controle-52999.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// a=1, b=tecla. O primeiro toque no rocker só abre o menu rápido.
    Click(Key),
    /// a=2, b=tecla
    DoubleClick(Key),
    /// a=6, b=novo nível de brilho (0–9). Certeza alta para o nível.
    BrightnessUp(u32),
    /// a=7, b=novo nível de brilho (0–9)
    BrightnessDown(u32),
    /// a=12, b=novo nível do filtro eletrocrômico (0/1/2). O óculos manda o nível mesmo
    /// quando o auto-escurecimento impede a mudança (visto em 16/11).
    Dimmer(u32),
    Other { code: u32, param: u32 },
}

impl Action {
    pub fn from_raw(code: u32, param: u32) -> Self {
        match code {
            1 => Self::Click(Key::from_raw(param)),
            2 => Self::DoubleClick(Key::from_raw(param)),
            6 => Self::BrightnessUp(param),
            7 => Self::BrightnessDown(param),
            12 => Self::Dimmer(param),
            code => Self::Other { code, param },
        }
    }

    /// Nível de brilho que o óculos passou a usar, quando a ação o informa.
    pub fn brightness_level(&self) -> Option<u32> {
        match *self {
            Self::BrightnessUp(v) | Self::BrightnessDown(v) => Some(v),
            _ => None,
        }
    }

    /// Nível do filtro informado pela ação.
    pub fn dimmer_level(&self) -> Option<u32> {
        match *self {
            Self::Dimmer(v) => Some(v),
            _ => None,
        }
    }

    pub fn label_pt(&self) -> String {
        match self {
            Self::Click(k) => format!("clique em {}", k.label_pt()),
            Self::DoubleClick(k) => format!("duplo clique em {}", k.label_pt()),
            Self::BrightnessUp(v) => format!("brilho + → nível {v}"),
            Self::BrightnessDown(v) => format!("brilho − → nível {v}"),
            Self::Dimmer(v) => format!("filtro → nível {v}"),
            Self::Other { code, param } => format!("código {code}, parâmetro {param}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// 27 2e: tipo u32 LE, estado u32 LE, tempo u32 LE (unidade desconhecida)
    Key { key: Key, state: KeyState, device_time: u32 },
    /// 27 12: código u32, parâmetro u32, u32 ignorado, tempo u64 ns
    Action { action: Action, timestamp_ns: u64 },
    /// 27 8a (provável): três sensores de temperatura a cada ~10 s
    Sensor { index: u64, value: f32 },
    Unknown { magic: Magic, raw: Vec<u8> },
}

/// Nunca falha: o que não reconhece vira [`Event::Unknown`].
pub fn decode(f: &Frame) -> Event {
    let b = &f.body;
    match f.magic {
        KEY_MAGIC if b.len() >= 12 => Event::Key {
            key: Key::from_raw(u32_le(b, 0)),
            state: KeyState::from_raw(u32_le(b, 4)),
            device_time: u32_le(b, 8),
        },
        ACTION_MAGIC if b.len() >= 20 => Event::Action {
            action: Action::from_raw(u32_le(b, 0), u32_le(b, 4)),
            timestamp_ns: u64_le(b, 12),
        },
        SENSOR_MAGIC => decode_sensor(b).unwrap_or_else(|| unknown(f)),
        _ => unknown(f),
    }
}

fn unknown(f: &Frame) -> Event {
    Event::Unknown { magic: f.magic, raw: f.body.clone() }
}

/// protobuf: campo 3 { campo 1 = índice (omitido se 0), campo 2 = float }
fn decode_sensor(b: &[u8]) -> Option<Event> {
    if b.first() != Some(&0x1a) {
        return None;
    }
    let mut i = 1;
    read_varint(b, &mut i).ok()?;
    let mut index = 0;
    if b.get(i) == Some(&0x08) {
        i += 1;
        index = read_varint(b, &mut i).ok()?;
    }
    if b.get(i) == Some(&0x15) && b.len() >= i + 5 {
        return Some(Event::Sensor { index, value: f32_le(b, i + 1) });
    }
    None
}

impl Event {
    /// Mesmo texto de describe_key()/describe_event() do xr_control_peek.py.
    pub fn describe_pt(&self) -> String {
        match self {
            Self::Key { key, state, device_time } => {
                format!("tecla: {} {} (t={device_time})", key.label_pt(), state.label_pt())
            }
            Self::Action { action, timestamp_ns } => {
                format!("ação: {} (t={:.3}s)", action.label_pt(), *timestamp_ns as f64 / 1e9)
            }
            Self::Sensor { index, value } => {
                format!("sensor {index} = {:.1} (provável temperatura, °C)", f64::from(*value))
            }
            Self::Unknown { magic, raw } => {
                format!("magic={} bruto={}", hex(magic), hex(&raw[..raw.len().min(48)]))
            }
        }
    }
}
