#!/usr/bin/env python3
"""Port Mozc + Unicode CLDR emoji data into karukan-engine/data/emoji.yml.

Two upstream sources feed the file:

  * Mozc `src/data/emoji/emoji_data.tsv` provides Japanese hiragana
    readings and concise Japanese descriptions — the input most users
    type to surface an emoji via the IME's existing reading-lookup
    path. Columns used: 2 (UTF-8 emoji), 3 (space-separated hiragana
    readings), 6 (space-separated description tokens, "best label
    first").

  * Unicode UTS #51 `emoji-test.txt` provides the canonical CLDR
    English short name for each fully-qualified emoji. These become
    snake_case shortcodes (`grinning face with smiling eyes` →
    `grinning_face_with_smiling_eyes`) for the Slack-style
    `:shortcode` lookup.

Output `emoji.yml` shape:

  descriptions:    emoji → Japanese description (from Mozc col 6's
                   first token). Hand-curated overrides may be added
                   but keys already present here are overwritten.
  entries:         one row per emoji with:
                     - char       : the emoji glyph
                     - readings   : hiragana strings for kana-input
                                    lookup (e.g. `ぴえん` → 🥺)
                     - triggers   : Slack-style ASCII trigger strings
                                    (`:smile`, `:pien`, ...). Unified
                                    list combining MANUAL_ALIASES, CLDR
                                    snake_case names, and romaji
                                    derived from each reading. Single-n
                                    and double-n variants are both
                                    emitted so `:kiniku` and
                                    `:kinniku` both hit 💪 (read:
                                    きんにく).
                     - description: per-entry Japanese description

The previous file shape had a separate top-level `shortcodes:`
section; that's folded into per-entry `triggers:` so all "things you
can type after `:`" live in one place and the runtime needs one
lookup table.

Usage:
    python3 scripts/emoji_porter.py \
        --mozc /path/to/google/mozc \
        --emoji-test /path/to/emoji-test.txt \
        --out karukan-engine/data/emoji.yml
"""

from __future__ import annotations

import argparse
import re
import sys
import unicodedata
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable


HIRAGANA_RE = re.compile(r"^[ぁ-ゟーー]+$")
"""Pure-hiragana reading (plus U+30FC prolonged-sound mark).

Mozc readings occasionally include ASCII variants like `1` or `#`;
those are not what an IME user would type to reach an emoji, so we
drop them.
"""

SNAKE_RE = re.compile(r"[^a-z0-9]+")


