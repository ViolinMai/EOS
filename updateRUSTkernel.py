import os
import re
import sys
import shutil
import difflib
import textwrap
import tempfile
from collections import Counter

USAGE = """
================================================================================
MAI ADVANCED SYNC & PATCH MANAGER (Zero-Indent Top-Level Engine v4.0)
================================================================================
Block formats in 'patch.txt'   (every block must end with '### END'):

1) Full file replacement / creation:
   ### FILE: path/to/file.rs
   <full file contents>
   ### END

2) Top-level item operations (.rs and .py only):
   ### ITEM ADD: path/to/file.rs      -> add new item(s); ERROR if it already exists
   ### ITEM FIX: path/to/file.rs      -> replace existing item(s); ERROR if missing
   ### ITEM DELETE: path/to/file.rs   -> delete item(s); body = only the signature line(s)
   ### ITEM: path/to/file.rs          -> legacy upsert (replace if exists, else add)

   * An item = a zero-indent definition (fn struct enum union trait impl mod type
     const static use macro_rules extern / python: def class import var) INCLUDING
     its attributes/decorators and doc comments, up to the next top-level item.
   * Braces/brackets are balanced with a real lexer (strings, chars, raw strings,
     lifetimes and nested comments are ignored), so '}' inside text cannot fool it.
   * One block may contain several items (FIX/ADD). DELETE takes several lines:
         ### ITEM DELETE: src/main.rs
         fn old_helper
         struct OldThing
         use core::fmt::Write;
         ### END
   * FIX keeps the old attributes/docs if your new text has none; if your new
     text has its own attributes/docs they replace the old ones.
   * Item identity = (kind, name); 'pub' / 'unsafe' / 'const fn' etc. are ignored.
     impl blocks are identified by their header (impl<T> Trait for Type).
   * 'use' / 'import' are identified by their exact statement; new ones are
     inserted after the last existing use/import.
   * Indentation is auto-adapted (tabs/spaces, 2/4 width) to the target file.

Safety:
   * Everything is staged in memory first. ANY error -> NOTHING is written and
     patch.txt is kept so you can fix it.
   * After each operation the file is re-parsed and verified: every other item
     must be byte-identical. Otherwise the whole patch is rejected.
   * CRLF/LF, BOM and final newline of each file are preserved. Non UTF-8 files
     are never touched. Originals are saved as <file>.bak.

Usage:
   1. Paste blocks into 'patch.txt'.
   2. python updateRUSTkernel.py            (apply)
      python updateRUSTkernel.py --check    (validate only, write nothing)
   3. If 'patch.txt' is empty, a project dump is written to 'full_project_dump.txt'.
================================================================================
"""

SOURCE_CODE_EXTENSIONS = (
    '.rs', '.py', '.ld', '.conf', '.toml', '.s', '.asm', '.nsh', '.TAG', '.lock', '.ini'
)

EXACT_SOURCE_FILES = {'CMakeLists.txt'}

IGNORED_DIRS = {
    "__pycache__", ".git", "dist", "build", ".venv", "venv",
    "node_modules", ".dart_tool", "ephemeral",
    ".gradle", ".idea", ".vscode", "Pods", ".fvm",
    "data", "downloads", "shared", "debug", "tools", "x86_64-unknown-none"
}

DUMP_FILENAME = "full_project_dump.txt"
PATCH_FILENAME = "patch.txt"


# ==============================================================================
# Project dump (unchanged behaviour)
# ==============================================================================

def is_source_file(filename):
    if filename in {DUMP_FILENAME, PATCH_FILENAME, "index_cache.json", "cert.der", "key.der"}:
        return False
    if filename.endswith(".bak"):
        return False
    if filename in EXACT_SOURCE_FILES:
        return True
    return filename.endswith(SOURCE_CODE_EXTENSIONS)


def get_project_data(startpath='.'):
    source_files = []
    tree_lines = [f"PROJECT SOURCE STRUCTURE: {os.path.basename(os.path.abspath(startpath))}", "-" * 80]

    for root, dirs, files in os.walk(startpath):
        rel_root = os.path.relpath(root, startpath).replace("\\", "/")
        dirs[:] = [d for d in dirs if d not in IGNORED_DIRS and not d.startswith('.')]
        valid_files = [f for f in sorted(files) if is_source_file(f) and f != os.path.basename(__file__)]

        level = 0 if rel_root == '.' else rel_root.count('/') + 1
        folder_name = os.path.basename(os.path.abspath(startpath)) if rel_root == '.' else os.path.basename(root)

        if valid_files or dirs:
            indent = ' ' * 4 * level
            tree_lines.append(f"{indent}[DIR]  {folder_name}/")
            subindent = ' ' * 4 * (level + 1)
            for f in valid_files:
                tree_lines.append(f"{subindent}[FILE] {f}")
                source_files.append(os.path.join(root, f))

    return "\n".join(tree_lines), source_files


