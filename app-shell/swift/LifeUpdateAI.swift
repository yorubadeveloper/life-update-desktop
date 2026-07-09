// The one non-Rust piece: the bridge to Apple Intelligence.
//
// The FoundationModels framework (macOS 26+) is Swift-only, so the Rust
// core shells out to this helper:
//   check      - reports Apple Intelligence availability
//   summarize  - stdin: a document with optional [RECENT SESSIONS] memory
//                and the [CURRENT SESSION LOG]; stdout: JSON
//                {project, category, summary}. Runs a grounding self-check
//                and regenerates once if the first draft makes claims the
//                log doesn't support.
//   condense   - stdin: an over-long log fragment; stdout: a few bullet
//                lines. Used map-reduce style by the Rust side so long
//                sessions don't silently truncate at the context window.
//
// Guided generation uses a *runtime* schema (DynamicGenerationSchema), not
// the @Generable macro - the macro's plugin ships only with full Xcode,
// while the dynamic API works with Command Line Tools alone. Output shape
// is structurally guaranteed; category is a closed enum; temperature stays
// low because summaries should be boring and factual.
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

The input may contain a [RECENT SESSIONS] section (this user's previous summarized sessions) followed by the [CURRENT SESSION LOG]. Summarize ONLY the current session log; the recent sessions exist for continuity.

Rules:
- The apps in the log (terminals, editors, browsers) are tools the user was using. The user does not build or work for those apps. Never describe the session as improving or working on one of those apps.
- Window titles usually contain the real project, file, or document name. Take the project name from there, never from a tool or app name.
- Continuity: if the current session continues work from a recent session, REUSE that session's exact project name, and let the summary reflect the continuation (returning to, continuing, finishing something started earlier) when the log supports it.
- Recent sessions may contain mistakes. Never copy their wording or claims - reuse only a project name, and only when the current log genuinely continues that work. Every other rule applies to recent sessions' names too (a tool or app name is never a project, even if a recent session used one).
- Never output a placeholder or invented name. If no project name is visible anywhere, use a short plain description of the activity as the project instead.
- The summary describes what was concretely done, based only on the current log. If intent is unclear, describe the visible activity plainly rather than inventing a goal.
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

struct Summary {
    var project: String
    var category: String
    var summary: String
}

func makeSchema() throws -> GenerationSchema {
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
                description: "A short 2-6 word name for what was worked on, taken from project/file/document names in the log (reuse a recent session's project name when continuing it)",
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
    return try GenerationSchema(root: root, dependencies: [])
}

func generate(document: String, extraCaution: String?) async throws -> Summary {
    let schema = try makeSchema()
    let session = LanguageModelSession(instructions: INSTRUCTIONS)
    var prompt = "Summarize this work session:\n\n\(document)"
    if let caution = extraCaution {
        prompt += "\n\nIMPORTANT: A previous attempt was rejected for this reason: \(caution)\nOnly state what the log explicitly shows."
    }
    let response = try await session.respond(
        to: prompt,
        schema: schema,
        options: GenerationOptions(temperature: 0.1)
    )
    let content = response.content
    return Summary(
        project: (try? content.value(String.self, forProperty: "project")) ?? "",
        category: (try? content.value(String.self, forProperty: "category")) ?? "",
        summary: (try? content.value(String.self, forProperty: "summary")) ?? ""
    )
}

/// Grounding self-check: does the draft claim anything the log doesn't
/// support? Returns nil when grounded, else the problem description.
func groundingProblem(document: String, draft: Summary) async -> String? {
    do {
        let verdictSchema = DynamicGenerationSchema(
            name: "verdict",
            description: "Whether the summary is fully supported by the log",
            anyOf: ["supported", "unsupported"]
        )
        let root = DynamicGenerationSchema(
            name: "GroundingCheck",
            properties: [
                .init(name: "verdict", schema: verdictSchema),
                .init(
                    name: "problem",
                    description: "If unsupported: the specific claim that the log does not support, in one short sentence. If supported: an empty string.",
                    schema: DynamicGenerationSchema(type: String.self)
                ),
            ]
        )
        let schema = try GenerationSchema(root: root, dependencies: [])
        let session = LanguageModelSession(
            instructions: "You are a strict fact checker. Given an activity log and a proposed summary of it, decide whether every claim in the summary is supported by the log. Mentioning apps, files, project names, or activities that appear in the log is supported; invented outcomes, invented names, or activities not in the log are unsupported."
        )
        let response = try await session.respond(
            to: "Activity log:\n\(document)\n\nProposed summary:\nproject: \(draft.project)\nsummary: \(draft.summary)",
            schema: schema,
            options: GenerationOptions(temperature: 0.0)
        )
        let verdict = (try? response.content.value(String.self, forProperty: "verdict")) ?? "supported"
        if verdict == "unsupported" {
            let problem = (try? response.content.value(String.self, forProperty: "problem")) ?? "unsupported claim"
            return problem.isEmpty ? "unsupported claim" : problem
        }
        return nil
    } catch {
        return nil // checker failure must never block a summary
    }
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

        let input = FileHandle.standardInput.readDataToEndOfFile()
        guard let document = String(data: input, encoding: .utf8),
              !document.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            fail("no input on stdin")
        }

        switch command {
        case "condense":
            do {
                let session = LanguageModelSession(
                    instructions: "You condense a fragment of an activity log. Output 3-6 short bullet lines of what was concretely done, keeping every project, file, and document name that appears. No commentary."
                )
                let response = try await session.respond(
                    to: "Condense this log fragment:\n\n\(document)",
                    options: GenerationOptions(temperature: 0.1)
                )
                print(response.content)
            } catch {
                fail("condense failed: \(error)")
            }

        case "summarize":
            do {
                var draft = try await generate(document: document, extraCaution: nil)
                if let problem = await groundingProblem(document: document, draft: draft) {
                    // One retry with the checker's objection folded in; the
                    // second draft is accepted either way (never block).
                    draft = try await generate(document: document, extraCaution: problem)
                }
                let out: [String: String] = [
                    "project": draft.project,
                    "category": draft.category,
                    "summary": draft.summary,
                ]
                let data = try JSONSerialization.data(withJSONObject: out)
                FileHandle.standardOutput.write(data)
            } catch {
                fail("generation failed: \(error)")
            }

        default:
            fail("unknown command: \(command)")
        }
    }
}
