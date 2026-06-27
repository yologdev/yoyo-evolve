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
AGENT_ID="${YOYO_TOKEN%%.*}"     # the token text before the dot, e.g. alice--yoyo
VAULT="$YOPEDIA_VAULT_ID"        # which vault to file into — passed on every ingest
```

If `$YOYO_TOKEN` or `$YOPEDIA_VAULT_ID` is unset, your second brain isn't wired up
yet — **skip ingestion/recall silently and carry on**. A yopedia call must never
fail your actual work.

## Recall FIRST (no token needed)

Before researching a topic, check what you already know and build on it:

```bash
# Ask a question scoped to your own knowledge:
curl -sS -X POST "$BASE/api/query" -H "Content-Type: application/json" \
  -d "{\"question\":\"What have I learned about <topic>?\",\"scope\":\"agent:$AGENT_ID\"}"

# Keyword search your knowledge:
curl -sS "$BASE/api/wiki/search?q=<topic>&scope=agent:$AGENT_ID"

# Bootstrap your whole accumulated context:
curl -sS "$BASE/api/agents/$AGENT_ID/context"
```

## Ingest a source (whenever something genuinely informs you)

From a URL (no quoting worries — inline is fine):

```bash
curl -sS -X POST "$BASE/api/agents/$AGENT_ID/ingest" \
  -H "Authorization: Bearer $YOYO_TOKEN" -H "Content-Type: application/json" \
  -d "{\"url\":\"https://example.com/paper\",\"vaultId\":\"$VAULT\"}"
# → {"slug":"...","deduped":false}
```

From your own notes — **build the JSON with python3** (text has quotes/newlines;
hand-concatenated JSON breaks → 400):

```bash
python3 -c "import json,os,datetime; print(json.dumps({
  'title': 'Proprioception in software systems — ' + datetime.date.today().isoformat(),
  'text':  'What it said, why it matters, and the source URL(s).',
  'vaultId': os.environ['YOPEDIA_VAULT_ID']}))" \
| curl -sS -X POST "$BASE/api/agents/$AGENT_ID/ingest" \
    -H "Authorization: Bearer $YOYO_TOKEN" -H "Content-Type: application/json" --data @-
```

## Ingest a report (a synthesis worth keeping)

Same python3→curl pattern, with a dated report title:

```bash
python3 -c "import json,os,datetime; print(json.dumps({
  'title': 'Dream research — <topic> (' + datetime.date.today().isoformat() + ')',
  'text':  '<what I explored / key findings / open questions / sources>',
  'vaultId': os.environ['YOPEDIA_VAULT_ID']}))" \
| curl -sS -X POST "$BASE/api/agents/$AGENT_ID/ingest" \
    -H "Authorization: Bearer $YOYO_TOKEN" -H "Content-Type: application/json" --data @-
```

## Rules

- **Recall before you research; ingest after you learn.** That's the loop.
- Give every note a clear, **dated title** — recall and dedup work better.
- Re-posting the same URL/text is safe: it **dedups** (`"deduped":true`) instead of duplicating.
- Always build text/report bodies with `python3 -c` (quotes/newlines break naive strings).
- Your notes are **private agent-knowledge**; the vault is your organizing lens.
- **Never fail your task over yopedia.** If a key is unset or a call errors, log it and move on.
  Responses: 200 ok · 401 bad/missing token · 403 token is for another agent · 400 no url/text/vault · 500 retry.