# Manual Slack-style aliases.
#
# CLDR names like `grinning_face_with_smiling_eyes` are long and don't
# match how people actually type `:smile`. This table lets popular
# emojis surface from short aliases. Keep entries short — the
# rewriter does first-char-anchored subsequence matching, so anything
# longer than the alias still typed in order will match
# (`:smle` → `smile`).
#
# Keys are emoji characters (NFC-normalized); values are lists of
# alias shortcodes. The porter merges these into per-entry `triggers:`
# so the runtime only consults one lookup table.
MANUAL_ALIASES: dict[str, list[str]] = {
    "😀": ["grinning"],
    "😃": ["smiley"],
    "😄": ["smile", "happy"],
    "😁": ["grin"],
    "😆": ["laughing", "satisfied"],
    "😅": ["sweat_smile"],
    "🤣": ["rofl"],
    "😂": ["joy", "lol"],
    "🙂": ["slightly_smiling_face"],
    "🙃": ["upside_down"],
    "😉": ["wink"],
    "😊": ["blush"],
    "😇": ["innocent"],
    "🥰": ["smiling_face_with_hearts"],
    "😍": ["heart_eyes"],
    "🤩": ["star_struck"],
    "😘": ["kissing_heart"],
    "😗": ["kissing"],
    "🥲": ["smiling_face_with_tear"],
    "😋": ["yum"],
    "😛": ["stuck_out_tongue"],
    "😜": ["stuck_out_tongue_winking_eye"],
    "🤪": ["zany_face"],
    "😎": ["sunglasses", "cool"],
    "🤓": ["nerd"],
    "🥸": ["disguised_face"],
    "🤔": ["thinking"],
    "😐": ["neutral_face"],
    "😶": ["no_mouth"],
    "😏": ["smirk"],
    "😒": ["unamused"],
    "🙄": ["rolling_eyes"],
    "😬": ["grimacing"],
    "🤥": ["lying_face"],
    "😪": ["sleepy"],
    "😴": ["sleeping"],
    "🤤": ["drooling_face"],
    "😷": ["mask"],
    "🤒": ["thermometer_face"],
    "🤕": ["head_bandage"],
    "🤢": ["nauseated_face"],
    "🤮": ["vomiting_face"],
    "🤧": ["sneezing_face"],
    "🥵": ["hot_face"],
    "🥶": ["cold_face"],
    "😵": ["dizzy_face"],
    "🤯": ["exploding_head", "mind_blown"],
    "🤠": ["cowboy"],
    "🥳": ["partying_face", "party"],
    "🥴": ["woozy"],
    "🤐": ["zipper_mouth"],
    "😈": ["smiling_imp"],
    "👿": ["imp"],
    "💀": ["skull"],
    "☠️": ["skull_crossbones"],
    "💩": ["poop", "hankey"],
    "🤡": ["clown"],
    "👻": ["ghost"],
    "👽": ["alien"],
    "👾": ["space_invader"],
    "🤖": ["robot"],
    "😺": ["smiley_cat"],
    "😸": ["smile_cat"],
    "😹": ["joy_cat"],
    "😻": ["heart_eyes_cat"],
    "😼": ["smirk_cat"],
    "😽": ["kissing_cat"],
    "🙀": ["scream_cat"],
    "😿": ["crying_cat"],
    "😾": ["pouting_cat"],
    "❤️": ["heart", "red_heart", "love"],
    "🧡": ["orange_heart"],
    "💛": ["yellow_heart"],
    "💚": ["green_heart"],
    "💙": ["blue_heart"],
    "💜": ["purple_heart"],
    "🤎": ["brown_heart"],
    "🖤": ["black_heart"],
    "🤍": ["white_heart"],
    "💔": ["broken_heart"],
    "❣️": ["heart_exclamation"],
    "💕": ["two_hearts"],
    "💞": ["revolving_hearts"],
    "💓": ["heartbeat"],
    "💗": ["heartpulse"],
    "💖": ["sparkling_heart"],
    "💘": ["cupid"],
    "💝": ["gift_heart"],
    "💟": ["heart_decoration"],
    "👍": ["thumbsup", "+1", "like"],
    "👎": ["thumbsdown", "-1", "dislike"],
    "👌": ["ok_hand"],
    "✌️": ["v"],
    "🤞": ["crossed_fingers"],
    "🤘": ["metal"],
    "🤙": ["call_me"],
    "👈": ["point_left"],
    "👉": ["point_right"],
    "👆": ["point_up_2"],
    "👇": ["point_down"],
    "☝️": ["point_up"],
    "✋": ["hand", "raised_hand"],
    "🤚": ["raised_back_of_hand"],
    "🖐️": ["raised_hand_with_fingers_splayed"],
    "🖖": ["vulcan_salute"],
    "👋": ["wave"],
    "🤝": ["handshake"],
    "🙏": ["pray"],
    "👏": ["clap"],
    "🙌": ["raised_hands"],
    "💪": ["muscle"],
    "🔥": ["fire"],
    "✨": ["sparkles"],
    "⭐": ["star"],
    "🌟": ["star2"],
    "💯": ["100"],
    "✅": ["white_check_mark"],
    "❌": ["x"],
    "⚠️": ["warning"],
    "❓": ["question"],
    "❗": ["exclamation"],
    "🎉": ["tada"],
    "🎊": ["confetti_ball"],
    "🚀": ["rocket"],
    "💡": ["bulb"],
    "📝": ["memo", "note"],
    "📌": ["pushpin"],
    "🐛": ["bug"],
    "🎯": ["dart"],
    "👀": ["eyes"],
    "🤷": ["shrug"],
    "🙋": ["raising_hand"],
}


# Extra hiragana readings to append onto Mozc's table.
#
# Mozc's emoji_data.tsv covers most common readings but misses some
# colloquialisms; e.g. it already has `ぴえん` for 🥺 but lacks similar
# slang for a few other faces. Add entries here when a reading you'd
# expect to surface an emoji is missing — the porter merges these
# into the auto-generated `entries:` table, deduplicated against the
# upstream readings.
MANUAL_READINGS: dict[str, list[str]] = {
    # Placeholder example; populate as needed when Mozc misses a reading.
}


