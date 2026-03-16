#!/usr/bin/env python3
"""Build the yoyo journey website from markdown sources."""

import html
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "site"


def read_file(name):
    try:
        return (ROOT / name).read_text()
    except FileNotFoundError:
        print(f"WARNING: {name} not found — section will be empty")
        return ""


def md_inline(text):
    """Convert inline markdown (bold, code, links) to HTML."""
    text = html.escape(text)
    text = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", text)
    text = re.sub(r"`(.+?)`", r"<code>\1</code>", text)
    text = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", r'<a href="\2">\1</a>', text)
    return text


# ── Parsers ──


def parse_journal(content):
    entries = []
    chunks = re.split(r"^## ", content, flags=re.MULTILINE)
    for chunk in chunks:
        chunk = chunk.strip()
        if not chunk:
            continue
        lines = chunk.split("\n")
        m = re.match(r"Day\s+(\d+)\s*[—–\-]+\s*(.+)", lines[0])
        if not m:
            continue
        day = int(m.group(1))
        title = m.group(2).strip()
        body = "\n".join(lines[1:]).strip()
        entries.append({"day": day, "title": title, "body": body})
    return entries



def parse_identity(content):
    intro_lines = []
    rules = []
    sections = re.split(r"^## ", content, flags=re.MULTILINE)
    for section in sections:
        section = section.strip()
        if not section:
            continue
        lines = section.split("\n")
        header = lines[0].strip()
        # Intro: everything before the first ## (starts with # title)
        if header.startswith("# ") or header.startswith("Who "):
            for line in lines[1:] if header.startswith("# ") else lines:
                if line.strip():
                    intro_lines.append(line.strip())
        elif "rule" in header.lower():
            for line in lines[1:]:
                m = re.match(r"^\d+\.\s+\*\*(.+?)\*\*(.*)$", line)
                if m:
                    rules.append(
                        f"<strong>{html.escape(m.group(1))}</strong>"
                        f"{md_inline(m.group(2))}"
                    )
                elif re.match(r"^\d+\.", line):
                    text = line.split(".", 1)[1].strip()
                    rules.append(md_inline(text))
    return {"intro": intro_lines, "rules": rules}


# ── Renderers ──


def render_journal(entries):
    if not entries:
        return (
            '<div class="timeline-empty">'
            "No journal entries yet. The journey begins soon."
            "</div>"
        )
    parts = []
    for entry in entries:
        body_html = ""
        if entry["body"]:
            body_html = md_inline(entry["body"])
            body_html = body_html.replace("\n\n", "<br><br>").replace("\n", " ")
        parts.append(
            f'      <article class="entry">\n'
            f'        <div class="entry-marker"></div>\n'
            f'        <div class="entry-content">\n'
            f'          <span class="entry-day">Day {entry["day"]}</span>\n'
            f'          <h3 class="entry-title">{md_inline(entry["title"])}</h3>\n'
            f'          <p class="entry-body">{body_html}</p>\n'
            f"        </div>\n"
            f"      </article>"
        )
    return "\n".join(parts)



def render_identity(identity):
    parts = []
    if identity["intro"]:
        # First paragraph as mission statement
        mission = md_inline(identity["intro"][0])
        parts.append(f'      <p class="mission">{mission}</p>')
        # Remaining paragraphs
        for line in identity["intro"][1:]:
            parts.append(f'      <p class="identity-text">{md_inline(line)}</p>')
    if identity["rules"]:
        parts.append('      <ol class="rules">')
        for rule in identity["rules"]:
            parts.append(f"        <li>{rule}</li>")
        parts.append("      </ol>")
    return "\n".join(parts)


# ── Templates ──


HTML_TEMPLATE = """\
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>yoyo \u2014 Day {day_count}</title>
  <meta name="description" content="A coding agent that evolves itself. Currently on Day {day_count}.">
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:ital,wght@0,300;0,400;0,500;0,700;1,400&display=swap" rel="stylesheet">
  <link rel="stylesheet" href="style.css">
</head>
<body>
  <nav>
    <a href="#" class="nav-name">yoyo</a>
    <div class="nav-links">
      <a href="#journal">journal</a>
      <a href="#identity">identity</a>
      <a href="https://github.com/yologdev/yoyo-evolve" target="_blank" rel="noopener">github \u2197</a>
    </div>
  </nav>

  <main>
    <header class="hero">
      <h1>yoyo<span class="cursor">_</span></h1>
      <p class="day-count">Day {day_count}</p>
      <p class="tagline">a coding agent growing up in public</p>
    </header>

    <section id="journal">
      <h2 class="section-label">// journal</h2>
      <div class="timeline">
{journal_html}
      </div>
    </section>

    <section id="identity">
      <h2 class="section-label">// identity</h2>
{identity_html}
    </section>
  </main>

  <footer>
    <p>built by an AI that evolves itself</p>
    <a href="https://github.com/yologdev/yoyo-evolve">github.com/yologdev/yoyo-evolve</a>
  </footer>
</body>
</html>
"""

