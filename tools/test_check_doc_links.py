#!/usr/bin/env python3
"""Unit tests for the documentation link checker: python3 -m unittest tools/test_check_doc_links.py"""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_doc_links as checker


class Slugs(unittest.TestCase):
    def test_github_anchor_rules(self):
        cases = {
            '3.3 v0.7.0 delivery record — 2026-10-14': '33-v070-delivery-record--2026-10-14',
            'PLY — Player, simulation and controls': 'ply--player-simulation-and-controls',
            'v0.12 scope: building materials': 'v012-scope-building-materials',
            'The `materials` scene': 'the-materials-scene',
            'See [the guide](guide.md) & more': 'see-the-guide--more',
            'snake_case Names': 'snake_case-names',
            '1920 × 1080': '1920--1080',
            'Café crème': 'café-crème',
        }
        for text, slug in cases.items():
            self.assertEqual(checker.slugify(text), slug, text)

    def test_duplicates_are_numbered_and_explicit_anchors_count(self):
        lines = ['# Notes', '## Gaps', '## Gaps', '### Gaps ##', '<a id="Custom-Spot"></a>',
                 '```', '## Not a heading', '```']
        self.assertEqual(checker.anchors(lines),
                         {'notes', 'gaps', 'gaps-1', 'gaps-2', 'custom-spot'})


class Links(unittest.TestCase):
    def write(self, root, files):
        for name, text in files.items():
            path = Path(root) / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding='utf-8')

    def test_reports_missing_files_and_anchors(self):
        with tempfile.TemporaryDirectory() as root:
            self.write(root, {
                'a.md': '# Top\n\n## Section two\n\n[ok](#section-two) [bad](#nowhere)\n'
                        '[file](docs/b.md#detail) [missing](docs/c.md) [gone](docs/b.md#gone)\n'
                        '[dir](docs/) [code](src/x.rs#L4) [up](../outside.md)\n',
                'docs/b.md': '## Detail\n[back](../a.md#top) [ref]\n\n[ref]: ../a.md#absent\n',
                'src/x.rs': '',
            })
            broken = checker.check(root, ['a.md', 'docs/b.md'])
        self.assertEqual(broken, [
            'a.md:5: #nowhere',
            'a.md:6: docs/c.md',
            'a.md:6: docs/b.md#gone',
            'a.md:7: ../outside.md',
            'docs/b.md:4: ../a.md#absent',
        ])

    def test_ignores_external_and_code(self):
        with tempfile.TemporaryDirectory() as root:
            self.write(root, {'a.md': '[web](https://example.com/x.md) [mail](mailto:a@b.c)\n'
                                      '`[inline](missing.md)`\n'
                                      '```md\n[fenced](missing.md)\n```\n'
                                      '~~~\n[tilde](missing.md)\n~~~\n'
                                      '<a href="#top">top</a> <img src="missing.png">\n'
                                      '# Top\n'})
            broken = checker.check(root, ['a.md'])
        self.assertEqual(broken, ['a.md:9: missing.png'])

    def test_link_text_may_span_lines_and_files_limit_presence(self):
        with tempfile.TemporaryDirectory() as root:
            self.write(root, {'a.md': '[a long\nlink](b.md "title") [c](c.md)\n',
                              'b.md': '', 'c.md': ''})
            self.assertEqual(checker.check(root, ['a.md']), [])
            self.assertEqual(checker.check(root, ['a.md'], files={'a.md', 'b.md'}),
                             ['a.md:2: c.md'])


if __name__ == '__main__':
    unittest.main()
