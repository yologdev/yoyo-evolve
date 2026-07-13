#!/usr/bin/env python3
"""Process sponsor data fetched from the GitHub Sponsors API.

This is the single source of truth for sponsor state. Reads the raw
GraphQL response from /tmp/sponsor_raw.json (written by the caller's
`gh api graphql` invocation) and updates:

  - sponsors/sponsor_info.json — THE single source of truth for sponsor state.
                                 Contains every sponsor (recurring + one-time) keyed
                                 by login, with computed benefits, first_seen,
                                 benefit_expires, run_used, and shouted_out flags.
                                 Both this script and evolve.sh read it; this script
                                 rebuilds it, evolve.sh only mutates run_used.
  - sponsors/active.json       — flat list of currently-active sponsors for display
                                 (derived from sponsor_info.json)
  - SPONSORS.md                — append-only sponsor wall
  - README.md                  — auto-maintained block between SPONSORS_START/END markers

Side effect: opens GitHub issues for newly-eligible shoutout sponsors
($10+ recurring or $10+ one-time) using `gh issue create`. Requires
`gh` to be authenticated with a token that has `issues: write`.

Stdout: exactly one line `<monthly_cents>|<true|false>` consumed by
callers that want a summary (currently nothing — kept for ad-hoc use).

Stderr: WARNING/ERROR lines.

Exit codes:
  0 — success
  2 — sponsor fetch failed: missing/empty/invalid /tmp/sponsor_raw.json,
      GraphQL errors, unexpected response shape, or truncated results
      (totalCount > len(nodes)). On exit 2 NO files are written, so a
      transient API failure cannot wipe the committed sponsor state.
  3 — sponsor_info.json is unreadable (corrupt JSON or I/O error).
      Refuses to overwrite with defaults because that would destroy
      sponsors' run_used / shouted_out flags.
  4 — SPONSORS.md is missing a required section header for a sponsor we
      need to add (e.g. "## 💎 Genesis ($1,000)"). Human must add the
      section before the refresh can proceed.
  5 — README.md is missing, or missing the SPONSORS_START/SPONSORS_END
      markers, or the markers are in the wrong order. This is the exact
      silent-drop class the refactor targets, so it is fatal by design.
  Other non-zero — unhandled exception during file writes (loud).
"""

import json
import os
import subprocess
import sys
from datetime import datetime, timedelta, timezone

RAW_JSON = "/tmp/sponsor_raw.json"
ACTIVE_FILE = "sponsors/active.json"
SPONSOR_INFO_FILE = "sponsors/sponsor_info.json"
# 90-day grace period: one-time sponsors stay in sponsor_info for this many
# days after first_seen, so we remember their run_used/shouted_out flags
# even after they stop appearing in the GitHub Sponsors API response.
GRACE_DAYS = 90
SPONSORS_MD = "SPONSORS.md"
README_MD = "README.md"

README_MARKER_START = "<!-- SPONSORS_START -->"
README_MARKER_END = "<!-- SPONSORS_END -->"

REPO = os.environ.get("REPO", "yologdev/yoyo-evolve")


class FetchFailed(Exception):
    """Raised when the sponsor query failed and no file writes should happen."""


def warn(msg):
    print(f"WARNING: {msg}", file=sys.stderr)


def err(msg):
    print(f"ERROR: {msg}", file=sys.stderr)


def load_raw_nodes(path):
    """Load sponsor nodes from the GraphQL response.

    Raises FetchFailed for any condition that means we don't have
    trustworthy sponsor data — caller must abort before touching
    committed files.
    """
    if not os.path.exists(path):
        raise FetchFailed(f"sponsor raw file missing: {path}")
    if os.path.getsize(path) == 0:
        raise FetchFailed(f"sponsor raw file is empty: {path} (gh likely failed before writing)")

    try:
        with open(path) as f:
            data = json.load(f)
    except json.JSONDecodeError as e:
        raise FetchFailed(f"sponsor raw file is not valid JSON: {e}")

    if not isinstance(data, dict):
        raise FetchFailed(f"sponsor raw file has unexpected top-level type: {type(data).__name__}")

    if data.get("errors"):
        msgs = "; ".join(str(e.get("message", e)) for e in data["errors"])
        raise FetchFailed(f"GraphQL errors: {msgs}")

    try:
        shipments = data["data"]["viewer"]["sponsorshipsAsMaintainer"]
        nodes = shipments["nodes"] or []
    except (KeyError, TypeError):
        raise FetchFailed("sponsor raw file has unexpected shape (no viewer.sponsorshipsAsMaintainer.nodes)")

    # Pagination guard. The query requests first:100; if totalCount exceeds
    # that we'd silently drop sponsors beyond the first page — the exact
    # silent-data-loss class this refactor exists to kill. Fail loudly
    # instead and force a human to add pagination support.
    total = shipments.get("totalCount")
    if isinstance(total, int) and total > len(nodes):
        raise FetchFailed(
            f"sponsor query truncated: totalCount={total} but only {len(nodes)} "
            f"nodes returned. Add pagination (endCursor/hasNextPage) to the "
            f"GraphQL query in .github/workflows/sponsors-refresh.yml."
        )

    return nodes


