#!/usr/bin/env bash
# Drives headless Claude Code through PLAN.md until the plan is complete or a
# deadline arrives, sleeping through usage-limit resets along the way.
#
#   caffeinate -i scripts/supervise.sh                 # hard stop: next Saturday 17:00 local
#   DEADLINE="2026-10-17 17:00" scripts/supervise.sh   # explicit hard stop
#   SKIP_PERMISSIONS=1 scripts/supervise.sh            # --dangerously-skip-permissions instead of the allowlist
#
# One conversation is resumed across runs; its id is kept in .supervisor/
# (gitignored) with one JSON result per run and supervise.log. Delete
# .supervisor/session_id to start a fresh conversation. Ctrl-C stops cleanly
# after the current run; whatever is on disk is committed and pushed.
set -uo pipefail
cd "$(dirname "$0")/.."

STATE=.supervisor
mkdir -p "$STATE"
LOG="$STATE/supervise.log"
BRANCH="${BRANCH:-yq-core/milestone-1}"
MARGIN_MIN="${MARGIN_MIN:-45}"   # no new run starts closer to the deadline than this
DEADLINE_EPOCH=$(python3 scripts/supervise-time.py deadline "${DEADLINE:-}") || exit 1
DEADLINE_HUMAN=$(date -r "$DEADLINE_EPOCH" '+%a %Y-%m-%d %H:%M %Z')

log() {
  printf '%s %s\n' "$(date '+%F %T')" "$*" | tee -a "$LOG"
}

finish() {
  if [ -n "$(git status --porcelain)" ]; then
    log "committing uncommitted work on $BRANCH"
    git add -A
    git commit -q -m "WIP: supervisor stop at $(date '+%F %T')" || true
  fi
  git push -q -u origin "$BRANCH" 2>>"$LOG" && log "pushed $BRANCH" || log "push failed; see $LOG"
}
trap 'log "interrupted"; finish; exit 130' INT TERM

git fetch -q origin
if git show-ref -q "refs/remotes/origin/$BRANCH"; then
  git checkout -q "$BRANCH" && git pull -q --rebase origin "$BRANCH"
else
  git checkout -q -B "$BRANCH" origin/main
fi
log "supervising $BRANCH until $DEADLINE_HUMAN"

ALLOWED=(
  Read Edit Write Glob Grep Agent TodoWrite
  "WebFetch(domain:docs.rs)" "WebFetch(domain:github.com)" "WebFetch(domain:raw.githubusercontent.com)"
  "Bash(cargo *)" "Bash(rustup *)" "Bash(rustc *)" "Bash(git *)" "Bash(gh *)"
  "Bash(scripts/*)" "Bash(./scripts/*)" "Bash(node *)" "Bash(deno *)" "Bash(bun *)"
  "Bash(python3 *)" "Bash(yq *)" "Bash(wasm-opt *)" "Bash(cd *)" "Bash(ls *)" "Bash(cat *)"
  "Bash(head *)" "Bash(tail *)" "Bash(wc *)" "Bash(grep *)" "Bash(rg *)" "Bash(find *)"
  "Bash(sed *)" "Bash(awk *)" "Bash(sort *)" "Bash(uniq *)" "Bash(diff *)" "Bash(cut *)"
  "Bash(tr *)" "Bash(echo *)" "Bash(printf *)" "Bash(mkdir *)" "Bash(cp *)" "Bash(mv *)"
  "Bash(rm *)" "Bash(gzip *)" "Bash(date *)" "Bash(timeout *)" "Bash(test *)" "Bash(which *)"
  "Bash(command *)" "Bash(true)" "Bash(xxd *)" "Bash(stat *)" "Bash(touch *)"
)
DISALLOWED=(
  "Bash(git push --force*)" "Bash(git push -f*)" "Bash(git reset --hard*)" "Bash(git clean*)"
  "Bash(rm -rf *)" "Bash(rm -fr *)"
)

brief() {
  cat <<'PROMPT'
Read AGENTS.md, PLAN.md, docs/design.md and tests/fixtures/README.md first. This repo is a Rust port of mikefarah/yq (pkg/yqlib) targeting a small wasm module. Execute PLAN.md milestone by milestone, starting with milestone 1 (node model, YAML and JSON codecs, printer, and the conformance runner in crates/yq-core/src/conformance.rs so scenarios can actually be evaluated), then milestone 2 (lexer, postfix, parser, core operators) and onward. Run scripts/fetch-reference.sh to get the Go source into ref/yq and port from it; the fixture is the spec.

Rules that matter most:
- Measure with `cargo test --test conformance` and read target/conformance-report.txt. Raise crates/yq-core/tests/conformance_baseline.txt in the same commit that makes more scenarios pass. Never lower it.
- Commit after every operator group that goes green, naming the group and the new count. Push the branch after every few commits so progress is never lost.
- Keep crates/yq-core free of std::env, std::fs, std::time, std::process. Verify the wasm with scripts/build-wasm.sh and `deno run --allow-read js/smoke.ts`.
- No braceless if statements. Comments only for surprising behavior. cargo fmt and `cargo clippy --all-targets -- -D warnings` must pass before each commit.
- Update PLAN.md "Decisions and findings" whenever you learn something that changes the approach, and keep its conformance count current.
- You are running unattended with a permission allowlist. If a command is denied, find another way rather than retrying it.
PROMPT
}

