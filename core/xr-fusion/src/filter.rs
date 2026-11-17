//! Filtro de orientação (Mahony sem termo integral) no referencial da tela.
//!
//! - Arfagem e rolagem são corrigidas pelo acelerômetro (gravidade).
//! - A guinada só vem do giroscópio: deriva com o tempo. Para conter a deriva, o bias do
//!   giroscópio é estimado sempre que o óculos fica parado (o de fábrica não bateu com o medido).
//! - [`Fusion::recenter`] zera a guinada (botão "recentrar").
//! - O magnetômetro ainda não é usado.

use glam::{DQuat, DVec3};
use xr_proto::imu::{ImuKind, ImuSample};

use crate::{Calibration, DisplayRotation};

pub const GRAVITY: f64 = 9.806_65;
/// "Cima" no mundo: o referencial tem y para baixo.
const UP: DVec3 = DVec3::new(0.0, -1.0, 0.0);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilterConfig {
    /// ganho da correção pela gravidade (1/s)
    pub kp: f64,
    /// ganho no começo, para convergir rápido
    pub kp_startup: f64,
    pub startup_s: f64,
    /// só corrige pela gravidade se |a| estiver a menos disto de g (m/s²)
    pub accel_gate: f64,
    /// abaixo disto (rad/s, média móvel) o óculos é considerado parado
    pub still_gyro: f64,
    /// tempo parado para tirar uma média do bias (s)
    pub still_window_s: f64,
    /// peso de cada nova média no bias aprendido
    pub bias_alpha: f64,
    /// intervalos maiores que isto entre amostras não são integrados (s)
    pub max_dt: f64,
    /// usar o bias de fábrica (giroscópio e acelerômetro) antes de aprender um parado.
    /// Desligado: em 16/11 o bias medido parado não bateu com o de fábrica (ver notes/arquitetura-v1.md).
    pub use_factory_bias: bool,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            kp: 1.0,
            kp_startup: 10.0,
            startup_s: 1.0,
            accel_gate: 1.5,
            still_gyro: 0.02,
            still_window_s: 1.0,
            bias_alpha: 0.3,
            max_dt: 0.1,
            use_factory_bias: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Orientation {
    pub timestamp_ns: u64,
    /// tela → mundo, com a guinada recentrada
    pub q: DQuat,
    pub yaw_deg: f64,
    pub pitch_deg: f64,
    pub roll_deg: f64,
    /// velocidade angular já sem bias, no referencial da tela (°/s): x = arfagem, y = guinada, z = rolagem
    pub rate_dps: DVec3,
    pub still: bool,
    pub temp_c: f32,
}

#[derive(Debug, Clone)]
pub struct Fusion {
    cfg: FilterConfig,
    rot: DisplayRotation,
    calib: Option<Calibration>,
    q: DQuat,
    yaw_ref: DQuat,
    initialized: bool,
    last_ts: Option<u64>,
    elapsed_s: f64,
    learned_bias: Option<DVec3>,
    gyro_lp: f64,
    still_sum: DVec3,
    still_time: f64,
    still: bool,
}

fn v3(a: [f32; 3]) -> DVec3 {
    DVec3::new(f64::from(a[0]), f64::from(a[1]), f64::from(a[2]))
}

/// (guinada, arfagem, rolagem) em graus de uma orientação tela → mundo.
pub fn euler_deg(q: DQuat) -> (f64, f64, f64) {
    let f = q * DVec3::Z;
    let r = q * DVec3::X;
    let d = q * DVec3::Y;
    let yaw = f.x.atan2(f.z);
    let pitch = (-f.y).atan2(f.x.hypot(f.z));
    let roll = r.y.atan2(d.y);
    (yaw.to_degrees(), pitch.to_degrees(), roll.to_degrees())
}

impl Fusion {
    pub fn new(calib: Option<Calibration>, cfg: FilterConfig) -> Self {
        let rot = calib.as_ref().map_or(DisplayRotation::identity(), |c| DisplayRotation::from_xyzw(c.display_q_left));
        Self {
            cfg,
            rot,
            calib,
            q: DQuat::IDENTITY,
            yaw_ref: DQuat::IDENTITY,
            initialized: false,
            last_ts: None,
            elapsed_s: 0.0,
            learned_bias: None,
            gyro_lp: 0.0,
            still_sum: DVec3::ZERO,
            still_time: 0.0,
            still: false,
        }
    }

    pub fn rotation(&self) -> DisplayRotation {
        self.rot
    }

    /// Bias atual do giroscópio (rad/s, eixos do IMU) e se ele já foi aprendido com o óculos parado.
    pub fn gyro_bias(&self, temp_c: f64) -> (DVec3, bool) {
        if let Some(b) = self.learned_bias {
            return (b, true);
        }
        let factory = self
            .calib
            .as_ref()
            .filter(|_| self.cfg.use_factory_bias)
            .and_then(|c| c.gyro_bias_at(temp_c))
            .map_or(DVec3::ZERO, DVec3::from_array);
        (factory, false)
    }

    /// Zera a guinada na direção atual.
    pub fn recenter(&mut self) {
        let (yaw, _, _) = euler_deg(self.q);
        self.yaw_ref = DQuat::from_rotation_y(-yaw.to_radians());
    }

    /// Recomeça do zero (por exemplo, ao reconectar).
    pub fn reset(&mut self) {
        *self = Self::new(self.calib.take(), self.cfg);
    }

    /// Processa uma amostra. Só as do tipo giroscópio + acelerômetro geram saída.
    pub fn update(&mut self, s: &ImuSample) -> Option<Orientation> {
        if s.kind != ImuKind::GyroAccel {
            return None;
        }
        let gyro_raw = v3(s.gyro);
        let accel_raw = v3(s.accel);
        if !gyro_raw.is_finite() || !accel_raw.is_finite() {
            return None;
        }
        let dt = match self.last_ts {
            Some(prev) if s.timestamp_ns > prev => (s.timestamp_ns - prev) as f64 * 1e-9,
            _ => 0.0,
        };
        self.last_ts = Some(s.timestamp_ns);
        let dt = if dt > self.cfg.max_dt { 0.0 } else { dt };

        let temp = f64::from(s.temp_c);
        let (bias, _) = self.gyro_bias(temp);
        let accel_bias = self
            .calib
            .as_ref()
            .filter(|_| self.cfg.use_factory_bias)
            .and_then(|c| c.accel_bias)
            .map_or(DVec3::ZERO, DVec3::from_array);
        let g = self.rot.to_display(gyro_raw - bias);
        let a = self.rot.to_display(accel_raw - accel_bias);
        let a_norm = a.length();
        let accel_ok = a_norm > 1e-6 && (a_norm - GRAVITY).abs() < self.cfg.accel_gate;

        if !self.initialized {
            if !accel_ok {
                return None;
            }
            self.q = DQuat::from_rotation_arc(a / a_norm, UP);
            self.initialized = true;
            self.recenter();
        }

        self.update_still(gyro_raw, g, accel_ok, dt);

        if dt > 0.0 {
            let mut omega = g;
            if accel_ok {
                let measured = a / a_norm;
                let estimated = self.q.inverse() * UP;
                let kp = if self.elapsed_s < self.cfg.startup_s { self.cfg.kp_startup } else { self.cfg.kp };
                omega += measured.cross(estimated) * kp;
            }
            self.q = (self.q * DQuat::from_scaled_axis(omega * dt)).normalize();
            self.elapsed_s += dt;
        }

        let q_out = self.yaw_ref * self.q;
        let (yaw, pitch, roll) = euler_deg(q_out);
        Some(Orientation {
            timestamp_ns: s.timestamp_ns,
            q: q_out,
            yaw_deg: yaw,
            pitch_deg: pitch,
            roll_deg: roll,
            rate_dps: DVec3::new(g.x.to_degrees(), g.y.to_degrees(), g.z.to_degrees()),
            still: self.still,
            temp_c: s.temp_c,
        })
    }

    fn update_still(&mut self, gyro_raw: DVec3, g_corrected: DVec3, accel_ok: bool, dt: f64) {
        if dt <= 0.0 {
            return;
        }
        let k = (dt / 0.05).min(1.0); // média móvel de ~50 ms
        self.gyro_lp += k * (g_corrected.length() - self.gyro_lp);
        self.still = accel_ok && self.gyro_lp < self.cfg.still_gyro;
        if !self.still {
            self.still_sum = DVec3::ZERO;
            self.still_time = 0.0;
            return;
        }
        self.still_sum += gyro_raw * dt;
        self.still_time += dt;
        if self.still_time >= self.cfg.still_window_s {
            let mean = self.still_sum / self.still_time;
            self.learned_bias = Some(match self.learned_bias {
                Some(b) => b + (mean - b) * self.cfg.bias_alpha,
                None => mean,
            });
            self.still_sum = DVec3::ZERO;
            self.still_time = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(ts: u64, gyro: DVec3, accel: DVec3) -> ImuSample {
        ImuSample {
            timestamp_ns: ts,
            kind: ImuKind::GyroAccel,
            gyro: gyro.as_vec3().to_array(),
            accel: accel.as_vec3().to_array(),
            mag: [xr_proto::imu::MAG_NO_DATA; 3],
            temp_c: 40.0,
        }
    }

    /// Roda `secs` segundos a 1 kHz com giroscópio e acelerômetro dados por `f(t)`.
    fn run(fu: &mut Fusion, t0: &mut u64, secs: f64, f: impl Fn(f64) -> (DVec3, DVec3)) -> Orientation {
        let mut last = None;
        for i in 0..(secs * 1000.0) as u64 {
            *t0 += 1_000_000;
            let (g, a) = f(i as f64 / 1000.0);
            if let Some(o) = fu.update(&sample(*t0, g, a)) {
                last = Some(o);
            }
        }
        last.unwrap()
    }

    fn accel_for(q_body_to_world: DQuat) -> DVec3 {
        q_body_to_world.inverse() * UP * GRAVITY
    }

    #[test]
    fn inclinacao_parada() {
        for (pitch, roll) in [(20.0f64, 0.0f64), (0.0, -15.0), (-30.0, 10.0)] {
            let q = DQuat::from_rotation_x(pitch.to_radians()) * DQuat::from_rotation_z(roll.to_radians());
            let mut fu = Fusion::new(None, FilterConfig::default());
            let mut t = 0;
            let o = run(&mut fu, &mut t, 3.0, |_| (DVec3::ZERO, accel_for(q)));
            assert!((o.pitch_deg - pitch).abs() < 0.5, "arfagem {} != {pitch}", o.pitch_deg);
            assert!((o.roll_deg - roll).abs() < 0.5, "rolagem {} != {roll}", o.roll_deg);
            assert!(o.yaw_deg.abs() < 0.5, "guinada {}", o.yaw_deg);
            // as fórmulas do imu_peek só coincidem com estes ângulos quando um deles é zero
            if pitch == 0.0 || roll == 0.0 {
                let (tp, tr) = crate::tilt_deg(accel_for(q));
                assert!((tp - pitch).abs() < 1e-6 && (tr - roll).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn virar_a_direita_90_graus() {
        let mut fu = Fusion::new(None, FilterConfig::default());
        let mut t = 0;
        let level = accel_for(DQuat::IDENTITY);
        run(&mut fu, &mut t, 0.5, |_| (DVec3::ZERO, level));
        let w = 90f64.to_radians();
        let o = run(&mut fu, &mut t, 1.0, |_| (DVec3::new(0.0, w, 0.0), level));
        assert!((o.yaw_deg - 90.0).abs() < 1.0, "guinada {}", o.yaw_deg);
        assert!(o.pitch_deg.abs() < 0.5 && o.roll_deg.abs() < 0.5);
        fu.recenter();
        let o = run(&mut fu, &mut t, 0.1, |_| (DVec3::ZERO, level));
        assert!(o.yaw_deg.abs() < 0.5, "depois de recentrar: {}", o.yaw_deg);
    }

    #[test]
    fn olhar_para_cima() {
        let mut fu = Fusion::new(None, FilterConfig::default());
        let mut t = 0;
        let w = 30f64.to_radians();
        // 1 s girando para cima a 30°/s, com o acelerômetro acompanhando
        let o = run(&mut fu, &mut t, 1.0, |s| {
            (DVec3::new(w, 0.0, 0.0), accel_for(DQuat::from_rotation_x(w * s)))
        });
        assert!((o.pitch_deg - 30.0).abs() < 1.0, "arfagem {}", o.pitch_deg);
    }

    #[test]
    fn aprende_bias_parado_e_para_de_derivar() {
        let mut fu = Fusion::new(None, FilterConfig::default());
        let mut t = 0;
        let level = accel_for(DQuat::IDENTITY);
        let bias = DVec3::new(0.001, 0.003, -0.002);
        run(&mut fu, &mut t, 10.0, |_| (bias, level));
        let (b, learned) = fu.gyro_bias(40.0);
        assert!(learned);
        assert!((b - bias).length() < 1e-4, "bias {b:?}");
        let y0 = run(&mut fu, &mut t, 0.01, |_| (bias, level)).yaw_deg;
        let y1 = run(&mut fu, &mut t, 60.0, |_| (bias, level)).yaw_deg;
        assert!((y1 - y0).abs() < 0.5, "deriva em 60 s: {}", y1 - y0);
    }

    #[test]
    fn ignora_magnetometro_e_lixo() {
        let mut fu = Fusion::new(None, FilterConfig::default());
        let mut s = sample(1, DVec3::ZERO, accel_for(DQuat::IDENTITY));
        s.kind = ImuKind::Mag;
        assert!(fu.update(&s).is_none());
        s.kind = ImuKind::GyroAccel;
        s.gyro[0] = f32::NAN;
        assert!(fu.update(&s).is_none());
        // em queda livre não dá para começar
        assert!(fu.update(&sample(2, DVec3::ZERO, DVec3::ZERO)).is_none());
    }
}