def recurring_benefits(monthly_cents):
    dollars = monthly_cents / 100
    b = []
    if dollars >= 5:
        b.append("priority")
    if dollars >= 10:
        b.append("shoutout")
    if dollars >= 25:
        b.append("sponsors_md")
    if dollars >= 50:
        b.append("readme")
    return b


def onetime_benefits(total_cents):
    dollars = total_cents / 100
    b = []
    if dollars >= 5:
        b.append("priority")
    if dollars >= 10:
        b.append("shoutout")
    if dollars >= 20:
        b.append("sponsors_md")
    if dollars >= 50:
        b.append("readme")
    if dollars >= 1000:
        b.append("genesis")
    return b


def split_nodes(nodes):
    """Split GraphQL nodes into recurring map and one-time list."""
    recurring = {}  # login -> monthly_cents
    onetime = []
    monthly_cents = 0

    for n in nodes:
        login = (n.get("sponsorEntity") or {}).get("login", "")
        if not login:
            continue
        cents = (n.get("tier") or {}).get("monthlyPriceInCents", 0)
        if n.get("isOneTimePayment", False):
            onetime.append({"login": login, "cents": cents})
        else:
            recurring[login] = cents
            monthly_cents += cents
    return recurring, onetime, monthly_cents


def load_json_or_default(path, default):
    """Load JSON from path. Missing file → default. Unreadable/corrupt → fatal.

    The "missing" case is fine (first run, fresh checkout). The "unreadable"
    case must NEVER silently overwrite the file with default data — that's how
    you destroy a sponsor's run_used flags.
    """
    if not os.path.exists(path):
        return default
    try:
        with open(path) as f:
            return json.load(f)
    except (json.JSONDecodeError, OSError) as e:
        err(f"refusing to overwrite unreadable file {path}: {e}")
        sys.exit(3)


def _compute_benefit_expires(total_cents, first_seen):
    """Compute benefit_expires for a one-time sponsor based on amount + first_seen.

    Returns the string to store in `benefit_expires`. Genesis ($1,000+) → "never".
    Everything else gets a rolling window from first_seen.
    """
    dollars = total_cents / 100
    try:
        fs_date = datetime.strptime(first_seen, "%Y-%m-%d")
    except (ValueError, TypeError):
        fs_date = datetime.now(timezone.utc)
    if dollars >= 1000:
        return "never"
    if dollars >= 50:
        return (fs_date + timedelta(days=60)).strftime("%Y-%m-%d")
    if dollars >= 10:
        return (fs_date + timedelta(days=30)).strftime("%Y-%m-%d")
    if dollars >= 5:
        return (fs_date + timedelta(days=14)).strftime("%Y-%m-%d")
    return ""


def _extract_onetime(entry):
    """Pull the one-time portion out of an existing sponsor_info entry.

    Handles both shapes: a top-level onetime entry, and a onetime nested
    under a recurring entry. Returns the onetime dict or None.
    """
    if not isinstance(entry, dict):
        return None
    if entry.get("type") == "onetime":
        return entry
    nested = entry.get("onetime")
    if isinstance(nested, dict):
        return nested
    return None


