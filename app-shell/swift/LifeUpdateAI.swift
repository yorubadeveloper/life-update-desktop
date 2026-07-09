// The one non-Rust piece: a tiny bridge to Apple Intelligence.
//
// The FoundationModels framework (macOS 26+) is Swift-only, so the Rust
// core shells out to this helper. `check` reports availability; `summarize`
// reads a redacted activity log on stdin and prints a JSON object
// {project, category, summary} on stdout.
//
// Uses guided generation with a *runtime* schema (DynamicGenerationSchema)
// rather than the @Generable macro - the macro's compiler plugin ships only
// with full Xcode, while the dynamic API works with Command Line Tools
// alone. Guided generation structurally guarantees the output shape: the
// category is a real enum (can't hallucinate a fifth value), and there is
// no JSON-in-prose to parse. Temperature is kept low: summaries should be
// boring and factual.
//
// Prompt rules deliberately contain NO example names of any kind - small
// models parrot literal examples straight into their output.
//
// Built by scripts/build-ai-helper.sh into src-tauri/resources/ai-helper/.

import Foundation
import FoundationModels

func fail(_ message: String, code: Int32 = 1) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(code)
}

let INSTRUCTIONS = """
You summarize a user's work session from redacted activity logs into a changelog-style record.

Rules:
- The apps in the log (terminals, editors, browsers) are tools the user was using. The user does not build or work for those apps. Never describe the session as improving or working on one of those apps.
- Window titles usually contain the real project, file, or document name. Take the project name from there, never from a tool or app name.
- Never output a placeholder or invented name. If no project name is visible in the log, use a short plain description of the activity as the project instead.
- The summary describes what was concretely done (debugging an error, editing specific files, researching a problem), based only on the log. If intent is unclear, describe the visible activity plainly rather than inventing a goal.
- Never include personal names, email addresses, usernames, or other personally identifying details. Describe the work, not the people.

Categories (sessions are anyone's time, not just programming):
- deep_work: focused building, coding, or substantial creation
- learning: research, reading, studying, following a course
- creative: design, art, music, video, or writing work
- meeting: calls, video meetings, live collaboration
- admin: email, planning, scheduling, errands, routine upkeep
- personal: life tasks like travel planning, shopping, health, finances
- other: only when nothing above fits
"""

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

        // Runtime-built schema: {project: String, category: enum, summary: String}
        let categorySchema = DynamicGenerationSchema(
            name: "category",
            description: "The kind of session",
            anyOf: ["deep_work", "learning", "creative", "meeting", "admin", "personal", "other"]
        )
        let root = DynamicGenerationSchema(
            name: "SessionSummary",
            properties: [
                .init(
                    name: "project",
                    description: "A short 2-6 word name for what was worked on, taken from project/file/document names in the log",
                    schema: DynamicGenerationSchema(type: String.self)
                ),
                .init(name: "category", schema: categorySchema),
                .init(
                    name: "summary",
                    description: "One or two plain sentences describing what was concretely done, like a changelog entry",
                    schema: DynamicGenerationSchema(type: String.self)
                ),
            ]
        )

        do {
            let schema = try GenerationSchema(root: root, dependencies: [])
            let session = LanguageModelSession(instructions: INSTRUCTIONS)
            let response = try await session.respond(
                to: "Summarize this work session:\n\n\(activity)",
                schema: schema,
                options: GenerationOptions(temperature: 0.1)
            )
            let content = response.content
            let out: [String: String] = [
                "project": (try? content.value(String.self, forProperty: "project")) ?? "",
                "category": (try? content.value(String.self, forProperty: "category")) ?? "",
                "summary": (try? content.value(String.self, forProperty: "summary")) ?? "",
            ]
            let data = try JSONSerialization.data(withJSONObject: out)
            FileHandle.standardOutput.write(data)
        } catch {
            fail("generation failed: \(error)")
        }
    }
}