# Hiragana-mora → list of romaji forms. Each entry covers both
# Hepburn (`shi`, `cha`, `tsu`, `fu`, `ji`, `ja`) and Kunrei
# (`si`, `tya`, `tu`, `hu`, `zi`, `zya`) when they differ — both
# spellings are kept as triggers so users get the same emoji
# regardless of which romanization their fingers are used to.
#
# The table is intentionally hand-maintained rather than inverted
# from Mozc's `romanji-hiragana.tsv` because:
#   * Mozc's TSV also lists ASCII-shortcut spellings (`la`/`xa` for
#     small kana, `whu` for う) that the average user would never
#     type to surface an emoji — including them would just inflate
#     the trigger list.
#   * The set of disagreements between Hepburn and Kunrei is small
#     and stable; spelling them out here makes it obvious what we
#     consider a "natural" trigger.
HIRAGANA_TO_ROMAJI: dict[str, list[str]] = {
    # Vowels
    "あ": ["a"], "い": ["i"], "う": ["u"], "え": ["e"], "お": ["o"],
    # K row
    "か": ["ka"], "き": ["ki"], "く": ["ku"], "け": ["ke"], "こ": ["ko"],
    "きゃ": ["kya"], "きゅ": ["kyu"], "きょ": ["kyo"],
    # S row (Hepburn vs Kunrei split on し and friends)
    "さ": ["sa"], "し": ["shi", "si"], "す": ["su"], "せ": ["se"], "そ": ["so"],
    "しゃ": ["sha", "sya"], "しゅ": ["shu", "syu"], "しょ": ["sho", "syo"],
    # T row (Hepburn vs Kunrei split on ち / つ)
    "た": ["ta"], "ち": ["chi", "ti"], "つ": ["tsu", "tu"],
    "て": ["te"], "と": ["to"],
    "ちゃ": ["cha", "tya"], "ちゅ": ["chu", "tyu"], "ちょ": ["cho", "tyo"],
    # N row
    "な": ["na"], "に": ["ni"], "ぬ": ["nu"], "ね": ["ne"], "の": ["no"],
    "にゃ": ["nya"], "にゅ": ["nyu"], "にょ": ["nyo"],
    # H row (Hepburn vs Kunrei split on ふ)
    "は": ["ha"], "ひ": ["hi"], "ふ": ["fu", "hu"],
    "へ": ["he"], "ほ": ["ho"],
    "ひゃ": ["hya"], "ひゅ": ["hyu"], "ひょ": ["hyo"],
    # M row
    "ま": ["ma"], "み": ["mi"], "む": ["mu"], "め": ["me"], "も": ["mo"],
    "みゃ": ["mya"], "みゅ": ["myu"], "みょ": ["myo"],
    # Y row
    "や": ["ya"], "ゆ": ["yu"], "よ": ["yo"],
    # R row
    "ら": ["ra"], "り": ["ri"], "る": ["ru"], "れ": ["re"], "ろ": ["ro"],
    "りゃ": ["rya"], "りゅ": ["ryu"], "りょ": ["ryo"],
    # W row
    "わ": ["wa"], "ゐ": ["wi"], "ゑ": ["we"], "を": ["wo"],
    # G row (dakuten)
    "が": ["ga"], "ぎ": ["gi"], "ぐ": ["gu"], "げ": ["ge"], "ご": ["go"],
    "ぎゃ": ["gya"], "ぎゅ": ["gyu"], "ぎょ": ["gyo"],
    # Z row (Hepburn vs Kunrei split on じ)
    "ざ": ["za"], "じ": ["ji", "zi"], "ず": ["zu"], "ぜ": ["ze"], "ぞ": ["zo"],
    "じゃ": ["ja", "zya"], "じゅ": ["ju", "zyu"], "じょ": ["jo", "zyo"],
    # D row (Hepburn vs Kunrei split on ぢ/づ)
    "だ": ["da"], "ぢ": ["ji", "di"], "づ": ["zu", "du"],
    "で": ["de"], "ど": ["do"],
    "ぢゃ": ["ja", "dya"], "ぢゅ": ["ju", "dyu"], "ぢょ": ["jo", "dyo"],
    # B row (dakuten)
    "ば": ["ba"], "び": ["bi"], "ぶ": ["bu"], "べ": ["be"], "ぼ": ["bo"],
    "びゃ": ["bya"], "びゅ": ["byu"], "びょ": ["byo"],
    # P row (handakuten)
    "ぱ": ["pa"], "ぴ": ["pi"], "ぷ": ["pu"], "ぺ": ["pe"], "ぽ": ["po"],
    "ぴゃ": ["pya"], "ぴゅ": ["pyu"], "ぴょ": ["pyo"],
    # Small kana — only emitted when they appear alone (not as part
    # of a yōon digraph). Mozc readings rarely contain these.
    "ぁ": ["xa"], "ぃ": ["xi"], "ぅ": ["xu"],
    "ぇ": ["xe"], "ぉ": ["xo"],
    "ゃ": ["xya"], "ゅ": ["xyu"], "ょ": ["xyo"],
}

