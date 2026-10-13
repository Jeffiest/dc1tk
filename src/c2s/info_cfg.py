"""Read a character/pack info.cfg the way the engine's CScriptInterpreter does.

Text is Shift-JIS (cp932).  // and /* */ comments are blanked.  A statement
is ``TAG arg, arg, ...;``.  A tag must start with an upper-case letter or the
engine ignores it (so the ``v2;`` line found first in most files does nothing:
the table entry is ``V2``).  An argument in double quotes is a string; one
containing '.' is a float; otherwise an int (atoi).  Standard library only.

    python -I -X utf8 info_cfg.py <info.cfg>
"""

import sys


def _strip_comments(t):
    out = []
    i, n = 0, len(t)
    while i < n:
        if t.startswith('//', i):
            j = t.find('\n', i)
            i = n if j < 0 else j
        elif t.startswith('/*', i):
            j = t.find('*/', i + 2)
            i = n if j < 0 else j + 2
            out.append(' ')
        else:
            out.append(t[i])
            i += 1
    return ''.join(out)


def _value(a):
    a = a.strip()
    if len(a) >= 2 and a[0] == '"' and a[-1] == '"':
        return a[1:-1].replace('\\"', '"')
    if '.' in a:
        try:
            return float(a)
        except ValueError:
            return None
    try:
        return int(a)
    except ValueError:
        return None


def parse(data):
    """Return a list of (tag, [args]) in file order."""
    if isinstance(data, bytes):
        data = data.decode('cp932', 'replace')
    t = _strip_comments(data)
    out = []
    i, n = 0, len(t)
    while i < n:
        while i < n and t[i] in ' \t\r\n':
            i += 1
        if i >= n:
            break
        j = i
        while j < n and (t[j].isalnum() or t[j] == '_'):
            j += 1
        tag = t[i:j]
        args, cur, q = [], [], False
        k = j
        while k < n:
            c = t[k]
            if c == '"':
                q = not q
            if not q and c in ',;':
                if ''.join(cur).strip():
                    args.append(_value(''.join(cur)))
                cur = []
                if c == ';':
                    break
            else:
                cur.append(c)
            k += 1
        if tag:
            out.append((tag, args))
        i = k + 1
        if not tag:
            i = max(i, j + 1)
    return out


class CharInfo:
    """The parts of a character info.cfg that matter for its model."""

    def __init__(self, stmts):
        self.stmts = stmts
        self.name = None
        self.model = None
        self.shadow_model = None
        self.imgs = []
        self.vertex_anime = []
        self.shape_anime = 0
        self.motions = {}       # motion set id -> dict(mot, bbp, wgt, keys=[...])
        self.shadow_motion = None
        self.lod = []
        cur = None
        for tag, a in stmts:
            if tag == 'NAME' and a:
                self.name = a[0]
            elif tag == 'MODEL' and a:
                self.model = a[0]
            elif tag == 'SHADOW_MODEL' and a:
                self.shadow_model = a[0]
            elif tag == 'IMG' and len(a) >= 2:
                self.imgs.append((a[0], a[1]))
            elif tag == 'VERTEX_ANIME':
                self.vertex_anime.extend(x for x in a if isinstance(x, str))
            elif tag == 'SHAPE_ANIME' and a:
                self.shape_anime = a[0]
            elif tag == 'MOTION' and len(a) >= 4:
                cur = {'mot': a[1], 'bbp': a[2], 'wgt': a[3], 'keys': []}
                self.motions[a[0]] = cur
            elif tag == 'SHADOW_MOTION' and len(a) >= 3:
                self.shadow_motion = {'mot': a[0], 'bbp': a[1], 'wgt': a[2]}
            elif tag == 'KEY' and len(a) >= 4 and cur is not None:
                cur['keys'].append({'name': a[0], 'start': a[1], 'end': a[2], 'step': a[3]})
            elif tag == 'LOD_MODEL' and len(a) >= 5:
                self.lod.append({'level': a[0], 'model': a[1], 'wgt': a[2], 'bbp': a[3], 'distance': a[4]})


def read(path):
    with open(path, 'rb') as f:
        return CharInfo(parse(f.read()))


if __name__ == '__main__':
    for tag, args in parse(open(sys.argv[1], 'rb').read()):
        print(tag, args)
