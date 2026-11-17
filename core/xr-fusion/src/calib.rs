//! Calibração de fábrica (resposta do 27 1f). Só os campos usados são guardados:
//! o número de série (FSN) e o resto do JSON são descartados.

use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CalibError {
    #[error("configuração não é um JSON válido ({0})")]
    Json(String),
    #[error("campo ausente ou inválido: {0}")]
    Field(&'static str),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    /// `display.target_q_left_display`, Hamilton xyzw: rotação IMU → tela (usada transposta).
    pub display_q_left: [f64; 4],
    /// `IMU.device_1.gyro_bias` (rad/s, eixos do IMU)
    pub gyro_bias: Option<[f64; 3]>,
    /// `IMU.device_1.gyro_bias_temp_data`: (temperatura °C, bias), em ordem de temperatura
    pub gyro_bias_temp: Vec<(f64, [f64; 3])>,
    /// `IMU.device_1.accel_bias` (m/s²)
    pub accel_bias: Option<[f64; 3]>,
}

fn arr<const N: usize>(v: &Value) -> Option<[f64; N]> {
    let a = v.as_array()?;
    if a.len() != N {
        return None;
    }
    let mut out = [0.0; N];
    for (o, x) in out.iter_mut().zip(a) {
        *o = x.as_f64().filter(|f| f.is_finite())?;
    }
    Some(out)
}

impl Calibration {
    pub fn from_json_str(s: &str) -> Result<Self, CalibError> {
        // a mensagem de erro do serde_json só traz linha e coluna, nunca o conteúdo
        let v: Value = serde_json::from_str(s).map_err(|e| CalibError::Json(e.to_string()))?;
        Self::from_value(&v)
    }

    pub fn from_value(v: &Value) -> Result<Self, CalibError> {
        let display_q_left = arr::<4>(&v["display"]["target_q_left_display"])
            .ok_or(CalibError::Field("display.target_q_left_display"))?;
        let imu = &v["IMU"]["device_1"];
        let mut gyro_bias_temp: Vec<(f64, [f64; 3])> = imu["gyro_bias_temp_data"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|e| Some((e["temp"].as_f64().filter(|t| t.is_finite())?, arr::<3>(&e["bias"])?)))
                    .collect()
            })
            .unwrap_or_default();
        gyro_bias_temp.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(Self {
            display_q_left,
            gyro_bias: arr::<3>(&imu["gyro_bias"]),
            gyro_bias_temp,
            accel_bias: arr::<3>(&imu["accel_bias"]),
        })
    }

    /// Bias do giroscópio para a temperatura dada: interpolado na tabela (limitado às pontas),
    /// ou o `gyro_bias` fixo se não houver tabela.
    /// HIPÓTESE ainda não validada com o óculos parado: o bias é subtraído da leitura bruta.
    pub fn gyro_bias_at(&self, temp_c: f64) -> Option<[f64; 3]> {
        let t = &self.gyro_bias_temp;
        if t.is_empty() || !temp_c.is_finite() {
            return self.gyro_bias;
        }
        if temp_c <= t[0].0 {
            return Some(t[0].1);
        }
        let last = t[t.len() - 1];
        if temp_c >= last.0 {
            return Some(last.1);
        }
        let i = t.partition_point(|e| e.0 <= temp_c);
        let (t0, b0) = t[i - 1];
        let (t1, b1) = t[i];
        let k = if t1 > t0 { (temp_c - t0) / (t1 - t0) } else { 0.0 };
        Some([0, 1, 2].map(|j| b0[j] + k * (b1[j] - b0[j])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_o_quaternio() {
        let c = Calibration::from_json_str(r#"{"FSN":"X","display":{"target_q_left_display":[0.3,0,0,0.95]}}"#).unwrap();
        assert_eq!(c.display_q_left, [0.3, 0.0, 0.0, 0.95]);
        assert_eq!(c.gyro_bias, None);
        assert_eq!(c.gyro_bias_at(40.0), None);
    }

    #[test]
    fn sem_quaternio_falha() {
        assert_eq!(Calibration::from_json_str("{}"), Err(CalibError::Field("display.target_q_left_display")));
        assert!(matches!(Calibration::from_json_str("{"), Err(CalibError::Json(_))));
    }

    #[test]
    fn interpola_bias_por_temperatura() {
        let c = Calibration::from_json_str(
            r#"{"display":{"target_q_left_display":[0,0,0,1]},
                "IMU":{"device_1":{"gyro_bias":[9,9,9],"gyro_bias_temp_data":[
                  {"temp":30,"bias":[0.3,0,0]},{"temp":20,"bias":[0.1,0,0]},{"temp":"x","bias":[1,1,1]}]}}}"#,
        )
        .unwrap();
        assert_eq!(c.gyro_bias_temp.len(), 2);
        assert_eq!(c.gyro_bias_at(10.0), Some([0.1, 0.0, 0.0]));
        assert_eq!(c.gyro_bias_at(40.0), Some([0.3, 0.0, 0.0]));
        let mid = c.gyro_bias_at(25.0).unwrap();
        assert!((mid[0] - 0.2).abs() < 1e-12);
        assert_eq!(c.gyro_bias_at(f64::NAN), Some([9.0, 9.0, 9.0]));
    }
}
