#!/usr/bin/env python3
"""Check relative links and #anchors in every tracked Markdown file.

Usage: python3 tools/check_doc_links.py [FILE.md ...]

Each relative link must name a file or directory in the repository, and each
`#anchor` (same-file or in another Markdown file) must match a heading, using
GitHub's anchor rules, or an explicit `<a id>`/`name` anchor. External URLs
and links inside fenced code blocks or inline code are ignored. Broken links
are printed as `file:line: target`; the exit status is 1 if there are any.
"""
import html
from pathlib import Path
import re
import subprocess
import sys
import unicodedata
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]

FENCE = re.compile(r'^\s*(`{3,}|~{3,})')
HEADING = re.compile(r'^ {0,3}(#{1,6})(?:[ \t]+(.*?))?[ \t]*$')
INLINE_CODE = re.compile(r'(`+)(.+?)\1')
# `](target)` or `](target "title")`; the link text may start on an earlier line.
INLINE_LINK = re.compile(r'\]\(\s*(<[^>\n]*>|[^)\s]+)(?:\s+(?:"[^"]*"|\'[^\']*\'))?\s*\)')
REFERENCE_DEF = re.compile(r'^ {0,3}\[[^\]]+\]:\s*(<[^>]*>|\S+)')
HTML_LINK = re.compile(r'<(?:a|img)\b[^>]*?\b(?:href|src)\s*=\s*["\']([^"\']*)["\']', re.I)
HTML_ANCHOR = re.compile(r'<[a-z][^>]*?\b(?:id|name)\s*=\s*["\']([^"\']+)["\']', re.I)
EXTERNAL = re.compile(r'^(?:[a-z][a-z0-9+.-]*:|//)', re.I)


def slugify(text):
    """GitHub's heading anchor: rendered text, lowercased, punctuation dropped, spaces to hyphens."""
    text = re.sub(r'<[^>]*>', '', text)                      # HTML tags
    text = re.sub(r'!?\[([^\]]*)\]\([^)]*\)', r'\1', text)   # links and images keep their text
    text = INLINE_CODE.sub(lambda m: m.group(2), text)
    text = html.unescape(text).strip().lower()
    kept = ''.join(c for c in text if c in ' -_' or unicodedata.category(c)[0] in 'LNM')
    return kept.replace(' ', '-')


def outside_fences(lines):
    """Yield (line number, line) for lines outside fenced code blocks."""
    fence = None
    for number, line in enumerate(lines, 1):
        match = FENCE.match(line)
        if fence:
            closing = match and match.group(1)[0] == fence[0] and len(match.group(1)) >= len(fence)
            if closing and not line.strip()[len(match.group(1)):].strip():
                fence = None
        elif match:
            fence = match.group(1)
        else:
            yield number, line


def code_free_lines(lines):
    """Lines outside fenced code, with inline code spans blanked."""
    for number, line in outside_fences(lines):
        yield number, INLINE_CODE.sub(lambda m: ' ' * len(m.group(0)), line)


def anchors(lines):
    """Every anchor a Markdown file defines: headings (duplicates numbered) and explicit ids."""
    found, seen = set(), {}
    for number, line in outside_fences(lines):
        blanked = INLINE_CODE.sub(lambda m: ' ' * len(m.group(0)), line)
        found.update(a.lower() for a in HTML_ANCHOR.findall(blanked))
        heading = HEADING.match(line)
        if not heading:
            continue
        text = re.sub(r'(^|[ \t]+)#+$', '', heading.group(2) or '')
        slug = slugify(text)
        count = seen.get(slug, 0)
        seen[slug] = count + 1
        found.add(slug if count == 0 else f'{slug}-{count}')
    return found


def links(lines):
    """(line number, target) for every link outside code."""
    for number, line in code_free_lines(lines):
        for match in INLINE_LINK.finditer(line):
            yield number, match.group(1).strip('<>')
        definition = REFERENCE_DEF.match(line)
        if definition:
            yield number, definition.group(1).strip('<>')
        for target in HTML_LINK.findall(line):
            yield number, target


def repository_files(root):
    """Paths git would commit: tracked plus untracked-but-not-ignored, that exist on disk."""
    out = subprocess.run(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'],
                         cwd=root, check=True, capture_output=True).stdout.decode()
    return {p for p in out.split('\0') if p and (root / p).exists()}


def check(root, markdown, files=None):
    """Broken links as 'file:line: target'. `files` limits which paths count as present."""
    root = Path(root).resolve()
    directories = None
    if files is not None:
        directories = {parent.as_posix() for p in files for parent in Path(p).parents}
    cache = {}

    def exists(relative):
        if files is None:
            return (root / relative).exists()
        return relative in files or relative in directories

    def anchors_of(relative):
        if relative not in cache:
            cache[relative] = anchors((root / relative).read_text(encoding='utf-8').splitlines())
        return cache[relative]

    broken = []
    for source in sorted(markdown):
        lines = (root / source).read_text(encoding='utf-8').splitlines()
        for number, target in links(lines):
            if not target or EXTERNAL.match(target):
                continue
            path, _, anchor = target.partition('#')
            path = unquote(path)
            if path:
                base = root if path.startswith('/') else (root / source).parent
                resolved = (base / path.lstrip('/')).resolve()
                try:
                    relative = resolved.relative_to(root).as_posix()
                except ValueError:
                    broken.append(f'{source}:{number}: {target}')
                    continue
            else:
                relative = source
            if relative != '.' and not exists(relative):
                broken.append(f'{source}:{number}: {target}')
                continue
            if anchor and relative.endswith('.md') and (root / relative).is_file():
                if unquote(anchor).lower() not in anchors_of(relative):
                    broken.append(f'{source}:{number}: {target}')
    return broken


def main(argv):
    files = repository_files(ROOT)
    markdown = argv or sorted(p for p in files if p.endswith('.md'))
    broken = check(ROOT, markdown, files)
    for line in broken:
        print(line)
    print(f'{len(broken)} broken link(s) in {len(markdown)} Markdown file(s)', file=sys.stderr)
    return 1 if broken else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
