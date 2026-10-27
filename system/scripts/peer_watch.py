#!/usr/bin/env python3
"""Peer watch — pull the P0 competitors' commit/release digests.

Targets live in PEER-WATCH.md (aginxos-next). Output: a dated markdown
digest on stdout; the watch log is maintained by the operator agent
(runs this, diffs against the last entry, appends the analysis).
"""
import json
import sys
import urllib.request
from datetime import datetime, timezone

TARGETS = [
    ("Obscura", "h4ckf0r0day/obscura"),
    ("Moli", "lexmount/moli"),
]

API = "https://api.github.com"


def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": "peer-watch"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.loads(r.read().decode())


def since(commits, days=7):
    cutoff = datetime.now(timezone.utc).timestamp() - days * 86400
    out = []
    for c in commits:
        from email.utils import parsedate_to_datetime
        # ISO 8601 with Z
        dt = datetime.fromisoformat(c["commit"]["author"]["date"].replace("Z", "+00:00"))
        if dt.timestamp() >= cutoff:
            out.append((c["commit"]["author"]["date"][:10], c["commit"]["message"].split("\n")[0]))
    return out


def main(days=7):
    today = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    for label, repo in TARGETS:
        print(f"## {label}（{repo}）近 {days} 天")
        try:
            commits = get(f"{API}/repos/{repo}/commits?per_page=50")
            recent = since(commits, days)
            if not recent:
                print("  （无提交）")
            for date, msg in recent:
                print(f"- {date} {msg[:100]}")
            rels = get(f"{API}/repos/{repo}/releases?per_page=5")
            fresh = [
                r for r in rels
                if datetime.fromisoformat(r["published_at"].replace("Z", "+00:00")).timestamp()
                >= datetime.now(timezone.utc).timestamp() - days * 86400
            ]
            if fresh:
                print("  releases:", ", ".join(f"{r['tag_name']}({r['published_at'][:10]})" for r in fresh))
        except Exception as e:
            print(f"  FETCH-ERROR: {e}")
        print()


if __name__ == "__main__":
    main(int(sys.argv[1]) if len(sys.argv) > 1 else 7)
