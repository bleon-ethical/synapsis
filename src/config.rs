use std::path::PathBuf;
use std::sync::OnceLock;

static QUIET: OnceLock<bool> = OnceLock::new();

pub fn is_quiet() -> bool {
    *QUIET.get_or_init(|| std::env::var("SYNAPSIS_QUIET").is_ok() || std::env::var("QUIET").is_ok())
}

pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("SYNAPSIS_DATA_DIR") {
        PathBuf::from(dir)
    } else {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("synapsis")
    }
}

pub fn port() -> u16 {
    std::env::var("SYNAPSIS_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(7438)
}

pub fn log_level() -> String {
    std::env::var("SYNAPSIS_LOG").unwrap_or_else(|_| "info".to_string())
}

pub fn db_key() -> Option<Vec<u8>> {
    if let Ok(hex_key) = std::env::var("SYNAPSIS_DB_KEY") {
        hex::decode(&hex_key).ok().filter(|k| !k.is_empty())
    } else if let Ok(b64_key) = std::env::var("SYNAPSIS_DB_KEY_BASE64") {
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &b64_key).ok()
    } else {
        None
    }
}

pub fn insecure_tls() -> bool {
    std::env::var("SYNAPSIS_INSECURE_TLS").is_ok()
}

pub fn allow_private_mcp() -> bool {
    std::env::var("SYNAPSIS_ALLOW_PRIVATE_MCP").is_ok()
}

pub fn allow_dangerous_shell() -> bool {
    std::env::var("SYNAPSIS_ALLOW_DANGEROUS_SHELL").is_ok()
}

pub fn secret_key() -> Option<String> {
    std::env::var("SYNAPSIS_SECRET_KEY")
        .ok()
        .filter(|k| !k.is_empty())
}

pub fn api_keys() -> Vec<String> {
    std::env::var("SYNAPSIS_API_KEYS")
        .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default()
}
