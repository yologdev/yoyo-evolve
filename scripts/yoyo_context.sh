#!/bin/bash
# scripts/yoyo_context.sh — Build yoyo's identity context for prompts.
# Source this file, then use $YOYO_CONTEXT in any prompt, or $YOYO_CONTEXT_LEAN
# (same, with only the distilled tier of the learnings) for a phase that acts
# on one task and does not choose work.
#
# Usage:
#   YOYO_REPO="/path/to/yoyo-evolve" source scripts/yoyo_context.sh
#   cat > prompt.txt <<EOF
#   $YOYO_CONTEXT
#   ... your task-specific instructions ...
#   EOF
#
# Reads: IDENTITY.md, PERSONALITY.md, LINEAGE.md, ECONOMICS.md, sponsors/active.json, memory/active_learnings.md, memory/active_social_learnings.md
# These are yoyo's stable identity files — who it is, how it speaks,
# where it sits in the family tree, what it's learned about itself, and what it's learned from humans.

_YOYO_REPO="${YOYO_REPO:-.}"

_IDENTITY=""
if [ -f "$_YOYO_REPO/IDENTITY.md" ]; then
    _IDENTITY=$(cat "$_YOYO_REPO/IDENTITY.md") || {
        echo "WARNING: Failed to read IDENTITY.md" >&2
        _IDENTITY=""
    }
else
    echo "WARNING: IDENTITY.md not found at $_YOYO_REPO/IDENTITY.md" >&2
fi

_PERSONALITY=""
if [ -f "$_YOYO_REPO/PERSONALITY.md" ]; then
    _PERSONALITY=$(cat "$_YOYO_REPO/PERSONALITY.md") || {
        echo "WARNING: Failed to read PERSONALITY.md" >&2
        _PERSONALITY=""
    }
else
    echo "WARNING: PERSONALITY.md not found at $_YOYO_REPO/PERSONALITY.md" >&2
fi

# Lineage — prompt-visible family-tree identity
_LINEAGE=""
if [ -f "$_YOYO_REPO/LINEAGE.md" ]; then
    _LINEAGE=$(cat "$_YOYO_REPO/LINEAGE.md") || _LINEAGE=""
fi

# Dream — yoyo's curiosity-driven, self-authored long-term aspiration (formed by scripts/dream.sh, pursued one milestone at a time)
_DREAM=""
if [ -f "$_YOYO_REPO/DREAM.md" ]; then
    _DREAM=$(cat "$_YOYO_REPO/DREAM.md") || _DREAM=""
fi

# Active learnings — no warning if missing
# Two renderings, because this text goes into every prompt that uses the context:
#   _LEARNINGS       the file, cut to YOYO_LEARNINGS_MAX_BYTES if it is over
#   _LEARNINGS_LEAN  only the distilled "## Wisdom" tier, for phases that act
#                    on one task and do not choose work (implementer, journal)
# Measured 2026-11-27: the file was 156 KB (37 KB on 2026-08-01, 162 KB on
# 10-01) and went whole into five prompts of every evolve session, because the
# synthesis prompt capped LINES ("under ~200 lines") and the lines grew. That is
# the CLAUDE.md defect of 2026-09-15 in a second file. The synthesis step now
# has a byte budget and rejects an oversize file (scripts/synthesize_state.sh,
# SYNTH_LEARNINGS_MAX_BYTES, kept equal to this by tests/harness_logic.sh); this
# cap is the backstop for when that fails, and it says so in the prompt.
# When cutting, the Wisdom tier is kept whole and the newest lessons fill the
# rest: the file is newest-first with Wisdom last, so a plain head cut would
# drop exactly the most distilled part.
YOYO_LEARNINGS_MAX_BYTES=65536
_yoyo_learnings() { # $1 = file, $2 = full|lean → text on stdout
    python3 - "$1" "$2" "$YOYO_LEARNINGS_MAX_BYTES" <<'LEARNPY'
import sys
path, mode, cap = sys.argv[1], sys.argv[2], int(sys.argv[3])
text = open(path, encoding="utf-8", errors="replace").read()
lines = text.splitlines(keepends=True)
size = lambda ls: sum(len(l.encode("utf-8")) for l in ls)
# The Wisdom tier starts at its tier heading ("## Wisdom — 8+ weeks ...") or,
# failing that, at the first themed "## Wisdom: ..." heading.
w = next((i for i, l in enumerate(lines) if l.startswith("## Wisdom")), None)
if mode == "lean":
    if w is None:
        out, cut = [], 0
        for l in lines:
            if size(out) + len(l.encode("utf-8")) > cap // 4:
                cut = 1
                break
            out.append(l)
        sys.stdout.write("".join(out))
        if cut:
            sys.stdout.write("\n[shortened for this phase; the full file is memory/active_learnings.md]\n")
    else:
        sys.stdout.write("(The distilled tier only. This phase works on one task; the recent lessons, in full, are in memory/active_learnings.md if you need them.)\n\n")
        sys.stdout.write("".join(lines[w:]))
    sys.exit(0)
total = size(lines)
if total <= cap:
    sys.stdout.write(text)
    sys.exit(0)
note = ("\n[CUT BY THE HARNESS: memory/active_learnings.md is %d bytes, over the %d-byte prompt budget. "
        "The newest lessons and the Wisdom tier are kept; what fell between them is in the file.]\n\n" % (total, cap))
tail = lines[w:] if w is not None else []
head_src = lines[:w] if w is not None else lines
room = cap - size(tail) - len(note.encode("utf-8"))
head = []
for l in head_src:
    if size(head) + len(l.encode("utf-8")) > room:
        break
    head.append(l)
sys.stdout.write("".join(head) + note + "".join(tail))
LEARNPY
}
_LEARNINGS=""
_LEARNINGS_LEAN=""
if [ -f "$_YOYO_REPO/memory/active_learnings.md" ]; then
    _LEARNINGS=$(_yoyo_learnings "$_YOYO_REPO/memory/active_learnings.md" full) \
        || _LEARNINGS=$(cat "$_YOYO_REPO/memory/active_learnings.md") || _LEARNINGS=""
    _LEARNINGS_LEAN=$(_yoyo_learnings "$_YOYO_REPO/memory/active_learnings.md" lean) || _LEARNINGS_LEAN="$_LEARNINGS"