def build_sponsor_info(recurring, onetime_from_api, existing_state, today):
    """Merge live API data with on-disk state into a fresh sponsor_info dict.

    - `recurring` (dict login→monthly_cents) is authoritative: recurring
      sponsors not in the API response are dropped (sponsorship ended).
    - `onetime_from_api` seeds new one-time entries stamped first_seen=today.
    - `existing_state` preserves mutation fields (run_used, shouted_out,
      first_seen) for any login still within its grace window. One-time
      sponsors linger 90 days after first_seen even after they leave the
      API, so we remember whether they used their accelerated run.
    """
    sponsor_info = {}

    # --- Recurring entries ---
    for login, cents in recurring.items():
        existing = existing_state.get(login) or {}
        sponsor_info[login] = {
            "type": "recurring",
            "monthly_cents": cents,
            "benefits": recurring_benefits(cents),
            "first_seen": existing.get("first_seen") or today,
            "shouted_out": bool(existing.get("shouted_out", False)),
        }

    # --- One-time entries: gather prior state first, then overlay API ---
    # Start from every existing one-time entry (top-level or nested), so
    # sponsors within their grace window survive even if they disappear
    # from the API.
    onetime_state = {}
    for login, entry in existing_state.items():
        prev = _extract_onetime(entry)
        if prev is None:
            continue
        onetime_state[login] = {
            "total_cents": prev.get("total_cents", 0),
            "first_seen": prev.get("first_seen") or "",
            "benefit_expires": prev.get("benefit_expires") or "",
            "run_used": bool(prev.get("run_used", False)),
            "shouted_out": bool(prev.get("shouted_out", False)),
        }

    # Add/refresh API one-time sponsors. New entries get first_seen=today
    # and benefit_expires computed from the tier. Existing entries keep
    # their first_seen / benefit_expires (set once, never overwritten).
    for s in onetime_from_api:
        login = s["login"]
        cents = s["cents"]
        if login not in onetime_state:
            onetime_state[login] = {
                "total_cents": cents,
                "first_seen": today,
                "benefit_expires": "",
                "run_used": False,
                "shouted_out": False,
            }

    # Fill benefit_expires for any entry missing it (new entries, or
    # legacy ones that never had it computed). Never overwrite an
    # existing value — that would extend a sponsor's window retroactively.
    for login, info in onetime_state.items():
        if info.get("benefit_expires"):
            continue
        info["benefit_expires"] = _compute_benefit_expires(
            info.get("total_cents", 0),
            info.get("first_seen") or today,
        )

    # Sponsors are permanent: every one-time sponsor stays in sponsor_info
    # (and therefore in the README/SPONSORS.md listings) forever. The old
    # 90-day grace-window prune is deliberately gone — with a small roster,
    # recognition should not expire (creator decision, 2026-07-13). Only the
    # time-limited *benefits* (issue priority, shoutout window) still use
    # benefit_expires; listing does not.

    # --- Fold one-time entries into sponsor_info ---
    for login, info in onetime_state.items():
        total_cents = info.get("total_cents", 0)
        dollars = total_cents / 100
        benefit_expires = info.get("benefit_expires", "")
        active = True
        if benefit_expires and benefit_expires != "never" and benefit_expires < today:
            active = False
        benefits = onetime_benefits(total_cents) if (active and dollars >= 5) else []
        entry = {
            "type": "onetime",
            "total_cents": total_cents,
            "benefits": benefits,
            "first_seen": info.get("first_seen") or today,
            "benefit_expires": benefit_expires,
            "run_used": info["run_used"],
            "shouted_out": info["shouted_out"],
        }
        if login in sponsor_info:
            # Recurring takes precedence; nest the one-time entry under it
            sponsor_info[login]["onetime"] = entry
        else:
            sponsor_info[login] = entry

    return sponsor_info


