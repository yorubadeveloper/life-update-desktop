//! Curated model registry - mirrors
//! `agent/src/life_update_agent/inference/models.py` exactly. Kept as a
//! small duplicated const list rather than an IPC round-trip so the
//! settings dropdown can render instantly without waiting on the Python
//! CLI; `list_models` still asks Ollama directly for download status.

use serde::Serialize;

#[derive(Serialize, Clone, Copy)]
pub struct ModelChoice {
    pub name: &'static str,
    pub size_human: &'static str,
    pub description: &'static str,
}

pub const DEFAULT_MODEL: &str = "phi3:mini";

pub const MODEL_CHOICES: &[ModelChoice] = &[
    ModelChoice { name: "qwen2.5:0.5b", size_human: "398 MB", description: "fastest, lowest quality" },
    ModelChoice { name: "qwen2.5:1.5b", size_human: "986 MB", description: "good balance for low-end machines" },
    ModelChoice { name: "llama3.2:1b", size_human: "1.3 GB", description: "good balance" },
    ModelChoice { name: "phi3:mini", size_human: "2.2 GB", description: "recommended default" },
    ModelChoice { name: "llama3.2:3b", size_human: "2.0 GB", description: "higher quality, slower" },
];
