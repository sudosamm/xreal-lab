//! Tarefas da IMU (52998) e do vsync (52996): só leitura.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::time::{interval_at, sleep, MissedTickBehavior};
use xr_fusion::Fusion;
use xr_proto::frame::FrameDecoder;
use xr_proto::imu::{self, ImuKind};
use xr_proto::vsync;

use crate::rate::RateMeter;
use crate::{connect_any, Link, Shared};

pub async fn run_imu(sh: Arc<Shared>) {
    loop {
        if let Some((s, host)) = connect_any(&sh.cfg, |e| e.imu_port).await {
            sh.state.send_modify(|st| st.imu = Link::Connected { host });
            if let Err(e) = imu_session(&sh, s).await {
                tracing::info!("IMU: {e}");
            }
            sh.state.send_modify(|st| {
                st.imu = Link::Disconnected;
                st.imu_hz = 0.0;
            });
            sh.orientation.send_replace(None);
        }
        sleep(sh.cfg.retry_delay).await;
    }
}

async fn imu_session(sh: &Shared, mut s: TcpStream) -> Result<(), String> {
    let mut calib_rx = sh.calibration.subscribe();
    let mut fusion = Fusion::new(calib_rx.borrow_and_update().as_deref().cloned(), sh.cfg.filter);
    let mut recenter_seen = sh.recenter.load(Ordering::Relaxed);
    let mut dec = FrameDecoder::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut rate = RateMeter::new();
    // o primeiro tique só depois de 1 s: senão a primeira taxa sai de um intervalo quase zero
    let mut tick = interval_at(tokio::time::Instant::now() + Duration::from_secs(1), Duration::from_secs(1));
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last_data = Instant::now();
    let mut last_pub: Option<Instant> = None;
    loop {
        tokio::select! {
            n = s.read(&mut buf) => {
                let n = n.map_err(|e| e.to_string())?;
                if n == 0 {
                    return Err("conexão fechada".into());
                }
                last_data = Instant::now();
                dec.push(&buf[..n]);
                while let Some(f) = dec.next_frame().map_err(|e| e.to_string())? {
                    if f.magic != imu::MAGIC {
                        continue;
                    }
                    let Ok(sample) = imu::parse(&f.body) else { continue };
                    if sample.kind == ImuKind::GyroAccel {
                        rate.tick();
                    }
                    let rc = sh.recenter.load(Ordering::Relaxed);
                    if rc != recenter_seen {
                        fusion.recenter();
                        recenter_seen = rc;
                    }
                    if let Some(o) = fusion.update(&sample) {
                        if last_pub.map_or(true, |t| t.elapsed() >= sh.cfg.orientation_period) {
                            sh.orientation.send_replace(Some(o));
                            last_pub = Some(Instant::now());
                        }
                    }
                }
            }
            Ok(()) = calib_rx.changed() => {
                fusion = Fusion::new(calib_rx.borrow_and_update().as_deref().cloned(), sh.cfg.filter);
            }
            _ = tick.tick() => {
                let hz = rate.take_hz();
                sh.state.send_modify(|st| st.imu_hz = hz);
                if last_data.elapsed() > sh.cfg.imu_silence {
                    return Err("IMU em silêncio".into());
                }
            }
        }
    }
}

pub async fn run_vsync(sh: Arc<Shared>) {
    loop {
        if let Some((s, host)) = connect_any(&sh.cfg, |e| e.vsync_port).await {
            sh.state.send_modify(|st| st.vsync = Link::Connected { host });
            if let Err(e) = vsync_session(&sh, s).await {
                tracing::info!("vsync: {e}");
            }
            sh.state.send_modify(|st| {
                st.vsync = Link::Disconnected;
                st.vsync_hz = 0.0;
            });
        }
        sleep(sh.cfg.retry_delay).await;
    }
}

/// O vsync para ~12 s ao trocar o modo de tela: silêncio aqui não é queda.
async fn vsync_session(sh: &Shared, mut s: TcpStream) -> Result<(), String> {
    let mut dec = FrameDecoder::new();
    let mut buf = vec![0u8; 4096];
    let mut rate = RateMeter::new();
    let mut last = None;
    // o primeiro tique só depois de 1 s: senão a primeira taxa sai de um intervalo quase zero
    let mut tick = interval_at(tokio::time::Instant::now() + Duration::from_secs(1), Duration::from_secs(1));
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            n = s.read(&mut buf) => {
                let n = n.map_err(|e| e.to_string())?;
                if n == 0 {
                    return Err("conexão fechada".into());
                }
                dec.push(&buf[..n]);
                while let Some(f) = dec.next_frame().map_err(|e| e.to_string())? {
                    if let Ok(v) = vsync::parse_frame(&f) {
                        rate.tick();
                        last = Some(v.counter);
                    }
                }
            }
            _ = tick.tick() => {
                let hz = rate.take_hz();
                sh.state.send_modify(|st| {
                    st.vsync_hz = hz;
                    st.last_vsync_counter = last;
                });
            }
        }
    }
}