CSS = """\
/* yoyo journey — terminal chronicle */

:root {
  --bg: #0a0c10;
  --bg-raised: #12161c;
  --border: #1e2330;
  --text: #9ca3af;
  --text-bright: #d1d5db;
  --text-dim: #4a5568;
  --cyan: #22d3ee;
  --green: #34d399;
  --amber: #f59e0b;
  --red: #ef4444;
  --font: "JetBrains Mono", "Fira Code", "Cascadia Code", "Source Code Pro", monospace;
}

*, *::before, *::after {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html {
  scroll-behavior: smooth;
  scroll-padding-top: 4rem;
}

body {
  background: var(--bg);
  color: var(--text);
  font-family: var(--font);
  font-size: 14px;
  line-height: 1.7;
  -webkit-font-smoothing: antialiased;
}

a {
  color: var(--cyan);
  text-decoration: none;
}

a:hover {
  text-decoration: underline;
}

strong {
  color: var(--text-bright);
  font-weight: 500;
}

code {
  background: var(--bg-raised);
  padding: 0.15em 0.4em;
  font-size: 0.9em;
  border: 1px solid var(--border);
}


/* ── nav ── */

nav {
  position: sticky;
  top: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: space-between;
  max-width: 640px;
  width: 90%;
  margin: 0 auto;
  padding: 1rem 0;
  border-bottom: 1px solid var(--border);
  background: var(--bg);
}

.nav-name {
  font-weight: 700;
  font-size: 0.85rem;
  color: var(--cyan);
  letter-spacing: 0.05em;
}

.nav-name:hover {
  text-decoration: none;
  opacity: 0.8;
}

.nav-links {
  display: flex;
  gap: 1.5rem;
}

.nav-links a {
  color: var(--text-dim);
  font-size: 0.75rem;
  letter-spacing: 0.08em;
}

.nav-links a:hover {
  color: var(--text);
  text-decoration: none;
}


/* ── main ── */

main {
  max-width: 640px;
  width: 90%;
  margin: 0 auto;
}


/* ── hero ── */

.hero {
  padding: 5rem 0 4rem;
}

.hero h1 {
  font-size: 3.5rem;
  font-weight: 700;
  color: var(--cyan);
  line-height: 1;
  letter-spacing: -0.02em;
}

@keyframes blink {
  0%, 100% { opacity: 1; }
  50% { opacity: 0; }
}

.cursor {
  animation: blink 1.2s step-end infinite;
  color: var(--cyan);
  font-weight: 300;
}

.day-count {
  margin-top: 1rem;
  font-size: 1rem;
  color: var(--green);
  font-weight: 500;
}

.tagline {
  margin-top: 0.5rem;
  color: var(--text-dim);
  font-size: 0.85rem;
  font-style: italic;
}


/* ── sections ── */

section {
  padding: 3.5rem 0 0;
}

.section-label {
  font-size: 0.7rem;
  font-weight: 400;
  color: var(--text-dim);
  letter-spacing: 0.12em;
  margin-bottom: 2rem;
}


/* ── journal timeline ── */

.timeline {
  position: relative;
  padding-left: 28px;
}

.timeline::before {
  content: '';
  position: absolute;
  left: 3px;
  top: 6px;
  bottom: 0;
  width: 1px;
  background: var(--border);
}

.timeline-empty {
  color: var(--text-dim);
  font-style: italic;
  padding-left: 28px;
}

.entry {
  position: relative;
  margin-bottom: 2.5rem;
}

.entry-marker {
  position: absolute;
  left: -28px;
  top: 8px;
  width: 7px;
  height: 7px;
  background: var(--green);
}

.entry-day {
  font-size: 0.75rem;
  font-weight: 700;
  color: var(--green);
  letter-spacing: 0.05em;
}

.entry-title {
  font-size: 1.05rem;
  font-weight: 500;
  color: var(--text-bright);
  margin: 0.25rem 0 0.5rem;
  line-height: 1.4;
}

.entry-body {
  color: var(--text);
  font-size: 0.85rem;
  line-height: 1.7;
}


/* ── identity ── */

.mission {
  font-size: 1rem;
  color: var(--text-bright);
  line-height: 1.8;
  margin-bottom: 1.5rem;
  padding-left: 1rem;
  border-left: 2px solid var(--cyan);
}

.identity-text {
  font-size: 0.85rem;
  line-height: 1.7;
  margin-bottom: 1rem;
}

.rules {
  list-style: none;
  counter-reset: rules;
  padding: 0;
  margin-top: 2rem;
}

.rules li {
  counter-increment: rules;
  position: relative;
  padding-left: 2.5rem;
  margin-bottom: 0.75rem;
  font-size: 0.85rem;
  line-height: 1.7;
}

.rules li::before {
  content: counter(rules, decimal-leading-zero);
  position: absolute;
  left: 0;
  color: var(--text-dim);
  font-size: 0.75rem;
  font-weight: 300;
  top: 0.15rem;
}


/* ── footer ── */

footer {
  max-width: 640px;
  width: 90%;
  margin: 4rem auto 0;
  padding: 2rem 0 4rem;
  border-top: 1px solid var(--border);
}

footer p {
  font-size: 0.75rem;
  color: var(--text-dim);
  margin-bottom: 0.25rem;
}

footer a {
  font-size: 0.75rem;
  color: var(--text-dim);
}

footer a:hover {
  color: var(--cyan);
}


/* ── responsive ── */

@media (max-width: 480px) {
  .hero h1 {
    font-size: 2.5rem;
  }

  nav {
    flex-direction: column;
    align-items: flex-start;
    gap: 0.5rem;
  }

  .nav-links {
    gap: 1rem;
  }
}
"""


# ── Build ──


def build():
    day_count = 0
    try:
        day_count = int(read_file("DAY_COUNT").strip())
    except (ValueError, AttributeError):
        pass

    journal_html = render_journal(parse_journal(read_file("JOURNAL.md")))
    identity_html = render_identity(parse_identity(read_file("IDENTITY.md")))

    page = HTML_TEMPLATE.format(
        day_count=day_count,
        journal_html=journal_html,
        identity_html=identity_html,
    )

    DOCS.mkdir(exist_ok=True)
    (DOCS / "index.html").write_text(page)
    (DOCS / "style.css").write_text(CSS)
    (DOCS / ".nojekyll").touch()

    print(f"Site built: site/index.html (Day {day_count})")


if __name__ == "__main__":
    build()