def update_sponsors_md(sponsor_info, path=SPONSORS_MD):
    """Append-only update of SPONSORS.md. Returns True if file changed.

    Missing section header is fatal — silently dropping a sponsor is the
    exact bug class this refactor exists to eliminate.
    """
    if os.path.exists(path):
        with open(path) as f:
            existing = f.read()
    else:
        existing = ""

    def already_listed(login):
        return f"@{login}" in existing

    new_lines = {}  # section_header -> list of entry strings
    for login, info in sponsor_info.items():
        if already_listed(login):
            continue
        if info.get("type") == "recurring":
            dollars = info.get("monthly_cents", 0) // 100
            if dollars >= 50:
                section = "## 🦈 Patron ($50+/mo)"
                new_lines.setdefault(section, []).append(f"- @{login} — ${dollars}/mo")
            elif dollars >= 25:
                section = "## 🦑 Boost ($25+/mo)"
                new_lines.setdefault(section, []).append(f"- @{login} — ${dollars}/mo")
        else:
            dollars = info.get("total_cents", 0) // 100
            benefits = info.get("benefits", [])
            if "genesis" in benefits:
                section = "## 💎 Genesis ($1,000)"
                new_lines.setdefault(section, []).append(f"- @{login} — ${dollars:,}")
            elif dollars >= 50:
                section = "## 🚀 Rocket Fuel ($50+)"
                new_lines.setdefault(section, []).append(f"- @{login} — ${dollars}")
            elif "sponsors_md" in benefits:
                section = "## 🧬 Evolution Boost ($20+)"
                new_lines.setdefault(section, []).append(f"- @{login} — ${dollars}")

    if not new_lines:
        return False

    lines = existing.split("\n")
    missing_sections = []
    for section, entries in new_lines.items():
        try:
            idx = lines.index(section)
            for entry in reversed(entries):
                lines.insert(idx + 1, entry)
        except ValueError:
            missing_sections.append((section, len(entries)))

    if missing_sections:
        for section, n in missing_sections:
            err(f"section '{section}' not found in {path} — {n} sponsor(s) cannot be added")
        sys.exit(4)

    _atomic_write_text(path, "\n".join(lines))
    print(f"  Updated {path}.")
    return True


def render_readme_block(sponsor_info):
    """Render the auto-maintained sponsors block for README.md.

    EVERY sponsor appears here, permanently — Genesis in its own section,
    everyone else (any amount, recurring or one-time) below it. Listing
    ignores `benefits`/`benefit_expires` entirely; those only govern the
    time-limited perks (issue priority, shoutout window).
    Returns the full block including START/END markers.
    """
    genesis = []
    patrons = []  # everyone who isn't Genesis, any amount

    for login, info in sponsor_info.items():
        benefits = info.get("benefits", [])
        if "genesis" in benefits:
            dollars = info.get("total_cents", 0) // 100
            genesis.append((login, f"${dollars:,}"))
        elif info.get("type") == "recurring":
            dollars = info.get("monthly_cents", 0) // 100
            patrons.append((login, f"${dollars}/mo"))
        else:
            dollars = info.get("total_cents", 0) // 100
            patrons.append((login, f"${dollars}"))

    def avatar_tag(login, amount, size):
        # Raw HTML so we can control pixel size; markdown image syntax can't.
        return (
            f'<a href="https://github.com/{login}" title="@{login} — {amount}">'
            f'<img src="https://github.com/{login}.png?size={size * 2}" '
            f'width="{size}" height="{size}" alt="@{login}" />'
            f'</a>'
        )

    lines = [README_MARKER_START]
    lines.append("<!-- This block is auto-maintained by scripts/refresh_sponsors.py — do not edit by hand. -->")
    lines.append("")

    if not genesis and not patrons:
        lines.append("_No sponsors yet. Be the first — [sponsor yoyo](https://github.com/sponsors/yologdev)._")
    else:
        if genesis:
            lines.append("**💎 Genesis Sponsors:**")
            lines.append("")
            lines.append(
                " ".join(avatar_tag(login, amount, 80) for login, amount in sorted(genesis))
            )
            lines.append("")
        if patrons:
            lines.append("**🚀 Sponsors:**")
            lines.append("")
            lines.append(
                " ".join(avatar_tag(login, amount, 64) for login, amount in sorted(patrons))
            )
            lines.append("")

    lines.append(README_MARKER_END)
    return "\n".join(lines)


