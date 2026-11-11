#!/usr/bin/env bash
# PreToolUse guard. Blocks `git commit` and `git push` when the staged diff holds something that
# looks like a credential. A real token was pasted into this project's chat once; this is why.
#
# The hook receives JSON on stdin. Exit 2 blocks the action and sends stderr to the agent.
set -uo pipefail

input="$(cat)"

# Only act on commands that publish work.
if ! printf '%s' "$input" | grep -qE 'git[[:space:]]+(commit|push)'; then
  exit 0
fi

root="$(git rev-parse --show-toplevel 2>/dev/null)" || exit 0
cd "$root" || exit 0

# Patterns: Devpost tokens, GitHub tokens, AWS access keys, private key blocks, generic secrets.
pattern='cfat_[A-Za-z0-9]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----|(api[_-]?key|secret|password)["'"'"' ]*[:=]["'"'"' ]*[A-Za-z0-9/+_-]{20,}'

hits="$(git diff --cached -U0 | grep -E '^\+' | grep -vE '^\+\+\+' | grep -E "$pattern" | head -5)"

if [[ -n "$hits" ]]; then
  {
    echo "Blocked: the staged changes look like they contain a credential."
    echo "$hits" | sed -E 's/(.{60}).*/\1.../'
    echo "Remove it, rotate the credential, and stage again."
  } >&2
  exit 2
fi

exit 0