# Yōon digraphs (small や/ゆ/ょ following a consonant kana) listed
# explicitly so segmentation can prefer the 2-char form before
# falling back to the 1-char kana lookup.
YOON_DIGRAPHS: set[str] = {
    k for k in HIRAGANA_TO_ROMAJI if len(k) == 2
}

# Vowel chars for the prolonged-sound mark (ー) handler.
VOWELS = set("aiueo")

# Cap per reading. The Cartesian-product expansion can blow up on
# readings with many Hepburn/Kunrei split morae *and* multiple ん;
# in practice readings stay under this cap, but we trim to avoid
# pathological cases polluting the trigger list with hundreds of
# nearly-identical strings.
MAX_TRIGGERS_PER_READING = 16


@dataclass
class EmojiEntry:
    char: str
    readings: list[str]
    triggers: list[str] = field(default_factory=list)
    description: str | None = None


def nfc(s: str) -> str:
    return unicodedata.normalize("NFC", s)


def parse_mozc_tsv(path: Path) -> Iterable[EmojiEntry]:
    """Yield `EmojiEntry` rows from Mozc `emoji_data.tsv`.

    Skips rows without an emoji character or hiragana reading; both
    are required for the rewriter to surface a candidate.
    """
    with path.open("r", encoding="utf-8") as f:
        for line in f:
            if not line or line.startswith("#"):
                continue
            cols = line.rstrip("\n").split("\t")
            if len(cols) < 6:
                continue
            char = nfc(cols[1].strip())
            if not char:
                continue
            readings = []
            seen: set[str] = set()
            for r in cols[2].split(" "):
                if r and HIRAGANA_RE.match(r) and r not in seen:
                    seen.add(r)
                    readings.append(r)
            if not readings:
                continue
            description = None
            for token in cols[5].split(" "):
                token = token.strip()
                if token:
                    description = token
                    break
            yield EmojiEntry(char=char, readings=readings, description=description)


def parse_emoji_test(path: Path) -> Iterable[tuple[str, str]]:
    """Yield `(emoji, snake_case_shortcode)` pairs from `emoji-test.txt`.

    Only fully-qualified rows are considered — minimally-qualified
    forms are visual duplicates that resolve to the same character at
    render time.
    """
    with path.open("r", encoding="utf-8") as f:
        for line in f:
            if not line or line.startswith("#"):
                continue
            if "; fully-qualified" not in line:
                continue
            try:
                _, after_hash = line.split("#", 1)
            except ValueError:
                continue
            after_hash = after_hash.strip()
            parts = after_hash.split(None, 2)
            if len(parts) < 3:
                continue
            emoji, _version, name = parts
            shortcode = SNAKE_RE.sub("_", name.lower()).strip("_")
            if not shortcode:
                continue
            yield nfc(emoji), shortcode


def segment_morae(reading: str) -> list[str]:
    """Split a hiragana reading into morae, preferring 2-char yōon."""
    out: list[str] = []
    i = 0
    while i < len(reading):
        if i + 1 < len(reading) and reading[i:i + 2] in YOON_DIGRAPHS:
            out.append(reading[i:i + 2])
            i += 2
        else:
            out.append(reading[i])
            i += 1
    return out


def double_for_sokuon(opt: str) -> list[str]:
    """Return the doubled-consonant form(s) used after a small っ.

    For most consonants the rule is "repeat the first letter" so
    `ko` → `kko`. For `ch*` we also emit the Hepburn `tch*` variant
    (`matcha` is more common than `maccha`) so both spellings hit.
    A vowel-initial `opt` cannot be doubled meaningfully; we emit it
    unchanged.
    """
    if not opt or not opt[0].isalpha() or opt[0] in VOWELS:
        return [opt]
    forms = [opt[0] + opt]
    if opt.startswith("ch"):
        forms.append("t" + opt)
    return forms


