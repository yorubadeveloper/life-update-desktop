//! Curated vision-engine registry for screen watching. "native" is Apple's
//! Vision framework OCR - on-device, hardware-accelerated, nothing bundled
//! (it replaced the old Tesseract dependency). The qwen options are Ollama
//! vision models for semantic descriptions, via the user's own Ollama.

use serde::Serialize;

#[derive(Serialize, Clone, Copy)]
pub struct VisionChoice {
    pub name: &'static str,
    pub size_human: &'static str,
    pub description: &'static str,
}

pub const NATIVE_ENGINE: &str = "native";
pub const DEFAULT_VISION_ENGINE: &str = NATIVE_ENGINE;

pub const VISION_CHOICES: &[VisionChoice] = &[
    VisionChoice {
        name: NATIVE_ENGINE,
        size_human: "built-in",
        description: "Apple Vision OCR - on-device, instant, reads screen text (recommended)",
    },
    VisionChoice { name: "qwen2.5vl:3b", size_human: "3.2 GB", description: "describes screen content semantically, runs when idle (via the Ollama app)" },
    VisionChoice { name: "qwen2.5vl:7b", size_human: "6.0 GB", description: "higher quality, slower (via the Ollama app)" },
];

pub fn is_ollama_backed(name: &str) -> bool {
    // "tesseract" is the pre-rewrite name for the non-Ollama path; configs
    // written by older builds may still carry it.
    name != NATIVE_ENGINE && name != "tesseract"
}
