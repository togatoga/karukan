#!/usr/bin/env python3
"""Bootstrap karukan-engine/data/single_kanji.yml from Mozc's single-kanji data.

Run once. The YAML is maintained by hand afterwards, so this refuses to
overwrite an existing file (pass --force to regenerate from scratch).

Upstream files (under `src/data/single_kanji/` in a Mozc checkout):

  * `single_kanji.tsv`: one row per reading, `<reading>\\t<kanji...>`, the
    kanji concatenated in display order (first = best).
  * `variant_rule.txt`: blocks headed by a type (異体字, 旧字体, ...) and
    `<kanji>\\t<base>` rows, meaning the kanji is annotated `<base>の<type>`.

Output shape, one entry per kanji:

  entries:
    - char: 高
      readings: [こ, こう, たか, だか]
    - char: 髙
      readings: [こう, たか]
      description: 高の異体字

A reading's candidates come out in file order (the entry higher in the
file is shown first). The TSV orders each reading's row by hand and the
same two kanji can be ordered differently under different readings, so
no single file order reproduces every row; the entries are sorted so
that as many of those row orders as possible survive (common kanji
first), which also puts a hand-added rare kanji where it belongs: at
the end.

Usage:
    python3 scripts/single_kanji_porter.py \
        --mozc /path/to/google/mozc \
        --out karukan-engine/data/single_kanji.yml
"""

from __future__ import annotations

import argparse
import re
import statistics
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path

_INT_LIKE = re.compile(r"^[+-]?\d+$")
_FLOAT_LIKE = re.compile(r"^[+-]?(\d+\.\d*|\.\d+|\d+(\.\d*)?[eE][+-]?\d+)$")


def yaml_escape(value: str) -> str:
    """Render `value` as a safe YAML scalar, inside a flow list too
    (quoted when YAML 1.1 would read it as something other than a
    plain string)."""
    if value == "":
        return "''"
    needs_quote = (
        value[0] in "-+?:,[]{}#&*!|>'\"%@`"
        or value[0].isdigit()
        or value.lower() in ("true", "false", "null", "yes", "no", "on", "off", "~")
        or any(ch in value for ch in ":#,[]{}\n")
        or value != value.strip()
        or bool(_INT_LIKE.match(value))
        or bool(_FLOAT_LIKE.match(value))
    )
    if needs_quote:
        escaped = value.replace("'", "''")
        return f"'{escaped}'"
    return value


def is_kana(char: str) -> bool:
    return "぀" <= char <= "ヿ"


@dataclass
class Entry:
    char: str
    readings: list[str] = field(default_factory=list)
    description: str | None = None


def read_single_kanji(path: Path) -> list[tuple[str, list[str]]]:
    """`(reading, [kanji...])` rows, each in upstream display order."""
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        reading, kanjis = line.split("\t")
        seen: set[str] = set()
        chars = []
        for char in kanjis:
            # A few rows carry okurigana (生る, 姉さん); only the kanji is
            # a single-kanji candidate.
            if char in seen or is_kana(char):
                continue
            seen.add(char)
            chars.append(char)
        rows.append((reading, chars))
    return rows


def read_variants(path: Path) -> dict[str, str]:
    """kanji → `<base>の<type>`. A kanji listed under several types keeps
    the first."""
    kind: str | None = None
    out: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        if "\t" not in line:
            kind = line.strip()
            continue
        if kind is None:
            continue
        target, base = line.split("\t")
        out.setdefault(target, f"{base}の{kind}")
    return out


def order_kanji(rows: list[tuple[str, list[str]]]) -> list[str]:
    """One global order keeping as many of the rows' pairwise orders as
    possible: start from each kanji's mean relative position in its rows,
    then move kanji within a window while the rows' pairwise votes say
    they belong earlier or later."""
    positions: dict[str, list[float]] = defaultdict(list)
    votes: dict[tuple[str, str], int] = defaultdict(int)
    for _, chars in rows:
        for i, char in enumerate(chars):
            positions[char].append(i / len(chars))
            for later in chars[i + 1 :]:
                votes[(char, later)] += 1

    def prefers(a: str, b: str) -> int:
        """Positive when the rows want `a` before `b`."""
        return votes.get((a, b), 0) - votes.get((b, a), 0)

    order = sorted(positions, key=lambda c: (statistics.mean(positions[c]), c))
    window = 200
    for _ in range(3):
        for i in range(len(order)):
            char = order[i]
            best, best_gain = i, 0
            gain = 0
            for j in range(i - 1, max(0, i - window) - 1, -1):
                gain += prefers(char, order[j])
                if gain > best_gain:
                    best, best_gain = j, gain
            gain = 0
            for j in range(i + 1, min(len(order), i + window + 1)):
                gain += prefers(order[j], char)
                if gain > best_gain:
                    best, best_gain = j, gain
            if best != i:
                order.pop(i)
                order.insert(best, char)
    return order


HEADER = """\
# Single-kanji conversion data: たか → 高, 嵩, 鷹, … one kanji at a time.
# Bootstrapped from Mozc single_kanji.tsv and variant_rule.txt
# (https://github.com/google/mozc) by scripts/single_kanji_porter.py and
# maintained by hand since. Mozc copyright/license: see /THIRD_PARTY_LICENSES.
#
# One entry per kanji:
#   - char: 髙                    the kanji
#     readings: [こう, たか]      every reading it converts from
#     description: 高の異体字     optional note shown beside the candidate
#                                 (also shown when another source produces
#                                 the same kanji)
# For one reading the entries higher in the file come out first, so put a
# common kanji above a rare one. A kanji listed twice is merged.
"""


def render_yaml(entries: list[Entry]) -> str:
    out: list[str] = [HEADER, "entries:"]
    for e in entries:
        out.append(f"  - char: {yaml_escape(e.char)}")
        if e.readings:
            readings = ", ".join(yaml_escape(r) for r in e.readings)
            out.append(f"    readings: [{readings}]")
        if e.description:
            out.append(f"    description: {yaml_escape(e.description)}")
    out.append("")
    return "\n".join(out)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--mozc", type=Path, required=True, help="Mozc checkout root")
    parser.add_argument("--out", type=Path, required=True, help="Output YAML path")
    parser.add_argument("--force", action="store_true", help="Overwrite an existing file")
    args = parser.parse_args()

    if args.out.exists() and not args.force:
        print(f"{args.out} exists and is hand-maintained; pass --force to regenerate", file=sys.stderr)
        return 1
    data_dir = args.mozc / "src" / "data" / "single_kanji"
    tsv = data_dir / "single_kanji.tsv"
    variants = data_dir / "variant_rule.txt"
    for path in (tsv, variants):
        if not path.is_file():
            print(f"missing: {path}", file=sys.stderr)
            return 1

    rows = read_single_kanji(tsv)
    descriptions = read_variants(variants)
    entries = {char: Entry(char) for char in order_kanji(rows)}
    for reading, chars in rows:
        for char in chars:
            entries[char].readings.append(reading)
    # Variant-only kanji (no reading) still get an entry: the note
    # decorates the kanji when another source produces it.
    for char, description in descriptions.items():
        entries.setdefault(char, Entry(char)).description = description
    ordered = list(entries.values())
    args.out.write_text(render_yaml(ordered), encoding="utf-8")
    print(
        f"{len(rows)} readings, {len(ordered)} kanji, {len(descriptions)} descriptions → {args.out}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
