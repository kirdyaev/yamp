use serde::Serialize;
use std::io::Write;
use std::process::{Command, Stdio};

const SERVICE: &str = "dev.kirdyaev.yamamp";
const ACCOUNT: &str = "oauth-token";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenStatus {
    pub has_token: bool,
    pub last4: Option<String>,
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

pub fn save_token(token: &str) -> Result<(), String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err("Token cannot be empty".to_string());
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return Err("Token contains invalid newline characters".to_string());
    }

    let hex = hex_encode(trimmed.as_bytes());
    let mut child = Command::new("/usr/bin/security")
        .arg("-i")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to invoke security CLI: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        let cmd = format!("add-generic-password -U -s {SERVICE} -a {ACCOUNT} -X {hex}\n");
        stdin
            .write_all(cmd.as_bytes())
            .map_err(|e| format!("Failed to write to security CLI: {e}"))?;
        stdin
            .flush()
            .map_err(|e| format!("Failed to flush security CLI: {e}"))?;
    }

    let status = child
        .wait()
        .map_err(|e| format!("Failed waiting on security CLI: {e}"))?;
    if !status.success() {
        return Err("macOS Keychain rejected saving the token".to_string());
    }
    Ok(())
}

pub fn load_token() -> Result<Option<String>, String> {
    let output = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", SERVICE, "-a", ACCOUNT, "-w"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to read from macOS Keychain: {e}"))?;

    if !output.status.success() {
        return Ok(None);
    }

    let raw = String::from_utf8(output.stdout)
        .map_err(|_| "Keychain item is not valid UTF-8".to_string())?;
    let trimmed = raw.trim_matches(|c| c == '\n' || c == '\r').trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

pub fn delete_token() -> Result<(), String> {
    let output = Command::new("/usr/bin/security")
        .args(["delete-generic-password", "-s", SERVICE, "-a", ACCOUNT])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to invoke security CLI: {e}"))?;

    if output.status.success() || output.status.code() == Some(44) {
        Ok(())
    } else {
        Err("Failed to remove token from macOS Keychain".to_string())
    }
}

pub fn token_status() -> Result<TokenStatus, String> {
    match load_token()? {
        Some(t) => {
            let char_count = t.chars().count();
            let last4 = if char_count >= 4 {
                Some(t.chars().skip(char_count - 4).collect())
            } else {
                None
            };
            Ok(TokenStatus {
                has_token: true,
                last4,
            })
        }
        None => Ok(TokenStatus {
            has_token: false,
            last4: None,
        }),
    }
}