def write_full_dump(tree_str, source_files, output_file=DUMP_FILENAME):
    with open(output_file, 'w', encoding='utf-8') as dump:
        dump.write(tree_str + "\n\n")
        dump.write("=" * 80 + "\nFULL SOURCE CODE DUMP\n" + "=" * 80 + "\n\n")

        for fpath in source_files:
            clean_fpath = fpath.replace("\\", "/")
            dump.write(f"\n{'=' * 80}\n--- FILE: {clean_fpath} ---\n{'=' * 80}\n\n")
            try:
                with open(fpath, 'r', encoding='utf-8', errors='replace') as src:
                    dump.write(src.read().strip() + "\n")
            except Exception as e:
                dump.write(f"[ERROR READING FILE]: {e}\n")

    print(f"\n[OK] Source code dump saved to: {output_file}")


def normalize_newlines(text):
    return text.replace('\r\n', '\n').replace('\r', '\n')


# ==============================================================================
# Errors / data types
# ==============================================================================

class PatchError(Exception):
    pass


class Item:
    """A top-level item occupying lines [start, end). `header` is the line of the
    real definition; lines [start, header) are attached attributes/decorators/docs."""
    __slots__ = ("start", "header", "end", "kind", "name")

    def __init__(self, start, header, end, kind, name):
        self.start, self.header, self.end, self.kind, self.name = start, header, end, kind, name

    @property
    def key(self):
        return (self.kind, self.name)

    def label(self):
        s = self.name if self.kind in ('impl', 'use', 'import', 'extern', 'other') else f"{self.kind} {self.name}"
        s = s.strip()
        return s if len(s) <= 70 else s[:67] + "..."


TYPE_KINDS = {'struct', 'enum', 'union', 'type', 'trait'}
VALUE_KINDS = {'const', 'static'}
SEMI_KINDS = {'const', 'static', 'type', 'use', 'extern_crate'}   # continue past '}' until ';'
GROUPED_KINDS = {'use', 'import'}


# ==============================================================================
# Lexers: replace comments / strings with spaces (same length, newlines kept)
# ==============================================================================

_RAW_STR = re.compile(r'b?r(#*)"')


def _blank(out, a, b):
    for k in range(a, b):
        if out[k] != '\n':
            out[k] = ' '


