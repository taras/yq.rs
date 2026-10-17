#!/usr/bin/env bash
# Follows the supervisor's live Claude Code transcript as readable lines:
# assistant text, tool calls, and the first line of each tool result.
#   scripts/watch.sh
set -uo pipefail
cd "$(dirname "$0")/.."
project_dir="$HOME/.claude/projects/$(pwd | sed 's#/#-#g; s#\.#-#g')"
transcript=$(ls -t "$project_dir"/*.jsonl 2>/dev/null | head -1)
if [ -z "$transcript" ]; then
  echo "no transcript yet under $project_dir"
  exit 1
fi
echo "following $transcript"
tail -n 50 -f "$transcript" | python3 -u -c '
import json, sys
for line in sys.stdin:
    try:
        event = json.loads(line)
    except ValueError:
        continue
    kind = event.get("type")
    content = event.get("message", {}).get("content")
    if not isinstance(content, list):
        continue
    for block in content:
        block_type = block.get("type")
        if kind == "assistant" and block_type == "text":
            print("\033[1m" + block["text"].strip() + "\033[0m")
        elif kind == "assistant" and block_type == "tool_use":
            args = block.get("input", {})
            detail = args.get("command") or args.get("file_path") or args.get("description") or json.dumps(args)[:160]
            print(f"\033[36m> {block.get(chr(110)+chr(97)+chr(109)+chr(101))}\033[0m {str(detail)[:200]}")
        elif kind == "user" and block_type == "tool_result":
            result = block.get("content")
            if isinstance(result, list):
                result = " ".join(part.get("text", "") for part in result if isinstance(part, dict))
            first = str(result or "").strip().splitlines()[:1]
            if first:
                print(f"  \033[2m{first[0][:200]}\033[0m")
'
