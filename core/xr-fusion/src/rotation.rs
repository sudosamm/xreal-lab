//! IMU → referencial da tela. O IMU está girado ~35° em relação às telas.

use glam::{DQuat, DVec3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayRotation {
    q_inv: DQuat,
}

impl DisplayRotation {
    pub fn identity() -> Self {
        Self { q_inv: DQuat::IDENTITY }
    }

    /// `q` = `display.target_q_left_display` (Hamilton xyzw). Aplica a transposta, como o imu_peek.py.
    pub fn from_xyzw(q: [f64; 4]) -> Self {
        let q = DQuat::from_xyzw(q[0], q[1], q[2], q[3]).normalize();
        Self { q_inv: q.inverse() }
    }

    pub fn to_display(&self, v: DVec3) -> DVec3 {
        self.q_inv * v
    }
}

/// Arfagem e rolagem (graus) só pelo acelerômetro, já no referencial da tela.
/// Mesmas fórmulas do imu_peek.py: arfagem = atan2(z, −y), rolagem = atan2(−x, −y).
pub fn tilt_deg(a: DVec3) -> (f64, f64) {
    (a.z.atan2(-a.y).to_degrees(), (-a.x).atan2(-a.y).to_degrees())
}
