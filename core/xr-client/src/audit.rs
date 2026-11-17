//! Registro dos ajustes enviados: uma linha JSON por envio. Nunca contém o id do óculos nem a calibração.

use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use xr_proto::gate::{Adjustment, Proposal, Undo};

pub fn write(path: Option<&Path>, firmware: Option<&str>, adj: Adjustment, bytes: &[u8], proposal: Option<&Proposal>, result: &str) {
    let Some(path) = path else { return };
    let unix = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let anterior = proposal.and_then(|p| p.current).map_or("desconhecido".to_string(), |a| a.describe_pt());
    let desfazer = match proposal.map(|p| &p.undo) {
        Some(Undo::Send(a)) => format!("enviar {}", a.describe_pt()),
        Some(Undo::Manual(s)) => s.clone(),
        None => "?".into(),
    };
    let line = serde_json::json!({
        "unix": unix,
        "firmware": firmware,
        "ajuste": adj.describe_pt(),
        "bytes": bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "anterior": anterior,
        "desfazer": desfazer,
        "resultado": result,
    });
    let res = (|| -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(f, "{line}")
    })();
    if let Err(e) = res {
        tracing::warn!("não consegui gravar o registro de ajustes em {}: {e}", path.display());
    }
}
