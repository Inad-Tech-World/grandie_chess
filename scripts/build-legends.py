#!/usr/bin/env python3
"""Build src-tauri/resources/masters/legends.pgn: a small pack of famous games
bundled with the app, taken from PGN Mentor player files (game scores are
historical records). Every game is checked for legal moves by the Rust test
`masters::legends::tests`.

    python3 scripts/build-legends.py DIR   # DIR holds Anderssen.pgn, Fischer.pgn, ...
"""
import re
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "resources" / "masters" / "legends.pgn"

# (file, white contains, black contains, year, round or None, title, why it's
# famous). A round like "Tilburg|1-0" also requires the event to contain
# "Tilburg" and the result to be 1-0.
LEGENDS = [
    ("Anderssen", "Anderssen", "Kieseritzky", "1851", None, "The Immortal Game", "Anderssen gives up both rooks, a bishop and his queen, then mates with his three remaining minor pieces."),
    ("Anderssen", "Anderssen", "Dufresne", "1852", None, "The Evergreen Game", "A quiet-looking position explodes into a queen sacrifice and a mating attack."),
    ("Morphy", "Morphy", "Isouard", "1858", None, "The Opera Game", "Morphy's model of rapid development: every piece joins the attack and the game ends with a queen sacrifice."),
    ("Lasker", "Lasker", "Bauer", "1889", None, "Lasker's double bishop sacrifice", "The first famous sacrifice of both bishops to strip the enemy king."),
    ("Steinitz", "Steinitz", "Bardeleben", "1895", None, "Steinitz vs von Bardeleben, Hastings", "A cascade of rook checks that Black could only escape by leaving the board."),
    ("Rubinstein", "Rotlewi", "Rubinstein", "1907", None, "Rubinstein's Immortal", "Rubinstein sacrifices both rooks and his queen in a combination of stunning geometry."),
    ("Capablanca", "Capablanca", "Marshall", "1918", None, "The Marshall Attack's debut", "Marshall unveils his prepared gambit; Capablanca defends calmly and wins."),
    ("Alekhine", "Bogoljubow", "Alekhine", "1922", None, "Alekhine's Hastings masterpiece", "Alekhine's long-range combination ending in a queen promotion."),
    ("Reti", "Reti", "Alekhine", "1925", None, "Réti vs Alekhine, Baden-Baden", "Alekhine's deep combination with rook sacrifices, calculated a dozen moves ahead."),
    ("Botvinnik", "Botvinnik", "Capablanca", "1938", None, "Botvinnik vs Capablanca, AVRO", "A classic: a central pawn majority, then a bishop sacrifice and a queen-and-knight attack."),
    ("Fischer", "Byrne", "Fischer", "1956", None, "The Game of the Century", "Thirteen-year-old Fischer gives up his queen for a crushing attack."),
    ("Tal", "Botvinnik", "Tal", "1960", "6", "Botvinnik vs Tal, World Championship game 6", "Tal's famous knight sacrifice that his opponent couldn't refute at the board."),
    ("Petrosian", "Petrosian", "Spassky", "1966", "10", "Petrosian vs Spassky, World Championship game 10", "Petrosian's exchange sacrifices and a final queen sacrifice."),
    ("Fischer", "Fischer", "Spassky", "1972", "6", "Fischer vs Spassky, World Championship game 6", "Fischer opens 1.c4 for a change and plays a positional masterpiece Spassky applauded."),
    ("Karpov", "Karpov", "Kasparov", "1985", "16", "Karpov vs Kasparov, World Championship game 16", "Kasparov's 'octopus' knight on d3 paralyses White's position."),
    ("Short", "Short", "Timman", "1991", "Tilburg|1-0", "Short's king walk", "White's king marches up the board to help deliver mate in the middlegame."),
    ("Kasparov", "Kasparov", "Topalov", "1999", None, "Kasparov's Immortal", "A rook sacrifice on d4 starts a king hunt across the whole board."),
    ("PolgarJ", "Polgar", "Kasparov", "2002", "ROW|1-0", "Polgar vs Kasparov, Russia vs Rest of the World", "Judit Polgar's first win over the world number one."),
    ("Carlsen", "Carlsen", "Anand", "2013", "WCh|1-0", "Carlsen vs Anand, World Championship game 5", "Carlsen grinds a near-equal endgame into a win: his trademark."),
    ("Carlsen", "Carlsen", "Anand", "2014", "6", "Carlsen vs Anand, World Championship 2014 game 6", "Both sides miss a chance in a tense game Carlsen goes on to win."),
]


def games(text: str):
    """Yield (headers dict, full PGN text) for each game."""
    parts = re.split(r"\n(?=\[Event )", "\n" + text.replace("\r\n", "\n"))
    for p in parts:
        p = p.strip()
        if not p.startswith("[Event"):
            continue
        headers = dict(re.findall(r'^\[(\w+) "([^"]*)"\]', p, re.M))
        yield headers, p


def main() -> None:
    src = Path(sys.argv[1])
    cache = {}
    out = []
    for file, white, black, year, rnd, title, why in LEGENDS:
        if file not in cache:
            cache[file] = list(games((src / f"{file}.pgn").read_text(encoding="latin-1")))
        found = None
        for h, pgn in cache[file]:
            if white.lower() in h.get("White", "").lower() and black.lower() in h.get("Black", "").lower() and h.get("Date", "").startswith(year):
                if rnd and "|" in rnd:
                    event, result = rnd.split("|")
                    ok = event.lower() in h.get("Event", "").lower() and h.get("Result") == result
                else:
                    ok = rnd is None or h.get("Round", "").split(".")[0] == rnd
                if ok:
                    found = pgn
                    break
        if not found:
            print(f"MISSING: {title} ({white} - {black} {year} r{rnd})")
            continue
        head, _, body = found.partition("\n\n")
        tags = f'[Legend "{title}"]\n[Famous "{why}"]'
        out.append(f"{head}\n{tags}\n\n{body.strip()}\n")
        print(f"ok: {title}")
    OUT.write_text("\n".join(out), encoding="utf-8")
    print(f"wrote {len(out)} games to {OUT}")


if __name__ == "__main__":
    main()