protocol() {
  local remaining_min=$1
  cat <<PROMPT

Time budget: the hard stop is $DEADLINE_HUMAN, about ${remaining_min} minutes from now (check with \`date\`). When fewer than 20 minutes remain, stop starting new work: commit, update PLAN.md with the current conformance count and the next step, push, and end your turn.

End every turn with exactly one of these lines so the supervisor knows what to do:
YQ_RS_STATUS: CONTINUE   (more work remains; you will be resumed)
YQ_RS_STATUS: DONE       (every milestone in PLAN.md is complete and pushed)
PROMPT
}

run=$(ls "$STATE"/run-*.json 2>/dev/null | wc -l | tr -d ' ')
errors=0
while :; do
  now=$(date +%s)
  remaining=$((DEADLINE_EPOCH - now))
  if ((remaining < MARGIN_MIN * 60)); then
    log "within ${MARGIN_MIN}m of the deadline; not starting another run"
    break
  fi
  run=$((run + 1))
  out="$STATE/run-$run.json"
  err="$STATE/run-$run.stderr"

  if [ -s "$STATE/session_id" ]; then
    prompt="Continue working through PLAN.md from where you left off. Re-read PLAN.md and target/conformance-report.txt first.$(protocol $((remaining / 60)))"
    resume=(--resume "$(cat "$STATE/session_id")")
  else
    prompt="$(brief)$(protocol $((remaining / 60)))"
    resume=()
  fi

  perms=(--permission-mode dontAsk --allowedTools "${ALLOWED[@]}" --disallowedTools "${DISALLOWED[@]}")
  if [ "${SKIP_PERMISSIONS:-}" = "1" ]; then
    perms=(--dangerously-skip-permissions)
  fi

  log "run $run starting (${remaining}s to deadline)"
  if command -v timeout >/dev/null 2>&1; then
    CLAUDE_CODE_RETRY_WATCHDOG=1 timeout -s TERM "$remaining" \
      claude -p "$prompt" --output-format json ${perms[@]+"${perms[@]}"} ${resume[@]+"${resume[@]}"} >"$out" 2>"$err"
  else
    CLAUDE_CODE_RETRY_WATCHDOG=1 \
      claude -p "$prompt" --output-format json ${perms[@]+"${perms[@]}"} ${resume[@]+"${resume[@]}"} >"$out" 2>"$err"
  fi
  code=$?

  if ! jq -e . "$out" >/dev/null 2>&1; then
    errors=$((errors + 1))
    log "run $run produced no JSON (exit $code); stderr: $(head -c 300 "$err" | tr '\n' ' ')"
    if ((errors >= 8)); then
      log "too many consecutive failures; stopping"
      break
    fi
    sleep 120
    continue
  fi

  session_id=$(jq -r '.session_id // empty' "$out")
  if [ -n "$session_id" ]; then
    printf '%s' "$session_id" >"$STATE/session_id"
  fi
  is_error=$(jq -r '.is_error // false' "$out")
  result=$(jq -r '.result // ""' "$out")
  cost=$(jq -r '.total_cost_usd // "?"' "$out")

  if [ "$is_error" = "true" ]; then
    if printf '%s' "$result" | grep -qiE "hit your .*limit|usage limit|rate limit|too many requests"; then
      nap=$(python3 scripts/supervise-time.py reset-sleep "$result")
      if ((now + nap > DEADLINE_EPOCH)); then
        log "limit reached and the reset is past the deadline: $(printf '%s' "$result" | head -c 200)"
        break
      fi
      log "limit reached; sleeping $((nap / 60))m: $(printf '%s' "$result" | head -c 200)"
      sleep "$nap"
      continue
    fi
    if printf '%s' "$result" | grep -qiE "authenticat|login|oauth"; then
      log "authentication problem, needs a human: $result"
      break
    fi
    errors=$((errors + 1))
    log "run $run error (exit $code): $(printf '%s' "$result" | head -c 300)"
    if ((errors >= 8)); then
      log "too many consecutive failures; stopping"
      break
    fi
    sleep 120
    continue
  fi

  errors=0
  log "run $run finished (exit $code, est. \$$cost): $(printf '%s' "$result" | tail -n 3 | tr '\n' ' ' | head -c 300)"
  if printf '%s' "$result" | grep -q "YQ_RS_STATUS: DONE"; then
    log "plan complete"
    break
  fi
  sleep 5
done

finish
log "supervisor exiting"