def update_readme(sponsor_info, path=README_MD):
    """Replace the SPONSORS_START..SPONSORS_END block in README.

    Missing/malformed markers are FATAL (exit 5). This is the exact
    silent-failure class the refactor exists to kill: if a maintainer
    restructures README and accidentally drops the markers, top-tier
    sponsors would silently vanish from README forever. We'd rather
    fail the hourly job loudly and force a human to notice.

    Missing README file (first-run / fresh checkout) is also fatal,
    since this script is the single source of truth for that file.
    """
    if not os.path.exists(path):
        err(f"{path} not found — README.md must exist with SPONSORS_START/END markers")
        sys.exit(5)

    with open(path) as f:
        content = f.read()
    start_idx = content.find(README_MARKER_START)
    end_idx = content.find(README_MARKER_END)

    if start_idx == -1 or end_idx == -1:
        err(
            f"{path} is missing {README_MARKER_START} or {README_MARKER_END} — "
            f"refusing to silently drop sponsors from README"
        )
        sys.exit(5)
    if end_idx < start_idx:
        err(f"{path} markers are in the wrong order — refusing to update")
        sys.exit(5)

    new_block = render_readme_block(sponsor_info)
    end_of_end_marker = end_idx + len(README_MARKER_END)
    new_content = content[:start_idx] + new_block + content[end_of_end_marker:]

    if new_content == content:
        return False

    _atomic_write_text(path, new_content)
    print(f"  Updated {path} sponsor block.")
    return True


def write_active_json(sponsor_info, path=ACTIVE_FILE):
    """Persist a flat list of active sponsors. Write failures are fatal."""
    active = []
    for login, info in sponsor_info.items():
        benefits = info.get("benefits", [])
        if "priority" not in benefits and "genesis" not in benefits:
            continue  # Not active — expired or too small
        if info.get("type") == "recurring":
            dollars = info.get("monthly_cents", 0) // 100
            active.append({"login": login, "amount": f"${dollars}/mo", "type": "recurring"})
        else:
            dollars = info.get("total_cents", 0) // 100
            if "genesis" in benefits:
                active.append({"login": login, "amount": f"${dollars:,}", "type": "genesis"})
            else:
                active.append({"login": login, "amount": f"${dollars}", "type": "onetime"})
    _atomic_write_text(path, json.dumps(active, indent=2))
    return active


def create_shoutout_issues(sponsor_info):
    """Open GitHub issues for newly-eligible shoutout sponsors.

    Eligibility: `shoutout` benefit + not yet shouted out. Dedup: query
    existing issues with `Shoutout: @login` in title before creating.
    On any subprocess failure, warn and continue — this is a side
    effect that shouldn't take down the whole refresh job.

    Mutates `sponsor_info` in-place, setting `shouted_out=True` on the
    entry that earned the benefit (either the top-level entry or a
    nested one-time entry under a recurring sponsor).
    """
    if not _gh_available():
        warn("gh CLI not available — skipping shoutout issue creation")
        return

    # Iterate over a snapshot of (login, entry) pairs so we can also
    # process nested one-time entries under recurring logins.
    for login, top_entry in list(sponsor_info.items()):
        _maybe_shoutout(login, top_entry)
        nested = top_entry.get("onetime") if isinstance(top_entry, dict) else None
        if isinstance(nested, dict):
            _maybe_shoutout(login, nested)


def _maybe_shoutout(login, entry):
    """Attempt to create a shoutout issue for this (login, entry) pair.

    Mutates `entry["shouted_out"] = True` only on confirmed success
    (issue created, or existing issue found via dedup). Failures warn
    and leave shouted_out as-is so the next run retries.
    """
    if "shoutout" not in entry.get("benefits", []):
        return
    if entry.get("shouted_out", False):
        return

    # Dedup against existing issues
    try:
        result = subprocess.run(
            ["gh", "issue", "list", "--repo", REPO, "--state", "all",
             "--search", f'"Shoutout: @{login}" in:title',
             "--json", "number", "--jq", "length"],
            capture_output=True, text=True, timeout=15,
        )
    except (subprocess.TimeoutExpired, FileNotFoundError) as e:
        warn(f"could not check shoutouts for @{login}: {e}")
        return

    if result.returncode != 0:
        warn(f"could not check shoutouts for @{login}: {result.stderr.strip()}")
        return

    # Treat non-numeric output as "can't verify" rather than "exists"
    count_str = result.stdout.strip()
    try:
        count = int(count_str) if count_str else 0
    except ValueError:
        warn(f"unexpected gh output while deduping @{login}: {count_str!r}")
        return
    if count > 0:
        # Already exists — mark as shouted out so we don't query again
        entry["shouted_out"] = True
        return

    # Compose title and body
    if entry.get("type") == "recurring":
        dollars = entry.get("monthly_cents", 0) // 100
        amount_str = f"${dollars}/mo"
    else:
        dollars = entry.get("total_cents", 0) // 100
        amount_str = f"${dollars}"

    title = f"Shoutout: @{login} — {amount_str} sponsor"
    body = (
        f"Thank you @{login} for sponsoring yoyo! 🐙💖\n\n"
        f"Tier: {amount_str}\n\n"
        f"Your support helps keep yoyo evolving."
    )

    try:
        result = subprocess.run(
            ["gh", "issue", "create", "--repo", REPO,
             "--title", title, "--label", "shoutout", "--body", body],
            capture_output=True, text=True, timeout=15,
        )
    except (subprocess.TimeoutExpired, FileNotFoundError) as e:
        warn(f"failed to create shoutout for @{login}: {e}")
        return

    if result.returncode != 0:
        warn(f"failed to create shoutout for @{login}: {result.stderr.strip()}")
        return

    print(f"  Created shoutout issue for @{login}")
    entry["shouted_out"] = True


