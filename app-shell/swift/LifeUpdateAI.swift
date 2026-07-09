// The one non-Rust piece: a tiny bridge to Apple Intelligence.
//
// The FoundationModels framework (macOS 26+) is Swift-only, so the Rust
// core shells out to this helper. `check` reports availability; `summarize`
// reads a redacted activity log on stdin and prints a JSON object
// {project, category, summary} on stdout. The model is managed entirely by
// macOS: nothing downloaded, nothing resident in our process.
//
// Deliberately avoids the @Generable guided-generation macro: the macro
// plugin ships only with full Xcode, not the Command Line Tools, so plain
// prompting + JSON extraction keeps the build working with CLT alone.
//
// Built by scripts/build-ai-helper.sh into src-tauri/resources/ai-helper/.

import Foundation
import FoundationModels

func fail(_ message: String, code: Int32 = 1) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(code)
}

let PROMPT_HEADER = """
You are summarizing a developer's work session from redacted activity logs.
Respond with ONLY a JSON object with exactly these keys:
- "project": a short (2-6 word) name for what they were working on
- "category": one of "deep_work", "maintenance", "meeting", "other"
- "summary": one or two plain sentences describing what was done, written like a changelog entry

Activity log:
"""

/// Extract the first {...} block from model output (models sometimes wrap
/// JSON in prose or code fences despite instructions).
func extractJSONObject(_ text: String) -> [String: Any]? {
    guard let start = text.firstIndex(of: "{"), let end = text.lastIndex(of: "}") else {
        return nil
    }
    let candidate = String(text[start...end])
    guard let data = candidate.data(using: .utf8),
          let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
        return nil
    }
    return obj
}

@main
struct LifeUpdateAI {
    static func main() async {
        let command = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "check"

        switch SystemLanguageModel.default.availability {
        case .available:
            break
        case .unavailable(let reason):
            fail("Apple Intelligence is not available: \(reason)", code: 2)
        }

        if command == "check" {
            print("available")
            exit(0)
        }
        guard command == "summarize" else {
            fail("unknown command: \(command)")
        }

        let input = FileHandle.standardInput.readDataToEndOfFile()
        guard let activity = String(data: input, encoding: .utf8),
              !activity.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            fail("no activity log on stdin")
        }

        let session = LanguageModelSession(
            instructions: "You summarize a developer's work session from redacted activity logs into a changelog-style record. You always answer with a single JSON object and nothing else."
        )

        do {
            let response = try await session.respond(to: PROMPT_HEADER + "\n" + activity + "\n\nJSON:")
            guard let obj = extractJSONObject(response.content) else {
                fail("model output was not valid JSON")
            }
            let out: [String: String] = [
                "project": obj["project"] as? String ?? "",
                "category": obj["category"] as? String ?? "",
                "summary": obj["summary"] as? String ?? "",
            ]
            let data = try JSONSerialization.data(withJSONObject: out)
            FileHandle.standardOutput.write(data)
        } catch {
            fail("generation failed: \(error)")
        }
    }
}
