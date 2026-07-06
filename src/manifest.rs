use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    /// rel_path → official-content sha256 (as of the last release)
    pub files: BTreeMap<String, String>,
}

pub fn sha256_hex(text: &str) -> String {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    format!("{:x}", h.finalize())
}

impl Manifest {
    pub fn path(claude_dir: &Path) -> std::path::PathBuf {
        claude_dir.join("harness/manifest.json")
    }

    pub fn load(claude_dir: &Path) -> Option<Manifest> {
        let text = std::fs::read_to_string(Self::path(claude_dir)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self, claude_dir: &Path) -> io::Result<()> {
        let path = Self::path(claude_dir);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)
    }
}
