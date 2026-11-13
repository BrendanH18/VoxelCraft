#!/usr/bin/env python3
"""Unit tests for the campaign ledger tool: python3 -m unittest tools/test_campaign_ledger.py"""
import contextlib
import io
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import campaign_ledger as ledger

HEADER = 'kind\tname\tevidence\n'


class Ledger(unittest.TestCase):
    def read(self, files):
        with tempfile.TemporaryDirectory() as root:
            for name, text in files.items():
                (Path(root) / name).write_text(text, encoding='utf-8')
            return ledger.read_ledger(root)

    def test_totals_by_fragment_and_kind(self):
        fragments, totals, problems = self.read({
            'ITEM-BUNDLES.tsv': HEADER
            + 'content\titem:bundle\ttests/bundles.rs::stores_one_stack\n'
            + '# a comment between rows\n'
            + 'behaviour\tbundle weight limit\ttests/bundles.rs::weight\n',
            'GAP-FIRE.tsv': HEADER + 'fix\tfire ticks are saved\ttests/fire.rs::saved\n',
        })
        self.assertEqual(problems, [])
        self.assertEqual(fragments['ITEM-BUNDLES'], {'content': 1, 'behaviour': 1, 'fix': 0})
        self.assertEqual(fragments['GAP-FIRE'], {'content': 0, 'behaviour': 0, 'fix': 1})
        self.assertEqual(totals, {'content': 1, 'behaviour': 1, 'fix': 1})

    def test_a_name_counts_once_across_fragments_and_kinds(self):
        _, totals, problems = self.read({
            'A.tsv': HEADER + 'content\titem:spyglass\ttests/a.rs::one\n',
            'B.tsv': HEADER + 'behaviour\tItem:Spyglass\ttests/b.rs::two\n',
        })
        self.assertEqual(sum(totals.values()), 1)
        self.assertEqual(len(problems), 1)
        self.assertIn('already counted in A.tsv:2', problems[0])

    def test_rows_need_a_known_kind_a_name_and_evidence(self):
        _, totals, problems = self.read({
            'A.tsv': HEADER
            + 'feature\titem:x\ttests/a.rs::x\n'
            + 'content\t\ttests/a.rs::y\n'
            + 'content\titem:z\t \n'
            + 'content\titem:w\n',
        })
        self.assertEqual(sum(totals.values()), 0)
        self.assertEqual(len(problems), 4)
        self.assertIn('unknown kind "feature"', problems[0])
        self.assertIn('no name', problems[1])
        self.assertIn('"item:z" has no evidence', problems[2])
        self.assertIn('expected 3 tab-separated fields, found 2', problems[3])

    def test_the_header_is_required(self):
        _, totals, problems = self.read({'A.tsv': 'content\titem:x\ttests/a.rs::x\n'})
        self.assertEqual(sum(totals.values()), 0)
        self.assertIn('first line must be', problems[0])

    def test_a_missing_folder_is_an_empty_ledger(self):
        fragments, totals, problems = ledger.read_ledger(Path(tempfile.gettempdir()) / 'no-such-ledger-folder')
        self.assertEqual((fragments, sum(totals.values()), problems), ({}, 0, []))

    def test_exit_status_reports_problems(self):
        out, err = io.StringIO(), io.StringIO()
        with tempfile.TemporaryDirectory() as root, contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            (Path(root) / 'A.tsv').write_text(HEADER + 'content\titem:x\ttests/a.rs::x\n', encoding='utf-8')
            self.assertEqual(ledger.main(['--dir', root, '--json']), 0)
            (Path(root) / 'B.tsv').write_text(HEADER + 'content\titem:x\ttests/b.rs::x\n', encoding='utf-8')
            self.assertEqual(ledger.main(['--dir', root]), 1)
        self.assertIn('"total": 1', out.getvalue())
        self.assertIn('already counted in A.tsv:2', err.getvalue())


if __name__ == '__main__':
    unittest.main()
