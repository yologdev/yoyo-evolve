# GASP delivery and recovery

The shell harness closes each run once. `scripts/gasp_delivery.py` delivers that
completed Git commit; it never executes yoyo or appends another session ending.
Evolution, social, dream and skill harnesses that source `gasp_shim.sh` use this
path. Cloudflare/Pocket's separate ingestion paths do not automatically use it.

Requirements: Git, Python 3 and the harness validator below. The validator uses
the same pinned `yoagent-state` reducer as yoyo; it is not a change to yoyo's CLI.

```sh
cargo build --locked --manifest-path scripts/gasp-delivery-validator/Cargo.toml \
  --target-dir target/gasp-delivery-validator
```

## Delivery contract

The helper anchors the original commit under `refs/gasp-delivery/<run-id-hash>`
in the source clone, then works in a temporary checkout. Up to five attempts
fetch main, rebase the single boundary commit, validate it, and push. Commands
have timeouts; retries use bounded exponential backoff and jitter. Access errors
and merge conflicts stop retries. No force pushes or automatic conflict choices.

Validation preserves main's exact event prefix, checks original event identities
and contents, rejects duplicate event IDs, and applies new operations through the
runtime reducer. Already-published legacy skipped operations remain readable;
new dangling operations are rejected. Memory facts, mirrors and their cursors
remain in the same boundary commit. They are never silently discarded to fix a
merge conflict.

Each published boundary includes `delivery/<run-id-hash>.json` containing only
the run ID and original commit ID. If a push acknowledgement is lost, the helper
checks this receipt and its events on main before retrying. The original Git
commit is preserved even though a successful rebase changes the delivered SHA.

The helper prints one JSON result and uses these exit codes:

| Result | Exit | Meaning |
| --- | --- | --- |
| `published` | 0 | Main acknowledged the validated record, or its matching receipt was found. |
| `saved_for_recovery` | 20 | Main publication failed; the original commit is verified on a recovery branch. |
| `local_only` | 21 | No remote acknowledgement; inspect the reason and preserve the local clone. |

Results also live in the source clone's Git directory under `gasp-delivery/`.
Credentials and remote URLs are excluded. The remote defaults to `origin`;
`GASP_PUSH_URL` can supply authenticated access through the environment. Never
include a token in a command example, receipt, or tracked config file.

The shim stays fail-soft for execution: it prints the result, exposes
`GASP_DELIVERY_STATUS`, and writes a `gasp_delivery` step output when available.
The evolution workflow checks that output **after** execution retries, so a
recording failure fails the job without executing Yoyo again. The clone is
removed only after `published`. An EXIT trap cannot close the run a second time.

## Recover a remote backup

Failed delivery tries `refs/heads/gasp-recovery/<run-id-hash>` without overwriting
an existing different commit. This is a backup, not a public main-ledger entry.
It contains the original GASP record, even when validation failed, so it must
never be blindly merged. The normal validation gates also apply to recovery.

From a clean GASP checkout with `origin` pointing to the state repository:

```sh
python3 /path/to/yoyo-evolve/scripts/gasp_delivery.py /path/to/yoyo-gasp \
  --run-id run_day209_EXAMPLE --recover \
  --validator /path/to/yoyo-evolve/target/gasp-delivery-validator/debug/gasp-delivery-validator
```

Use the actual run ID printed by the shim. The command fetches its recovery
branch and delivers that existing record. Repeating it after success checks the
receipt and does not create duplicate events or commits. Recovery branches are
retained for inspection; remove one only after confirming `published`.

For `local_only`, fix remote access and rerun the command against the retained
original clone, omitting `--recover`. If the reason is a merge conflict, invalid
events, or mismatched identity, investigate it before attempting recovery; this
helper deliberately makes no semantic repairs.

A hard kill before the boundary commit is outside this delivery contract. If
both main and backup pushes are unreachable and an ephemeral runner disappears,
its local-only copy can still be lost. This implementation does not promise
offline remote durability.

## Tests

```sh
python3 -m unittest discover -s scripts -p test_gasp_delivery.py -v
```

Tests use real local Git remotes and the real reducer. They cover concurrent
updates, lost acknowledgement, repeat delivery/recovery, memory conflicts,
invalid new operations, tolerated legacy skips, denied access, network failure,
credential redaction, and repeated EXIT-trap calls.