def mask_rust(t):
    n = len(t)
    out = list(t)
    i = 0
    while i < n:
        c = t[i]
        if c == '/' and t.startswith('//', i):
            j = t.find('\n', i)
            j = n if j == -1 else j
            _blank(out, i, j)
            i = j
        elif c == '/' and t.startswith('/*', i):
            depth, j = 1, i + 2
            while j < n and depth > 0:
                if t.startswith('/*', j):
                    depth += 1
                    j += 2
                elif t.startswith('*/', j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            if depth > 0:
                return None
            _blank(out, i, j)
            i = j
        elif c in 'rb' and (i == 0 or not (t[i - 1].isalnum() or t[i - 1] == '_')) and _RAW_STR.match(t, i):
            m = _RAW_STR.match(t, i)
            term = '"' + m.group(1)
            j = t.find(term, m.end())
            if j == -1:
                return None
            _blank(out, i, j + len(term))
            i = j + len(term)
        elif c == '"':
            j = i + 1
            while j < n and t[j] != '"':
                j += 2 if t[j] == '\\' else 1
            if j >= n:
                return None
            _blank(out, i, j + 1)
            i = j + 1
        elif c == "'":
            if t.startswith('\\', i + 1):
                j = t.find("'", i + 3)
                if j != -1 and j - i <= 12:
                    _blank(out, i, j + 1)
                    i = j + 1
                else:
                    i += 1
            elif i + 2 < n and t[i + 2] == "'" and t[i + 1] != "'":
                _blank(out, i, i + 3)
                i += 3
            else:
                i += 1          # lifetime
        else:
            i += 1
    return ''.join(out)


def mask_python(t):
    n = len(t)
    out = list(t)
    i = 0
    while i < n:
        c = t[i]
        if c == '#':
            j = t.find('\n', i)
            j = n if j == -1 else j
            _blank(out, i, j)
            i = j
        elif c in '"\'':
            q = t[i:i + 3]
            if q in ('"""', "'''"):
                j = i + 3
                while j < n and not t.startswith(q, j):
                    j += 2 if t[j] == '\\' else 1
                if j >= n:
                    return None
                end = j + 3
            else:
                j = i + 1
                while j < n and t[j] != c and t[j] != '\n':
                    j += 2 if t[j] == '\\' else 1
                end = j + 1 if (j < n and t[j] == c) else j
            _blank(out, i, min(end, n))
            i = min(end, n)
        else:
            i += 1
    return ''.join(out)


# ==============================================================================
# Rust top-level item parser
# ==============================================================================

_TOK = re.compile(r'[A-Za-z_][A-Za-z0-9_]*!?|\S')
_IDENT = re.compile(r'^[A-Za-z_][A-Za-z0-9_]*$')


def rust_header(m):
    """Classify a (masked) line -> (kind, raw_name) or (None, None)."""
    toks = _TOK.findall(m.strip())
    n, k = len(toks), 0
    if k < n and toks[k] == 'pub':
        k += 1
        if k < n and toks[k] == '(':
            while k < n and toks[k] != ')':
                k += 1
            k += 1
    while k < n:
        t = toks[k]
        if t in ('default', 'async', 'unsafe', 'safe'):
            k += 1
        elif t == 'const' and k + 1 < n and toks[k + 1] in ('fn', 'unsafe', 'async', 'extern'):
            k += 1
        elif t == 'extern':
            if k + 1 < n and toks[k + 1] == 'crate':
                return 'extern_crate', (toks[k + 2] if k + 2 < n else '')
            k += 1
            if k < n and toks[k] == '{':
                return 'extern', ''
        else:
            break
    if k >= n:
        return None, None
    t = toks[k]
    nxt = toks[k + 1] if k + 1 < n else ''
    if t in ('fn', 'struct', 'enum', 'trait', 'mod', 'type', 'union', 'const', 'static'):
        if t == 'static' and nxt == 'mut':
            nxt = toks[k + 2] if k + 2 < n else ''
        if _IDENT.match(nxt):
            return t, nxt
        return None, None
    if t == 'impl':
        return 'impl', ''
    if t == 'use':
        return 'use', ''
    if t == 'macro_rules!':
        return 'macro_rules', nxt
    return 'other', ''


def rust_end(ml, i, kind):
    depth = 0
    for j in range(i, len(ml)):
        for ch in ml[j]:
            if ch in '([{':
                depth += 1
            elif ch in ')]':
                depth -= 1
            elif ch == '}':
                depth -= 1
                if depth == 0 and kind not in SEMI_KINDS:
                    return j
            elif ch == ';' and depth == 0:
                return j
            if depth < 0:
                return None
    return None


def rust_name(kind, name, lines, ml, i, j):
    if kind == 'impl':
        txt = ' '.join(ml[i:j + 1]).split('{')[0]
        txt = re.split(r'\bwhere\b', txt)[0]
        txt = re.sub(r'^\s*unsafe\s+', '', txt)
        return re.sub(r'\s+', '', txt)
    if kind == 'use':
        txt = re.sub(r'\s+', '', ' '.join(ml[i:j + 1]))
        return re.sub(r',(?=[}\)])', '', txt)
    if kind == 'extern':
        return re.sub(r'\s+', '', lines[i].split('{')[0])
    if kind == 'other':
        return re.sub(r'\s+', '', lines[i])
    return name


def parse_rust(lines, attach_comments=False):
    n = len(lines)
    masked = mask_rust('\n'.join(lines))
    if masked is None:
        raise PatchError("unterminated string literal or block comment (cannot parse Rust safely)")
    ml = masked.split('\n')
    items, pending, i = [], None, 0
    while i < n:
        raw, m = lines[i], ml[i]
        s = raw.strip()
        if not s:
            pending = None
            i += 1
            continue
        if raw[0] in ' \t':
            i += 1
            continue
        if not m.strip():                                  # comment-only line
            if s.startswith('///') or s.startswith('/**') or attach_comments:
                if pending is None:
                    pending = i
            else:
                pending = None
            i += 1
            continue
        if s.startswith('#'):
            if s.startswith('#[') or s.startswith('#!['):
                depth, j = 0, i
                while j < n:
                    depth += ml[j].count('[') - ml[j].count(']')
                    if depth <= 0:
                        break
                    j += 1
                if j >= n:
                    raise PatchError(f"line {i + 1}: unbalanced attribute")
                if s.startswith('#!['):
                    pending = None
                elif pending is None:
                    pending = i
                i = j + 1
                continue
            i += 1
            continue
        kind, name = rust_header(m)
        if kind is None:
            kind, name = 'other', ''
        j = rust_end(ml, i, kind)
        if j is None:
            raise PatchError(f"line {i + 1}: cannot find the end of item '{s[:60]}' (unbalanced braces?)")
        name = rust_name(kind, name, lines, ml, i, j)
        items.append(Item(pending if pending is not None else i, i, j + 1, kind, name))
        pending = None
        i = j + 1
    return items


# ==============================================================================
# Python top-level item parser
# ==============================================================================

PY_CONT = ('else', 'elif', 'except', 'finally')


def py_header_name(ml, lines, header, end):
    m = ml[header]
    mm = re.match(r'(?:async\s+)?def\s+(\w+)', m)
    if mm:
        return 'def', mm.group(1)
    mm = re.match(r'class\s+(\w+)', m)
    if mm:
        return 'class', mm.group(1)
    if re.match(r'(import|from)\s', m):
        t = ' '.join(' '.join(ml[header:end]).split())
        t = re.sub(r',\s*\)', ')', t)
        t = re.sub(r'\(\s+', '(', t)
        t = re.sub(r'\s+\)', ')', t)
        return 'import', t
    mm = re.match(r'([A-Za-z_]\w*)\s*(?::[^=]+)?=(?!=)', m)
    if mm:
        return 'var', mm.group(1)
    return 'other', ' '.join(lines[header].split())


def parse_python(lines, attach_comments=False):
    n = len(lines)
    masked = mask_python('\n'.join(lines))
    if masked is None:
        raise PatchError("unterminated triple-quoted string (cannot parse Python safely)")
    ml = masked.split('\n')
    starts, depth, cont = [], 0, False
    for i in range(n):
        m = ml[i]
        if m.strip() and m[0] not in ' \t' and depth == 0 and not cont:
            kw = re.match(r'[A-Za-z_]+', m)
            if not (kw and kw.group(0) in PY_CONT):
                starts.append(i)
        depth += sum(m.count(c) for c in '([{') - sum(m.count(c) for c in ')]}')
        depth = max(depth, 0)
        cont = m.rstrip().endswith('\\')
    items, k = [], 0
    while k < len(starts):
        first, h = starts[k], k
        while ml[starts[h]].lstrip().startswith('@') and h + 1 < len(starts):
            h += 1
        header = starts[h]
        end = starts[h + 1] if h + 1 < len(starts) else n
        while end > header + 1 and (not lines[end - 1].strip() or lines[end - 1].startswith('#')):
            end -= 1
        start = first
        if attach_comments:
            prev_end = items[-1].end if items else 0
            while start - 1 >= prev_end and lines[start - 1].startswith('#'):
                start -= 1
        kind, name = py_header_name(ml, lines, header, end)
        items.append(Item(start, header, end, kind, name))
        k = h + 1
    return items


def parse_items(ext, lines, attach_comments=False):
    if ext == '.rs':
        return parse_rust(lines, attach_comments)
    if ext == '.py':
        return parse_python(lines, attach_comments)
    raise PatchError(f"ITEM operations support only .rs and .py files (got '{ext}'); use '### FILE:' for this file")


# ==============================================================================
# DELETE selectors  (short signatures, e.g. 'fn foo', 'impl A for B', 'class X')
# ==============================================================================

def parse_selectors(ext, body):
    sels = []
    if ext == '.rs':
        masked = mask_rust('\n'.join(body))
        if masked is None:
            raise PatchError("unterminated string/comment in DELETE body")
        ml = masked.split('\n')
    else:
        masked = mask_python('\n'.join(body))
        if masked is None:
            raise PatchError("unterminated string in DELETE body")
        ml = masked.split('\n')
    n, i, attrs = len(body), 0, []
    while i < n:
        raw, s = body[i], body[i].strip()
        if not s:
            attrs = []
            i += 1
            continue
        if raw[0] in ' \t' or not ml[i].strip() or s[0] in '}])':
            i += 1
            continue
        if s.startswith('#[') or s.startswith('@'):
            attrs.append(s)
            i += 1
            continue
        if s.startswith('#'):
            i += 1
            continue
        end = i
        if ext == '.rs':
            kind, name = rust_header(ml[i])
            if kind in (None, 'other'):
                i += 1
                continue
            if kind in ('use', 'extern_crate'):
                while end < n - 1 and ';' not in ml[end]:
                    end += 1
            name = rust_name(kind, name, body, ml, i, end)
        else:
            depth = 0
            while end < n:
                depth += sum(ml[end].count(c) for c in '([{') - sum(ml[end].count(c) for c in ')]}')
                if depth <= 0:
                    break
                end += 1
            kind, name = py_header_name(ml, body, i, end + 1)
            if kind == 'other' and _IDENT.match(s):
                kind, name = 'var', s
            if kind == 'other':
                i += 1
                continue
        cfg = _cfg_norm(attrs) if attrs else None
        sels.append((kind, name, cfg))
        attrs = []
        i = end + 1
    return sels


def _cfg_norm(attr_lines):
    return tuple(re.sub(r'\s+', '', l) for l in attr_lines if 'cfg' in l or l.strip().startswith('@'))


def cfg_sig(lines, it):
    return _cfg_norm(lines[it.start:it.header])


def find_targets(ext, lines, items, kind, name, cfg=None):
    ex = [x for x in items if x.kind == kind and x.name == name]
    if not ex and ext == '.rs':
        for grp in (TYPE_KINDS, VALUE_KINDS):
            if kind in grp:
                ex = [x for x in items if x.kind in grp and x.name == name]
    if len(ex) > 1 and cfg is not None:
        f = [x for x in ex if cfg_sig(lines, x) == cfg]
        if f:
            ex = f
    return ex


def not_found_msg(items, kind, name, rel):
    cands = [x.label() for x in items]
    close = difflib.get_close_matches(f"{kind} {name}", cands, n=3, cutoff=0.5)
    same = [x.label() for x in items if x.name == name and x.kind != kind]
    msg = f"{kind} '{name}' not found in {rel}."
    if same:
        msg += f" Found with another kind: {same}."
    if close:
        msg += f" Similar items: {close}."
    return msg


# ==============================================================================
# Text state, body cleaning, indentation
# ==============================================================================

def load_state(path):
    with open(path, 'rb') as f:
        raw = f.read()
    bom = raw.startswith(b'\xef\xbb\xbf')
    if bom:
        raw = raw[3:]
    try:
        text = raw.decode('utf-8')
    except UnicodeDecodeError as e:
        raise PatchError(f"'{path}' is not valid UTF-8 ({e}); refusing to touch it")
    crlf = text.count('\r\n') * 2 > text.count('\n')
    text = normalize_newlines(text)
    final_nl = text.endswith('\n')
    lines = text.split('\n') if text else []
    if final_nl and lines:
        lines.pop()
    return {'lines': lines, 'final_nl': final_nl or not lines, 'crlf': crlf, 'bom': bom}


def serialize(st):
    s = '\n'.join(st['lines'])
    if st['lines']:
        s += '\n'
    if st['crlf']:
        s = s.replace('\n', '\r\n')
    data = s.encode('utf-8')
    return (b'\xef\xbb\xbf' + data) if st['bom'] else data


def clean_body(body, dedent=True):
    lines = [l.rstrip() for l in body]
    nz = [i for i, l in enumerate(lines) if l.strip()]
    if len(nz) >= 2 and lines[nz[0]].strip().startswith('```') and lines[nz[-1]].strip() == '```':
        lines = lines[nz[0] + 1:nz[-1]]
    while lines and not lines[0].strip():
        lines.pop(0)
    while lines and not lines[-1].strip():
        lines.pop()
    if dedent and lines:
        lines = textwrap.dedent('\n'.join(lines)).split('\n')
    return lines


def detect_indent(lines):
    tabs = sp = 0
    widths = []
    for l in lines:
        if l.startswith('\t'):
            tabs += 1
        elif l.startswith(' '):
            sp += 1
            widths.append(len(l) - len(l.lstrip(' ')))
    style = 'tab' if tabs > sp else 'space'
    w = 4
    if widths:
        for cand in (8, 4, 3, 2):
            if sum(1 for x in widths if x % cand == 0) >= 0.8 * len(widths):
                w = cand
                break
    return style, w


def adapt_indent(new_lines, file_lines, notes):
    if not file_lines or not any(l[:1] in (' ', '\t') for l in new_lines):
        return new_lines
    fs, fw = detect_indent(file_lines)
    ps, pw = detect_indent(new_lines)
    rescale = (fs == 'space' and ps == 'space' and fw != pw and fw in (2, 4) and pw in (2, 4))
    changed = False
    out = []
    for l in new_lines:
        if not l.strip():
            out.append('')
            continue
        lead = len(l) - len(l.lstrip(' \t'))
        ws, rest = l[:lead], l[lead:]
        new_ws = ws
        if fs == 'space':
            if '\t' in ws:
                new_ws = ws.replace('\t', ' ' * fw)
            if rescale and '\t' not in ws and len(ws) % pw == 0:
                new_ws = ' ' * (len(ws) // pw * fw)
        else:
            if ' ' in ws and '\t' not in ws:
                new_ws = '\t' * (len(ws) // fw) + ' ' * (len(ws) % fw)
        changed |= (new_ws != ws)
        out.append(new_ws + rest)
    if changed:
        notes.append(f"indentation adapted to file style ({fs}, width {fw})")
    return out


def is_comment_only(ext, l):
    s = l.lstrip()
    return s.startswith('#') if ext == '.py' else s.startswith(('//', '/*', '*'))


# ==============================================================================
# Item operations (with post-verification)
# ==============================================================================

def _sig(lines, items, skip):
    return [(x.key, tuple(lines[x.start:x.end])) for x in items if x.key not in skip]


def _reparse(ext, lines, rel):
    try:
        return parse_items(ext, lines)
    except PatchError as e:
        raise PatchError(f"result would be malformed ({e}); nothing written")


def collapse_after_delete(lines, pos):
    if pos < len(lines) and not lines[pos].strip() and (pos == 0 or not lines[pos - 1].strip()):
        del lines[pos]
    if pos == 0:
        while lines and not lines[0].strip():
            del lines[0]
    if pos >= len(lines):
        while lines and not lines[-1].strip():
            lines.pop()


def apply_upsert(st, ext, action, body, it, rel, log):
    lines = st['lines']
    items = parse_items(ext, lines)
    new_lines = body[it.start:it.end]
    has_prefix = it.header > it.start
    targets = find_targets(ext, lines, items, it.kind, it.name, cfg_sig(body, it))
    if len(targets) > 1:
        raise PatchError(f"'{it.label()}' is ambiguous ({len(targets)} matches at lines "
                         f"{[x.header + 1 for x in targets]}); include its #[cfg(..)] attribute to disambiguate")
    eff = action
    if eff == 'UPSERT':
        eff = 'FIX' if targets else 'ADD'
    if eff == 'ADD' and targets:
        raise PatchError(f"'{it.label()}' already exists at line {targets[0].header + 1}; use FIX")
    if eff == 'FIX' and not targets:
        raise PatchError(not_found_msg(items, it.kind, it.name, rel) + " (use ADD to create it)")

    if eff == 'FIX':
        t = targets[0]
        rs = t.start if has_prefix else t.header
        if lines[rs:t.end] == new_lines:
            log.append(f"[=] FIX    {it.label()}  (already identical)")
            return
        skip = {t.key, it.key}
        before = _sig(lines, items, skip)
        if has_prefix and ext == '.rs':
            old_docs = [l for l in lines[t.start:t.header] if l.lstrip().startswith('///')]
            if old_docs and not any(l.lstrip().startswith(('///', '/**')) for l in new_lines[:it.header - it.start]):
                new_lines = old_docs + new_lines          # keep existing doc comments
        lines[rs:t.end] = new_lines
        delta = 0
        where = f"{rs + 1}-{rs + len(new_lines)}"
    else:
        skip = {it.key}
        before = _sig(lines, items, skip)
        group = [x for x in items if x.kind == it.kind]
        if it.kind in GROUPED_KINDS and group:
            pos = group[-1].end
            lines[pos:pos] = new_lines
        elif it.kind in GROUPED_KINDS and items:
            pos = items[0].start
            lines[pos:pos] = new_lines + ['']
        else:
            sep = 2 if (ext == '.py' and it.kind in ('def', 'class')) else 1
            last = items[-1] if items else None
            if (ext == '.py' and last is not None and last.kind == 'other'
                    and last.name.replace(' ', '').startswith('if__name__')):
                p0 = last.start                       # keep the __main__ guard last
                while p0 > 0 and not lines[p0 - 1].strip():
                    p0 -= 1
                lines[p0:last.start] = [''] * sep + new_lines + ['', '']
                pos = p0 + sep
            else:
                while lines and not lines[-1].strip():
                    lines.pop()
                if lines:
                    lines.extend([''] * sep)
                pos = len(lines)
                lines.extend(new_lines)
        delta = 1
        where = f"{pos + 1}-{pos + len(new_lines)}"

    after = _reparse(ext, lines, rel)
    if len(after) != len(items) + delta:
        raise PatchError(f"verification failed for '{it.label()}' (item count changed unexpectedly)")
    if not any(x.key == it.key for x in after):
        raise PatchError(f"verification failed for '{it.label()}' (item not found after patch)")
    if _sig(lines, after, skip) != before:
        raise PatchError(f"verification failed for '{it.label()}' (other items were affected)")
    log.append(f"[+] {eff:<6} {it.label()}  ({rel}:{where})")


def apply_delete(st, ext, kind, name, cfg, rel, log):
    lines = st['lines']
    items = parse_items(ext, lines)
    targets = find_targets(ext, lines, items, kind, name, cfg)
    if not targets:
        raise PatchError(not_found_msg(items, kind, name, rel))
    if len(targets) > 1:
        raise PatchError(f"'{kind} {name}' is ambiguous ({len(targets)} matches at lines "
                         f"{[x.header + 1 for x in targets]}); put its #[cfg(..)] line above the selector")
    t = targets[0]
    skip = {t.key}
    before = _sig(lines, items, skip)
    del lines[t.start:t.end]
    collapse_after_delete(lines, t.start)
    after = _reparse(ext, lines, rel)
    if len(after) != len(items) - 1 or _sig(lines, after, skip) != before:
        raise PatchError(f"verification failed deleting '{t.label()}'; nothing written")
    log.append(f"[-] DELETE {t.label()}  ({rel}:{t.start + 1}-{t.end})")


# ==============================================================================
# Patch parsing
# ==============================================================================

HDR = re.compile(r'^###\s*(FILE|ITEM)\b(?:\s+(ADD|DELETE|FIX|UPSERT|REPLACE))?\s*:\s*(.+?)\s*$', re.I)
END = re.compile(r'^###\s*END\b', re.I)


def safe_path(rel):
    p = rel.strip().strip('`"\'').replace('\\', '/')
    while p.startswith('./'):
        p = p[2:]
    p = p.lstrip('/')
    if not p:
        raise PatchError("empty file path")
    norm = os.path.normpath(p)
    if os.path.isabs(norm) or norm.split(os.sep)[0] == '..':
        raise PatchError(f"path escapes the project directory: {rel}")
    return norm


def parse_patch(raw):
    lines = normalize_newlines(raw).split('\n')
    n, i = len(lines), 0
    ops, errors = {}, []
    while i < n:
        m = HDR.match(lines[i].strip())
        if not m:
            i += 1
            continue
        kind, action, rel = m.group(1).upper(), (m.group(2) or '').upper(), m.group(3)
        start = i
        i += 1
        body, closed = [], False
        while i < n:
            s = lines[i].strip()
            if END.match(s):
                closed = True
                i += 1
                break
            if HDR.match(s):
                break
            body.append(lines[i])
            i += 1
        if not closed:
            errors.append(f"line {start + 1}: block for '{rel}' is not closed with '### END'")
            continue
        if kind == 'FILE' and action:
            errors.append(f"line {start + 1}: '### FILE' does not take an action ({action})")
            continue
        if kind == 'ITEM':
            action = {'': 'UPSERT', 'REPLACE': 'FIX'}.get(action, action)
        try:
            path = safe_path(rel)
        except PatchError as e:
            errors.append(f"line {start + 1}: {e}")
            continue
        ops.setdefault(path, []).append({'kind': kind, 'action': action, 'body': body, 'line': start + 1})
    return ops, errors


# ==============================================================================
# Per-file processing
# ==============================================================================

def suggest_files(path):
    base = os.path.basename(path)
    found = []
    for root, dirs, files in os.walk('.'):
        dirs[:] = [d for d in dirs if d not in IGNORED_DIRS and not d.startswith('.')]
        if base in files:
            found.append(os.path.relpath(os.path.join(root, base), '.').replace('\\', '/'))
        if len(found) >= 5:
            break
    return found


def process_file(rel, ops):
    ext = os.path.splitext(rel)[1].lower()
    existed = os.path.exists(rel)
    st = load_state(rel) if existed else None
    log = []
    for op in ops:
        where = f"patch line {op['line']}"
        try:
            if op['kind'] == 'FILE':
                body = clean_body(op['body'], dedent=False)
                if not body:
                    raise PatchError("empty FILE body (refusing to wipe the file)")
                if st is None:
                    st = {'lines': [], 'final_nl': True, 'crlf': False, 'bom': False}
                st['lines'] = body
                log.append(f"[+] FILE   {'replaced' if existed else 'created'}  {rel}")
                continue

            if st is None:
                hint = suggest_files(rel)
                raise PatchError(f"target file '{rel}' does not exist" +
                                 (f" (did you mean {hint}?)" if hint else "") + "; use '### FILE:' to create files")
            if ext not in ('.rs', '.py'):
                parse_items(ext, [])            # raises the proper message
            parse_items(ext, st['lines'])       # file must be well-formed before we touch it
            action = op['action']

            if action == 'DELETE':
                body = clean_body(op['body'], dedent=True)
                sels = parse_selectors(ext, body)
                if not sels:
                    raise PatchError("DELETE body has no recognizable signature (e.g. 'fn name', 'struct Name')")
                for kind, name, cfg in sels:
                    apply_delete(st, ext, kind, name, cfg, rel, log)
            else:
                body = clean_body(op['body'], dedent=True)
                if not body:
                    raise PatchError("empty ITEM body")
                notes = []
                body = adapt_indent(body, st['lines'], notes)
                new_items = parse_items(ext, body, attach_comments=True)
                if not new_items:
                    raise PatchError("no recognizable top-level item in the body")
                covered = set()
                for x in new_items:
                    covered.update(range(x.start, x.end))
                for idx, l in enumerate(body):
                    if idx not in covered and l.strip() and not is_comment_only(ext, l):
                        raise PatchError(f"body line {idx + 1} is outside any complete top-level item: "
                                         f"{l.strip()[:60]!r} (items must start at column 0 and be complete)")
                seen = set()
                for x in new_items:
                    if x.key in seen:
                        raise PatchError(f"'{x.label()}' appears twice in the same block")
                    seen.add(x.key)
                for x in new_items:
                    apply_upsert(st, ext, action, body, x, rel, log)
                log.extend(f"    note: {nt}" for nt in notes)
        except PatchError as e:
            raise PatchError(f"{where}: {e}")
    return st, log, existed


# ==============================================================================
# Main
# ==============================================================================

def write_staged(staged):
    written = []
    try:
        for path, (st, existed) in staged.items():
            d = os.path.dirname(path)
            if d:
                os.makedirs(d, exist_ok=True)
            if existed:
                shutil.copyfile(path, path + ".bak")
            fd, tmp = tempfile.mkstemp(dir=d or '.', prefix='.patch_tmp_')
            try:
                with os.fdopen(fd, 'wb') as f:
                    f.write(serialize(st))
                if existed:
                    shutil.copymode(path, tmp)
                os.replace(tmp, path)
            finally:
                if os.path.exists(tmp):
                    os.remove(tmp)
            written.append((path, existed))
    except Exception as e:
        print(f"\n[ERR] Write failed ({e}); rolling back...")
        for path, existed in reversed(written):
            try:
                if existed:
                    shutil.copyfile(path + ".bak", path)
                else:
                    os.remove(path)
            except Exception:
                pass
        return False
    return True


def apply_patch(dry_run=False):
    print(USAGE)

    raw_patch = ""
    if os.path.exists(PATCH_FILENAME):
        with open(PATCH_FILENAME, 'r', encoding='utf-8-sig', errors='replace') as f:
            raw_patch = f.read()

    if not raw_patch.strip():
        print("[!] 'patch.txt' is empty. Extracting latest code dump...")
        tree_str, source_files = get_project_data('.')
        print("\n" + tree_str)
        write_full_dump(tree_str, source_files)
        return

    print("[*] Parsing patch and verifying changes...")
    ops_map, parse_errors = parse_patch(raw_patch)
    if parse_errors:
        print("\n[ERR] Patch parsing failed:")
        for err in parse_errors:
            print(f"  - {err}")
        print("\nNothing was written. 'patch.txt' was kept.")
        return
    if not ops_map:
        print("\n[ERR] No valid '### FILE:' / '### ITEM:' blocks found in patch.txt. Nothing written.")
        return

    staged, errors = {}, []
    for rel, ops in ops_map.items():
        try:
            st, log, existed = process_file(rel, ops)
            staged[rel] = (st, existed)
            for line in log:
                print("  " + line)
        except PatchError as e:
            errors.append((rel, str(e)))

    if errors:
        print("\n[ERR] Patch rejected:")
        for rel, msg in errors:
            print(f"  - {rel}: {msg}")
        print("\nNothing was written. 'patch.txt' was kept so you can fix it.")
        return

    if dry_run:
        print(f"\n[OK] --check: patch is valid for {len(staged)} file(s). Nothing was written.")
        return

    if not write_staged(staged):
        print("Nothing was changed. 'patch.txt' was kept.")
        return

    for rel in staged:
        print(f"  [+] Patched: {rel}")
    shutil.copyfile(PATCH_FILENAME, PATCH_FILENAME + ".bak")
    with open(PATCH_FILENAME, 'w', encoding='utf-8') as f:
        f.write("")
    print(f"\n[OK] Successfully applied patch to {len(staged)} file(s).")

    tree_str, source_files = get_project_data('.')
    write_full_dump(tree_str, source_files)


if __name__ == "__main__":
    apply_patch(dry_run=('--check' in sys.argv or '--dry' in sys.argv))
