#!/usr/bin/env python3
"""Build src-tauri/resources/masters/players.json from PGN Mentor's file list.

The app downloads a player's games from PGN Mentor only when the user asks
for them; this catalogue just lists who is available. Re-run it to refresh:

    python3 scripts/masters-catalog.py            # fetch the page
    python3 scripts/masters-catalog.py files.html # or use a saved copy
"""
import html
import json
import re
import sys
import urllib.request
from pathlib import Path

URL = "https://www.pgnmentor.com/files.html"
OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "resources" / "masters" / "players.json"
ROW = re.compile(r'<a href="players/([A-Za-z0-9_.-]+)\.zip">[^<]*</a>.*?</td><td>([^<]+?),\s*([\d,]+)\s+games?</td>', re.S)


def main() -> None:
    if len(sys.argv) > 1:
        text = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
    else:
        req = urllib.request.Request(URL, headers={"User-Agent": "GrandieChess-catalog/1 (+https://github.com/Inad-Tech-World/grandie_chess)"})
        text = urllib.request.urlopen(req, timeout=60).read().decode("utf-8", errors="replace")
    seen = {}
    for file, name, games in ROW.findall(text):
        name = html.unescape(name).strip()
        seen[file] = {"file": file, "name": name, "games": int(games.replace(",", ""))}
    players = sorted(seen.values(), key=lambda p: p["name"].split()[-1].lower())
    if len(players) < 100:
        sys.exit(f"only {len(players)} players parsed; the page layout may have changed")
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps({"source": URL, "players": players}, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {len(players)} players to {OUT}")


if __name__ == "__main__":
    main()
