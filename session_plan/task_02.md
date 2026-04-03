Title: Release changelog extraction script + retroactive fix (Issue #240)
Files: scripts/extract_changelog.sh (new)
Issue: #240

## What to do

Create a script that extracts a specific version's changelog section from CHANGELOG.md and apply it retroactively to all existing GitHub releases. This addresses @danstis's request for human-readable release descriptions.

### Step 1: Create `scripts/extract_changelog.sh`

Create a new bash script that:
- Takes a version tag as argument (e.g., `v0.1.5`)
- Strips the `v` prefix to get the version number (e.g., `0.1.5`)
- Extracts everything between `## [0.1.5]` and the next `## [` heading from CHANGELOG.md
- Outputs the extracted section to stdout
- Exits with error if version not found

Example usage:
```bash
./scripts/extract_changelog.sh v0.1.5
# Outputs the 0.1.5 changelog section
```

Implementation approach:
```bash
#!/usr/bin/env bash
set -euo pipefail

TAG="${1:?Usage: extract_changelog.sh <tag>}"
VERSION="${TAG#v}"

# Use awk to extract section between ## [VERSION] and next ## [
awk -v ver="$VERSION" '
  /^## \[/ { if (found) exit; if (index($0, "[" ver "]")) found=1; next }
  found { print }
' CHANGELOG.md
```

Make it executable: `chmod +x scripts/extract_changelog.sh`

### Step 2: Apply retroactively to all existing releases

Use `gh release edit` to update each existing release with its changelog content:

```bash
for tag in $(gh release list --repo yologdev/yoyo-evolve --json tagName -q '.[].tagName'); do
  body=$(./scripts/extract_changelog.sh "$tag" 2>/dev/null) || continue
  if [ -n "$body" ]; then
    gh release edit "$tag" --repo yologdev/yoyo-evolve --notes "$body"
  fi
done
```

Run this as part of the implementation (it's safe — just updates release descriptions).

### Step 3: File a help-wanted issue

Since `.github/workflows/release.yml` cannot be modified by the agent, file a GitHub issue requesting the human wire the script into the release workflow:

```bash
gh issue create --repo yologdev/yoyo-evolve \
  --title "Help wanted: Wire extract_changelog.sh into release workflow" \
  --body "Created scripts/extract_changelog.sh to extract version-specific changelog sections from CHANGELOG.md.

To complete Issue #240, the release workflow needs to use this script to populate the GitHub release body instead of \`generate_release_notes: true\`.

Suggested change to .github/workflows/release.yml in the 'Create GitHub Release' step:
\`\`\`yaml
- uses: actions/checkout@v4
- name: Extract changelog
  id: changelog
  run: |
    BODY=\$(./scripts/extract_changelog.sh \${{ github.ref_name }})
    echo 'body<<EOF' >> \$GITHUB_OUTPUT
    echo \"\$BODY\" >> \$GITHUB_OUTPUT
    echo 'EOF' >> \$GITHUB_OUTPUT
- name: Create GitHub Release
  uses: softprops/action-gh-release@v2
  with:
    body: \${{ steps.changelog.outputs.body }}
    files: |
      yoyo-*.tar.gz
      ...
\`\`\`" \
  --label agent-help-wanted
```

### No source code changes needed

This task creates a new script and applies it — no Rust code modified.
