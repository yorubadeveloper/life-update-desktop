#!/usr/bin/env bash
# Summary-quality regression suite: runs every fixture in evals/ through
# the real Apple Intelligence helper and checks project/category against
# evals/expected.json. Run after ANY prompt or schema change - this is how
# placeholder-name and wrong-category regressions get caught before users
# do. Requires Apple Intelligence available on this machine.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HELPER="$SCRIPT_DIR/../src-tauri/resources/ai-helper/life-update-ai"
EVALS="$SCRIPT_DIR/../evals"
[ -x "$HELPER" ] || { echo "build the helper first: ./scripts/build-ai-helper.sh"; exit 1; }

pass=0; failures=0
for f in "$EVALS"/*.txt; do
  name=$(basename "$f" .txt)
  out=$("$HELPER" summarize < "$f")
  result=$(python3 - "$name" "$out" "$EVALS/expected.json" <<'PY'
import json, re, sys
name, raw, expected_path = sys.argv[1], sys.argv[2], sys.argv[3]
got = json.loads(raw)
exp = json.load(open(expected_path))[name]
problems = []
if got["category"] not in exp["categories"]:
    problems.append(f"category {got['category']!r} not in {exp['categories']}")
if not re.search(exp["project_matches"], got["project"], re.I):
    problems.append(f"project {got['project']!r} !~ /{exp['project_matches']}/")
if re.search(exp["project_rejects"], got["project"], re.I):
    problems.append(f"project {got['project']!r} matches forbidden /{exp['project_rejects']}/")
print("PASS" if not problems else "FAIL: " + "; ".join(problems) + f" | got: {raw}")
PY
)
  if [[ "$result" == PASS ]]; then
    echo "✓ $name"; pass=$((pass+1))
  else
    echo "✗ $name - $result"; failures=$((failures+1))
  fi
done
echo "---"
echo "$pass passed, $failures failed"
[ "$failures" -eq 0 ]