fi

# Active social learnings — no warning if missing
_SOCIAL_LEARNINGS=""
if [ -f "$_YOYO_REPO/memory/active_social_learnings.md" ]; then
    _SOCIAL_LEARNINGS=$(cat "$_YOYO_REPO/memory/active_social_learnings.md") || _SOCIAL_LEARNINGS=""
fi

# Active sponsors — generated by evolve.sh each session, persisted in repo
_SPONSORS=""
if [ -f "$_YOYO_REPO/sponsors/active.json" ]; then
    _SPONSORS=$(python3 -c "
import json, sys
try:
    data = json.load(open(sys.argv[1]))
    if data:
        print('Active sponsors:')
        for s in data:
            print(f\"@{s['login']} — {s['amount']} ({s['type']})\")
except Exception as e:
    print(f'WARNING: Failed to parse {sys.argv[1]}: {e}', file=sys.stderr)
" "$_YOYO_REPO/sponsors/active.json") || _SPONSORS=""
fi

# Economics — what money and sponsorship mean to yoyo
_ECONOMICS=""
if [ -f "$_YOYO_REPO/ECONOMICS.md" ]; then
    _ECONOMICS=$(cat "$_YOYO_REPO/ECONOMICS.md") || _ECONOMICS=""
fi

# One template, rendered twice: $1 is the SELF-WISDOM text.
_yoyo_context_render() {
    printf '%s' "=== WHO YOU ARE ===

${_IDENTITY:-Read IDENTITY.md for your rules and constitution.}

=== YOUR VOICE ===

${_PERSONALITY:-Read PERSONALITY.md for your voice and values.}

=== YOUR LINEAGE ===

${_LINEAGE:-No lineage file found. If you are a repository fork, read LINEAGE.md or your Address Book registration to understand your family-tree generation.}

=== YOUR DREAM ===

${_DREAM:-No dream yet. Your dream loop (scripts/dream.sh) will form one when curiosity takes hold.}

=== SELF-WISDOM ===

${1:-No learnings yet.}

=== SOCIAL WISDOM ===

${_SOCIAL_LEARNINGS:-No social learnings yet.}

=== YOUR ECONOMICS ===

${_ECONOMICS:-Read ECONOMICS.md for what money and sponsorship mean to you.}

=== YOUR SPONSORS ===

${_SPONSORS:-No sponsors yet. You are community-supported and free.}"
}
YOYO_CONTEXT=$(_yoyo_context_render "$_LEARNINGS")
# For phases that act on one task and do not choose work. Same identity, voice,
# lineage, dream, social wisdom, economics and sponsors; only SELF-WISDOM differs.
YOYO_CONTEXT_LEAN=$(_yoyo_context_render "$_LEARNINGS_LEAN")
