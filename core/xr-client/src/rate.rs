use std::time::Instant;

/// Conta eventos e devolve a taxa (por segundo) desde a última leitura.
pub struct RateMeter {
    count: u64,
    since: Instant,
}

impl RateMeter {
    pub fn new() -> Self {
        Self { count: 0, since: Instant::now() }
    }

    pub fn tick(&mut self) {
        self.count += 1;
    }

    pub fn take_hz(&mut self) -> f32 {
        let now = Instant::now();
        let secs = now.duration_since(self.since).as_secs_f64();
        let hz = if secs > 0.0 { self.count as f64 / secs } else { 0.0 };
        self.count = 0;
        self.since = now;
        hz as f32
    }
}
