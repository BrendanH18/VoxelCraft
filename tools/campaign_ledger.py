#!/usr/bin/env python3
"""Validate and total the build-5000 campaign ledger.

Usage: python3 tools/campaign_ledger.py [--dir DIR] [--json]

The campaign that began on 2026-11-12 counts every feature or improvement it
delivers. Each slice of work adds one fragment,
`docs/verification/2026-11-12-build-5000/ledger/<SLICE>.tsv`, with a header
line and one tab-separated row per delivered item:

    kind<TAB>name<TAB>evidence

- `kind` is `content` (a block, item, mob, effect, recipe, trade, loot
  table, structure piece, screen, command, option or caption), `behaviour`
  (one Java behaviour implemented or corrected) or `fix` (one defect).
- `name` identifies the item, such as `item:black_bundle`. A name may appear
  once in the whole ledger, whatever its kind, so nothing is counted twice.
- `evidence` names the test or fixture that proves it, such as
  `tests/java_trades.rs::every_offer_matches_java`.

Lines starting with `#` are comments. The exit status is 1 if any fragment
is malformed, an item lacks evidence or a name repeats; otherwise the totals
are printed, per fragment and by kind.
"""
import argparse
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_DIR = ROOT / 'docs' / 'verification' / '2026-11-12-build-5000' / 'ledger'
HEADER = ['kind', 'name', 'evidence']
KINDS = ('content', 'behaviour', 'fix')


def read_fragment(path):
    """Return (rows, problems) for one fragment; rows are (kind, name, evidence, line number)."""
    rows, problems = [], []
    try:
        lines = path.read_text(encoding='utf-8').split('\n')
    except (OSError, UnicodeDecodeError) as error:
        return rows, [f'{path.name}: unreadable ({error})']
    if lines and lines[-1] == '':
        lines.pop()
    body = [(number, line) for number, line in enumerate(lines, 1) if not line.startswith('#')]
    if not body or body[0][1].split('\t') != HEADER:
        return rows, [f'{path.name}:1: the first line must be "kind<TAB>name<TAB>evidence"']
    for number, line in body[1:]:
        fields = line.split('\t')
        if len(fields) != 3:
            problems.append(f'{path.name}:{number}: expected 3 tab-separated fields, found {len(fields)}')
            continue
        kind, name, evidence = (field.strip() for field in fields)
        if kind not in KINDS:
            problems.append(f'{path.name}:{number}: unknown kind "{kind}" (use {", ".join(KINDS)})')
        elif not name:
            problems.append(f'{path.name}:{number}: the item has no name')
        elif not evidence:
            problems.append(f'{path.name}:{number}: "{name}" has no evidence')
        else:
            rows.append((kind, name, evidence, number))
    return rows, problems


def read_ledger(directory):
    """Return (per-fragment counts, totals by kind, problems) for every fragment in `directory`."""
    fragments, totals, problems, seen = {}, dict.fromkeys(KINDS, 0), [], {}
    for path in sorted(Path(directory).glob('*.tsv')):
        rows, found = read_fragment(path)
        problems.extend(found)
        counts = dict.fromkeys(KINDS, 0)
        for kind, name, _evidence, number in rows:
            key = name.casefold()
            if key in seen:
                problems.append(f'{path.name}:{number}: "{name}" is already counted in {seen[key]}')
                continue
            seen[key] = f'{path.name}:{number}'
            counts[kind] += 1
            totals[kind] += 1
        fragments[path.stem] = counts
    return fragments, totals, problems


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--dir', type=Path, default=DEFAULT_DIR, help='ledger folder (default: the tracked one)')
    parser.add_argument('--json', action='store_true', help='print the totals as JSON')
    options = parser.parse_args(argv)
    fragments, totals, problems = read_ledger(options.dir)
    for problem in problems:
        print(problem, file=sys.stderr)
    total = sum(totals.values())
    if options.json:
        print(json.dumps({'fragments': fragments, 'totals': totals, 'total': total}, indent=2))
    else:
        for name, counts in fragments.items():
            print(f'{sum(counts.values()):6d}  {name}')
        print(f'{total:6d}  total ({", ".join(f"{totals[kind]} {kind}" for kind in KINDS)}) '
              f'in {len(fragments)} fragments')
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