def reading_to_triggers(reading: str) -> list[str]:
    """Generate the list of Slack-style trigger strings for a reading.

    Produces every (Hepburn × Kunrei × n/nn) combination by Cartesian
    product. Handles っ by doubling the next mora's leading consonant
    and ー by repeating the last emitted vowel. Output is deduped
    while preserving source order and capped at
    `MAX_TRIGGERS_PER_READING` so pathological readings don't dump
    hundreds of near-duplicates into the trigger table.
    """
    morae = segment_morae(reading)
    accumulators: list[str] = [""]
    pending_double = False

    for idx, mora in enumerate(morae):
        if mora == "っ":
            pending_double = True
            continue
        if mora == "ー":
            # Two romanizations of the prolonged-sound mark:
            #   - vowel repeat: `ke` + `ー` → `kee` (the standard
            #     IME-friendly form, also what `RomajiConverter` emits)
            #   - hyphen passthrough: `ke` + `ー` → `ke-`
            # Both are common in practice (`keeki` vs `ke-ki` for ケーキ);
            # emit both so `:keeki` and `:ke-ki` both reach 🍰.
            new_accs = []
            seen: set[str] = set()
            for acc in accumulators:
                candidates = [acc + "-"]
                if acc and acc[-1] in VOWELS:
                    candidates.insert(0, acc + acc[-1])
                for nxt in candidates:
                    if nxt not in seen:
                        seen.add(nxt)
                        new_accs.append(nxt)
            accumulators = new_accs
            continue

        if mora == "ん":
            # Slack-style mental model: enumerate every romaji string
            # a user might plausibly type to mean this ん.
            #   * `nn` — the unambiguous form a strict romaji parser
            #     would require.
            #   * `n`  — the casual form (works when the next mora is
            #     a consonant other than na-row, but users type it
            #     elsewhere too).
            #   * "" (silent) — only when the next mora is in な-row.
            #     This is the `:kiniku` case: users mentally split
            #     きんにく as ki-n-niku but their fingers type
            #     `kiniku`, letting the leading `n` of `niku` absorb
            #     the ん. Without the silent form `:kiniku` would
            #     miss 💪 even though it's the obvious way to type it.
            options: list[str] = ["n", "nn"]
            next_starts_with_n = False
            if idx + 1 < len(morae):
                next_opts = HIRAGANA_TO_ROMAJI.get(morae[idx + 1], [])
                next_starts_with_n = any(o.startswith("n") for o in next_opts)
            if next_starts_with_n:
                options.append("")
        else:
            options = list(HIRAGANA_TO_ROMAJI.get(mora, []))
            if not options:
                # Unknown mora (e.g. punctuation snuck through) — emit
                # it verbatim so the trigger at least round-trips.
                options = [mora]

        if pending_double:
            doubled: list[str] = []
            seen_d: set[str] = set()
            for opt in options:
                for d in double_for_sokuon(opt):
                    if d not in seen_d:
                        seen_d.add(d)
                        doubled.append(d)
            options = doubled
            pending_double = False

        new_accs = []
        seen2: set[str] = set()
        for acc in accumulators:
            for opt in options:
                combined = acc + opt
                if combined not in seen2:
                    seen2.add(combined)
                    new_accs.append(combined)
        accumulators = new_accs

        if len(accumulators) > MAX_TRIGGERS_PER_READING * 4:
            # Early trim during expansion to bound memory; the final
            # dedup-cap below keeps the output stable.
            accumulators = accumulators[: MAX_TRIGGERS_PER_READING * 4]

    triggers = [a for a in accumulators if a]
    return triggers[:MAX_TRIGGERS_PER_READING]


def build_triggers(
    entry: EmojiEntry,
    cldr_codes: dict[str, str],
) -> list[str]:
    """Assemble the full trigger list for an entry.

    Order is: manual aliases (shortest, most-used) → CLDR snake_case
    name → reading-derived romaji. Source order matters because the
    runtime sorts by tier then by length, and equal-rank entries
    fall back to source order — so we put the shortest/most idiomatic
    ones first.
    """
    triggers: list[str] = []
    seen: set[str] = set()

    def add(code: str) -> None:
        if code and code not in seen:
            seen.add(code)
            triggers.append(code)

    for code in MANUAL_ALIASES.get(entry.char, []):
        add(code)
    if entry.char in cldr_codes:
        add(cldr_codes[entry.char])
    for reading in entry.readings:
        for t in reading_to_triggers(reading):
            add(t)

    return triggers


_INT_LIKE = re.compile(r"^[+-]?\d+$")
_FLOAT_LIKE = re.compile(r"^[+-]?(\d+\.\d*|\.\d+|\d+(\.\d*)?[eE][+-]?\d+)$")


