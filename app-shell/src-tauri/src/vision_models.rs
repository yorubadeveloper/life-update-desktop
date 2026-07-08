//! Curated vision-engine registry, mirrors
//! `agent/src/life_update_agent/inference/vision_models.py` exactly.
//! "tesseract" is a bundled local binary (not an Ollama tag); the other
//! two are Ollama vision models, pulled the same way as the summarization
//! model in `models.rs`.

use serde::Serialize;

#[derive(Serialize, Clone, Copy)]
pub struct VisionChoice {
    pub name: &'static str,
    pub size_human: &'static str,
    pub description: &'static str,
}

pub const DEFAULT_VISION_ENGINE: &str = "tesseract";
pub const TESSERACT_ENGINE: &str = "tesseract";

pub const VISION_CHOICES: &[VisionChoice] = &[
    VisionChoice { name: "tesseract", size_human: "~35 MB", description: "fast, text-only, runs inline (recommended default)" },
    VisionChoice { name: "qwen2.5vl:3b", size_human: "3.2 GB", description: "reads screen content semantically, runs when idle" },
    VisionChoice { name: "qwen2.5vl:7b", size_human: "6.0 GB", description: "higher quality, slower" },
];

pub fn is_ollama_backed(name: &str) -> bool {
    name != TESSERACT_ENGINE
}
