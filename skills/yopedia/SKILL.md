---
name: yopedia
description: Save research and reflections into (and recall from) your personal knowledge vault (yopedia) — your second brain
tools: [bash]
core: false
origin: creator
keywords: ["yopedia", "ingest", "recall", "second brain", "knowledge vault", "bearer token"]
---

# Yopedia — your second brain

Yopedia (https://yopedia.yolog.dev) is your personal, persistent knowledge vault.
Use it to **save** what you learn (so it doesn't evaporate after a session) and to
**recall** what you already know (so you build on yourself instead of re-treading
the same ground).

## Setup (from env)

```bash
BASE=https://yopedia.yolog.dev
AGENT_ID="${YOPEDIA_AGENT_TOKEN%%.*}"     # the token text before the dot, e.g. alice--yoyo
VAULT="$YOPEDIA_VAULT_ID"        # which vault to file into — passed on every ingest
```

If `$YOPEDIA_AGENT_TOKEN` or `$YOPEDIA_VAULT_ID` is unset, your second brain isn't wired up
yet — **skip ingestion/recall silently and carry on**. A yopedia call must never
fail your actual work.

## Recall FIRST

Before researching a topic, check what you already know and build on it. Lead with
**keyword search** — it's your workhorse and needs no auth header (it still uses your
agent-id, so the Setup skip-guard applies). Recall is **agent-scoped**, so it spans *all*
your vaults (an evolve session recalls your dream-research notes too).

```bash
# 1. Keyword search (no auth) — YOUR MAIN RECALL.
#    Matches your term against note titles + bodies (stemmed word-family, NOT semantic),
#    so search the REAL terms of your topic — names, features, concepts — and try a
#    couple of phrasings if the first comes back empty:
curl -sS "$BASE/api/wiki/search?q=<keyword>&scope=agent:$AGENT_ID"

# 2. Whole index (no auth) — scan every note title you have, to see what's there at all:
curl -sS "$BASE/api/agents/$AGENT_ID/context"

# 3. Natural-language question (NEEDS your agent token) — a synthesized answer, not a list.
#    /api/query is authenticated; send the same token you ingest with. (Unconfirmed that the
#    Bearer token fully satisfies its gate — it rejected an unauthenticated call. If #3 errors,
#    just fall back to #1/#2.) Use when you want a digested answer rather than raw matches:
curl -sS -X POST "$BASE/api/query" \
  -H "Authorization: Bearer $YOPEDIA_AGENT_TOKEN" -H "Content-Type: application/json" \
  -d "{\"question\":\"What have I learned about <topic>?\",\"scope\":\"agent:$AGENT_ID\"}"
```

## Ingest a source (whenever something genuinely informs you)

**Ingest is fire-and-forget**: the POST is queued and processed in the background, so it returns almost immediately. Don't wait for or parse a slug — just fire it and move on. (A non-2xx response means it failed; log it and skip.)

From a URL (no quoting worries — inline is fine):

```bash
curl -sS -X POST "$BASE/api/agents/$AGENT_ID/ingest" \
  -H "Authorization: Bearer $YOPEDIA_AGENT_TOKEN" -H "Content-Type: application/json" \
  -d "{\"url\":\"https://example.com/paper\",\"vaultId\":\"$VAULT\"}"
# → {queued: true, jobId} (queued; processed in the background — no slug to wait on)
```

From your own notes — **build the JSON with python3** (text has quotes/newlines;
hand-concatenated JSON breaks → 400):

```bash
python3 -c "import json,os,datetime; print(json.dumps({
  'title': 'Proprioception in software systems — ' + datetime.date.today().isoformat(),
  'text':  'What it said, why it matters, and the source URL(s).',
  'vaultId': os.environ['YOPEDIA_VAULT_ID']}))" \
| curl -sS -X POST "$BASE/api/agents/$AGENT_ID/ingest" \
    -H "Authorization: Bearer $YOPEDIA_AGENT_TOKEN" -H "Content-Type: application/json" --data @-
```

## Ingest a report (a synthesis worth keeping)

Same python3→curl pattern, with a dated report title:

```bash
python3 -c "import json,os,datetime; print(json.dumps({
  'title': 'Research — <topic> (' + datetime.date.today().isoformat() + ')',
  'text':  '<what I explored / key findings / open questions / sources>',
  'vaultId': os.environ['YOPEDIA_VAULT_ID']}))" \
| curl -sS -X POST "$BASE/api/agents/$AGENT_ID/ingest" \
    -H "Authorization: Bearer $YOPEDIA_AGENT_TOKEN" -H "Content-Type: application/json" --data @-
```

## Rules

- **Recall before you research; ingest after you learn.** That's the loop.
- **Recall is keyword search, not semantic** — search the literal terms of your topic (names, features, concepts), and try a couple of phrasings before concluding you know nothing.
- Give every note a clear, **dated title** — recall and dedup work better.
- Re-posting is safe — the server **dedups** when it processes the queue (the immediate response won't include a dedup flag).
- Ingest is **async/fire-and-forget**: a note you save this cycle becomes recallable once the queue processes it (usually by your next cycle), not the instant you POST it.
- Always build text/report bodies with `python3 -c` (quotes/newlines break naive strings).
- Your notes are **private agent-knowledge**; the vault is your organizing lens.
- **Never fail your task over yopedia.** If a key is unset or a call errors, log it and move on.
  Responses: 2xx → {queued:true, jobId} (queued for background processing) · 401 bad/missing token · 403 token is for another agent · 400 no url/text/vault · 500 retry.