def yaml_escape(value: str) -> str:
    """Render `value` as a safe YAML scalar.

    Quotes anything YAML 1.1 would otherwise interpret as a non-string
    scalar (numbers like `+1`, booleans, null) so it round-trips back
    as the original string.
    """
    if value == "":
        return "''"
    needs_quote = (
        value[0] in "-+?:,[]{}#&*!|>'\"%@`"
        or value[0].isdigit()
        or value.lower() in ("true", "false", "null", "yes", "no", "on", "off", "~")
        or ":" in value
        or "#" in value
        or "\n" in value
        or bool(_INT_LIKE.match(value))
        or bool(_FLOAT_LIKE.match(value))
    )
    if needs_quote:
        escaped = value.replace("'", "''")
        return f"'{escaped}'"
    return value


HEADER = """\
# Derived from Mozc emoji_data.tsv (https://github.com/google/mozc) and
# Unicode UTS #51 emoji-test.txt (https://www.unicode.org/Public/).
# Regenerated by scripts/emoji_porter.py — manual edits in the auto-
# generated sections (descriptions / entries) are overwritten on
# re-port. Add curated short aliases at the top of
# scripts/emoji_porter.py (MANUAL_ALIASES) so they survive re-runs.
# Mozc copyright/license: see /THIRD_PARTY_LICENSES.
"""


def render_yaml(
    descriptions: dict[str, str],
    entries: list[EmojiEntry],
) -> str:
    out: list[str] = [HEADER]
    out.append("descriptions:")
    for char, desc in sorted(descriptions.items()):
        out.append(f"  {yaml_escape(char)}: {yaml_escape(desc)}")
    out.append("entries:")
    for e in entries:
        out.append(f"  - char: {yaml_escape(e.char)}")
        if e.readings:
            out.append("    readings:")
            for r in e.readings:
                out.append(f"      - {yaml_escape(r)}")
        if e.triggers:
            out.append("    triggers:")
            for t in e.triggers:
                out.append(f"      - {yaml_escape(t)}")
        # The per-emoji Japanese description lives in the top-level
        # `descriptions:` map; don't duplicate it per entry.
    out.append("")
    return "\n".join(out)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mozc", required=True, type=Path)
    parser.add_argument("--emoji-test", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()

    tsv = args.mozc / "src" / "data" / "emoji" / "emoji_data.tsv"
    if not tsv.exists():
        print(f"error: {tsv} not found", file=sys.stderr)
        return 1
    if not args.emoji_test.exists():
        print(f"error: {args.emoji_test} not found", file=sys.stderr)
        return 1

    entries = sorted(parse_mozc_tsv(tsv), key=lambda e: e.char)

    # Merge MANUAL_READINGS into the entries table, deduplicating
    # against the upstream readings already present.
    by_char = {e.char: e for e in entries}
    for char, extra in MANUAL_READINGS.items():
        char = nfc(char)
        if char in by_char:
            existing = by_char[char].readings
            for r in extra:
                if r and HIRAGANA_RE.match(r) and r not in existing:
                    existing.append(r)
        else:
            new_entry = EmojiEntry(char=char, readings=list(extra))
            entries.append(new_entry)
            by_char[char] = new_entry
    entries.sort(key=lambda e: e.char)

    # CLDR table: emoji → first-seen snake_case shortcode.
    cldr: dict[str, str] = {}
    for emoji, code in parse_emoji_test(args.emoji_test):
        cldr.setdefault(emoji, code)

    # Inject entries that exist only in CLDR or MANUAL_ALIASES (no Mozc
    # reading) so skin-tone variants and the like are still
    # `:`-reachable.
    extra_chars = (set(cldr) | set(MANUAL_ALIASES)) - set(by_char)
    for char in extra_chars:
        new_entry = EmojiEntry(char=nfc(char), readings=[])
        entries.append(new_entry)
        by_char[new_entry.char] = new_entry
    entries.sort(key=lambda e: e.char)

    # Build triggers per entry.
    for entry in entries:
        entry.triggers = build_triggers(entry, cldr)

    descriptions: dict[str, str] = {
        e.char: e.description for e in entries if e.description
    }

    # Drop entries that have no readings AND no triggers — they're
    # unreachable through either input path and would just bloat the
    # YAML file.
    entries = [e for e in entries if e.readings or e.triggers]

    args.out.write_text(render_yaml(descriptions, entries), encoding="utf-8")
    trigger_total = sum(len(e.triggers) for e in entries)
    print(
        f"wrote {len(entries)} entries, {trigger_total} triggers, "
        f"{len(descriptions)} descriptions to {args.out}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