def _gh_available():
    try:
        subprocess.run(["gh", "--version"], capture_output=True, timeout=5, check=True)
        return True
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired, FileNotFoundError):
        return False


def _atomic_write_text(path, text):
    """Write `text` to `path` atomically via tempfile + os.replace.

    A crash mid-write leaves the tempfile behind (which we'd rather leak
    than corrupt the target) but never leaves `path` truncated. The
    target file either has the old content or the full new content —
    never a half-written JSON blob that the next run would silently
    treat as an empty file.
    """
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    tmp = f"{path}.tmp.{os.getpid()}"
    with open(tmp, "w") as f:
        f.write(text)
    os.replace(tmp, path)


def write_json(path, data):
    """Atomic JSON write. See _atomic_write_text."""
    _atomic_write_text(path, json.dumps(data, indent=2))


def _onetime_with_unused_run(sponsor_info):
    """Return list of logins that have $2+ onetime credit not yet consumed.

    Checks both top-level one-time entries and onetime-nested-under-recurring.
    """
    out = []
    for login, entry in sponsor_info.items():
        nested = _extract_onetime(entry)
        if nested is None:
            continue
        if nested.get("total_cents", 0) >= 200 and not nested.get("run_used", False):
            out.append(login)
    return out


def main():
    # Phase 1: fetch + validate. Any failure raises FetchFailed and we
    # exit BEFORE touching any committed file.
    try:
        nodes = load_raw_nodes(RAW_JSON)
    except FetchFailed as e:
        err(f"sponsor fetch failed — refusing to update committed files: {e}")
        sys.exit(2)

    recurring, onetime_from_api, monthly_cents = split_nodes(nodes)

    # Phase 2: load existing state. Missing is fine (fresh checkout);
    # unreadable is fatal (exit 3) — we refuse to silently overwrite a
    # corrupt file with defaults because that would destroy run_used flags.
    existing_state = load_json_or_default(SPONSOR_INFO_FILE, {})
    today = datetime.now(timezone.utc).strftime("%Y-%m-%d")

    # Phase 3: build fresh sponsor_info, preserving mutation fields from
    # existing_state (first_seen, run_used, shouted_out).
    sponsor_info = build_sponsor_info(recurring, onetime_from_api, existing_state, today)

    # Phase 4: side effects (issue creation) — mutates sponsor_info
    # in-place, setting shouted_out=true on confirmed issue creation.
    # Failures warn and leave shouted_out=false so the next run retries.
    create_shoutout_issues(sponsor_info)

    # Phase 5: write files. Any unhandled write error propagates loudly.
    #
    # Write order: listings (SPONSORS.md, README.md, active.json) BEFORE
    # the single state file (sponsor_info.json). Rationale: if a listing
    # write fails, we abort without persisting the in-memory shouted_out
    # mutations from create_shoutout_issues. The next run reloads the
    # on-disk state, re-derives sponsor_info, and hits the dedup path
    # (existing issue found) — self-healing. If we persisted state
    # first and then failed on a listing, state would claim
    # shouted_out=true while the listing never got the update.
    update_sponsors_md(sponsor_info)
    update_readme(sponsor_info)
    write_active_json(sponsor_info)
    write_json(SPONSOR_INFO_FILE, sponsor_info)

    has_credits = "true" if _onetime_with_unused_run(sponsor_info) else "false"
    print(f"{monthly_cents}|{has_credits}")


if __name__ == "__main__":
    main()
