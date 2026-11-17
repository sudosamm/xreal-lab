//! xr-fusion — calibração de fábrica e orientação da cabeça a partir da IMU do One Pro.
//!
//! Referencial de saída (o da tela): x direita, y baixo, z frente.
//! Arfagem + = olhar para cima, guinada + = virar à direita, rolagem + = orelha direita para baixo.
//! Lógica de rotação portada de `load_rotation` do tools/imu_peek.py.
#![forbid(unsafe_code)]

pub mod calib;
pub mod filter;
pub mod rotation;

pub use calib::{CalibError, Calibration};
pub use filter::{FilterConfig, Fusion, Orientation};
pub use rotation::{tilt_deg, DisplayRotation};

pub use glam::{DQuat, DVec3};
