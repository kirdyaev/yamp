use serde::Deserialize;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[derive(Deserialize)]
struct BootReply {
    port: u16,
}

pub struct SidecarManager {
    child: Option<Child>,
    port: Option<u16>,
    api_secret: String,
    stream_nonce: String,
}

impl SidecarManager {
    pub fn new() -> Self {
        Self {
            child: None,
            port: None,
            api_secret: String::new(),
            stream_nonce: String::new(),
        }
    }

    pub fn start(&mut self, initial_token: Option<String>) -> Result<(), String> {
        if let Some(child) = self.child.as_mut() {
            if let Ok(None) = child.try_wait() {
                if self.port.is_some() {
                    return Ok(());
                }
            }
        }
        self.stop();

        let root = find_project_root()
            .ok_or_else(|| "Could not locate Yamamp project root (sidecar/server.py)".to_string())?;
        let python_bin = root.join(".venv").join("bin").join("python");
        let server_py = root.join("sidecar").join("server.py");
        if !python_bin.exists() {
            return Err(format!("Python venv not found at {}", python_bin.display()));
        }
        if !server_py.exists() {
            return Err(format!("Sidecar script not found at {}", server_py.display()));
        }

        self.api_secret = random_hex_32()?;
        self.stream_nonce = random_hex_32()?;

        let mut child = Command::new(&python_bin)
            .arg(&server_py)
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("Failed to start Python sidecar: {e}"))?;

        let boot_payload = serde_json::json!({
            "secret": &self.api_secret,
            "streamNonce": &self.stream_nonce,
            "token": initial_token,
        });

        if let Some(mut stdin) = child.stdin.take() {
            let line = format!("{boot_payload}\n");
            stdin
                .write_all(line.as_bytes())
                .map_err(|e| format!("Failed writing sidecar bootstrap: {e}"))?;
            stdin
                .flush()
                .map_err(|e| format!("Failed flushing sidecar bootstrap: {e}"))?;
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Sidecar stdout unavailable".to_string())?;
        let mut reader = BufReader::new(stdout);
        let mut first_line = String::new();
        reader
            .read_line(&mut first_line)
            .map_err(|e| format!("Failed reading sidecar startup port: {e}"))?;
        if first_line.trim().is_empty() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Python sidecar exited before reporting port".to_string());
        }

        let reply: BootReply = serde_json::from_str(first_line.trim())
            .map_err(|e| format!("Invalid sidecar boot reply: {e}"))?;

        self.port = Some(reply.port);
        self.child = Some(child);
        Ok(())
    }

    fn ensure_running(&mut self) -> Result<u16, String> {
        let alive = match self.child.as_mut() {
            Some(c) => matches!(c.try_wait(), Ok(None)),
            None => false,
        };
        if alive {
            if let Some(port) = self.port {
                return Ok(port);
            }
        }
        let token = crate::keychain::load_token()?;
        self.start(token)?;
        self.port
            .ok_or_else(|| "Sidecar started without port".to_string())
    }

    pub fn update_token(&mut self, token: Option<String>) -> Result<(), String> {
        let port = self.ensure_running()?;
        let url = format!("http://127.0.0.1:{port}/internal/token");
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .build();

        match agent
            .post(&url)
            .set("X-Yamamp-Secret", &self.api_secret)
            .send_json(serde_json::json!({ "token": token }))
        {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(code, resp)) => {
                let detail = extract_error_message(resp).unwrap_or_else(|| format!("HTTP {code}"));
                Err(detail)
            }
            Err(ureq::Error::Transport(e)) => Err(format!("Sidecar connection error: {e}")),
        }
    }

    pub fn get_json(&mut self, path: &str) -> Result<serde_json::Value, String> {
        let port = self.ensure_running()?;
        let url = format!("http://127.0.0.1:{port}{path}");
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(20))
            .build();

        match agent
            .get(&url)
            .set("X-Yamamp-Secret", &self.api_secret)
            .call()
        {
            Ok(resp) => resp
                .into_json::<serde_json::Value>()
                .map_err(|e| format!("Invalid JSON from sidecar: {e}")),
            Err(ureq::Error::Status(code, resp)) => {
                let detail = extract_error_message(resp).unwrap_or_else(|| format!("HTTP {code}"));
                Err(detail)
            }
            Err(ureq::Error::Transport(e)) => Err(format!("Sidecar connection error: {e}")),
        }
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.port = None;
    }
}

impl Drop for SidecarManager {
    fn drop(&mut self) {
        self.stop();
    }
}

fn extract_error_message(resp: ureq::Response) -> Option<String> {
    resp.into_json::<serde_json::Value>()
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
}

fn random_hex_32() -> Result<String, String> {
    let mut f = File::open("/dev/urandom").map_err(|e| format!("Failed to open /dev/urandom: {e}"))?;
    let mut buf = [0u8; 32];
    f.read_exact(&mut buf)
        .map_err(|e| format!("Failed to read random bytes: {e}"))?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for b in buf {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    Ok(out)
}

fn has_sidecar(dir: &Path) -> bool {
    dir.join("sidecar").join("server.py").exists() && dir.join(".venv").join("bin").join("python").exists()
}

fn find_project_root() -> Option<PathBuf> {
    if let Ok(env_root) = std::env::var("YAMAMP_ROOT") {
        let p = PathBuf::from(env_root);
        if has_sidecar(&p) {
            return Some(p);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for anc in cwd.ancestors() {
            if has_sidecar(anc) {
                return Some(anc.to_path_buf());
            }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        for anc in exe.ancestors() {
            if has_sidecar(anc) {
                return Some(anc.to_path_buf());
            }
        }
    }
    let manifest_parent = Path::new(env!("CARGO_MANIFEST_DIR")).parent()?;
    if has_sidecar(manifest_parent) {
        return Some(manifest_parent.to_path_buf());
    }
    None
}
