//! On-device OCR via Apple's Vision framework (VNRecognizeTextRequest) -
//! the same approach screenpipe uses on macOS. Hardware-accelerated,
//! nothing bundled, nothing downloaded, runs in-process in ~100ms.
//! Replaces the Python agent's Tesseract dependency entirely.

use objc2::rc::Retained;
use objc2::AllocAnyThread;
use objc2_foundation::{NSArray, NSData, NSDictionary};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizedTextObservation, VNRecognizeTextRequest, VNRequest,
    VNRequestTextRecognitionLevel,
};

/// Recognize text in a PNG image, returned as newline-joined lines in
/// natural (top-to-bottom) order.
pub fn ocr_png(png_bytes: &[u8]) -> Result<String, String> {
    {
        let data = NSData::with_bytes(png_bytes);
        let options: Retained<NSDictionary<_, _>> = NSDictionary::new();
        let handler = VNImageRequestHandler::initWithData_options(
            VNImageRequestHandler::alloc(),
            &data,
            &options,
        );

        let request = VNRecognizeTextRequest::new();
        request.setRecognitionLevel(VNRequestTextRecognitionLevel::Fast);
        request.setUsesLanguageCorrection(false);

        let base: Retained<VNRequest> = Retained::into_super(Retained::into_super(request.clone()));
        let requests: Retained<NSArray<VNRequest>> = NSArray::from_retained_slice(&[base]);

        handler
            .performRequests_error(&requests)
            .map_err(|e| e.to_string())?;

        let mut lines: Vec<String> = Vec::new();
        if let Some(results) = request.results() {
            for observation in results.iter() {
                let candidates = observation.topCandidates(1);
                if let Some(candidate) = candidates.firstObject() {
                    let s = candidate.string().to_string();
                    if !s.trim().is_empty() {
                        lines.push(s);
                    }
                }
            }
        }
        let _ = &results_type_hint(&request); // keep type inference stable across objc2 versions
        Ok(lines.join("\n"))
    }
}

// objc2-vision types results() as NSArray<VNRecognizedTextObservation> for
// this request; this no-op exists only to pin that in one place.
fn results_type_hint(request: &VNRecognizeTextRequest) -> Option<Retained<NSArray<VNRecognizedTextObservation>>> {
    request.results()
}

#[cfg(test)]
mod tests {
    /// Real-OCR smoke test, gated on an env var so CI/plain `cargo test`
    /// stays hermetic: LU_OCR_TEST_PNG=/path/to/screenshot.png points at an
    /// image known to contain text; the test asserts recognition finds some.
    #[test]
    fn recognizes_text_in_real_screenshot() {
        let Ok(path) = std::env::var("LU_OCR_TEST_PNG") else { return };
        let png = std::fs::read(&path).expect("test image readable");
        let text = super::ocr_png(&png).expect("ocr runs");
        assert!(
            text.len() > 20,
            "expected recognizable text in {path}, got: {text:?}"
        );
    }
}
