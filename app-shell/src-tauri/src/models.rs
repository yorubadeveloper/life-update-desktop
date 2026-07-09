//! Curated summarization-engine registry. Apple Intelligence is the
//! default: on-device, managed by macOS, nothing to download. The Ollama
//! models are an alternative for Macs without Apple Intelligence - they
//! talk to the user's own Ollama install (nothing is bundled).

use serde::Serialize;

#[derive(Serialize, Clone, Copy)]
pub struct ModelChoice {
    pub name: &'static str,
    pub size_human: &'static str,
    pub description: &'static str,
}

pub const APPLE_ENGINE: &str = "apple-intelligence";
pub const DEFAULT_MODEL: &str = APPLE_ENGINE;

pub const MODEL_CHOICES: &[ModelChoice] = &[
    ModelChoice {
        name: APPLE_ENGINE,
        size_human: "built-in",
        description: "Apple Intelligence - on-device, private, nothing to download (recommended)",
    },
    ModelChoice { name: "qwen2.5:0.5b", size_human: "398 MB", description: "fastest, lowest quality (via the Ollama app)" },
    ModelChoice { name: "qwen2.5:1.5b", size_human: "986 MB", description: "good balance for low-end machines (via the Ollama app)" },
    ModelChoice { name: "llama3.2:1b", size_human: "1.3 GB", description: "good balance (via the Ollama app)" },
    ModelChoice { name: "phi3:mini", size_human: "2.2 GB", description: "strong summaries (via the Ollama app)" },
    ModelChoice { name: "llama3.2:3b", size_human: "2.0 GB", description: "higher quality, slower (via the Ollama app)" },
];

pub fn is_ollama_model(name: &str) -> bool {
    name != APPLE_ENGINE
}
