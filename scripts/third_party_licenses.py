"""Writes the license texts of everything built into parrot-midi: the Rust
crates it uses (from `cargo metadata`) and the fonts they bundle. libmt32emu
(LGPL-2.1+) is covered by THIRD-PARTY.md and LGPL-2.1.txt.

    python3 scripts/third_party_licenses.py [--target TRIPLE] > THIRD-PARTY-LICENSES.txt
"""

import json
import os
import re
import subprocess
import sys

MIT_TERMS = """Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE."""

UNLICENSE = """This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or
distribute this software, either in source code form or as a compiled
binary, for any purpose, commercial or non-commercial, and by any
means.

In jurisdictions that recognize copyright laws, the author or authors
of this software dedicate any and all copyright interest in the
software to the public domain. We make this dedication for the benefit
of the public at large and to the detriment of our heirs and
successors. We intend this dedication to be an overt act of
relinquishment in perpetuity of all present and future rights to this
software under copyright law.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
OTHER DEALINGS IN THE SOFTWARE.

For more information, please refer to <https://unlicense.org>"""

LICENSE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|NOTICE|COPYRIGHT|UNLICENSE|OFL|UFL)", re.I)


def main():
    args = ["cargo", "metadata", "--format-version", "1", "--locked"]
    if "--target" in sys.argv:
        args += ["--filter-platform", sys.argv[sys.argv.index("--target") + 1]]
    meta = json.loads(subprocess.run(args, check=True, capture_output=True, text=True).stdout)
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = meta["resolve"]["root"]

    # Everything reachable from the program through normal dependencies
    # (not build scripts' tools or test-only crates).
    seen, todo = set(), [root]
    while todo:
        node = nodes[todo.pop()]
        for dep in node["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]) and dep["pkg"] not in seen:
                seen.add(dep["pkg"])
                todo.append(dep["pkg"])

    # UTF-8 with plain line ends on every system (Windows' console default
    # can't encode some of the license texts).
    sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    out = sys.stdout
    printed = {}  # license text -> the first crate it was printed for
    out.write("Third-party licenses for parrot-midi\n")
    out.write("=" * 36 + "\n\n")
    out.write("parrot-midi is MIT licensed (LICENSE). It contains Munt's libmt32emu\n")
    out.write("(LGPL-2.1-or-later, see THIRD-PARTY.md and LGPL-2.1.txt) and the Rust\n")
    out.write("crates below, each under the license named with it.\n\n")
    def has_file(pid):
        base = os.path.dirname(packages[pid]["manifest_path"])
        return any(LICENSE_FILE.match(f) for f in os.listdir(base))

    for pid in sorted(seen, key=lambda i: (not has_file(i), packages[i]["name"], packages[i]["version"])):
        p = packages[pid]
        if p["source"] is None:
            continue  # this program itself
        out.write("-" * 72 + "\n")
        out.write(f"{p['name']} {p['version']}\n")
        out.write(f"License: {p.get('license') or 'see below'}\n")
        if p.get("repository"):
            out.write(f"Source: {p['repository']}\n")
        base = os.path.dirname(p["manifest_path"])
        texts = []
        for d in (base, os.path.join(base, "fonts")):
            if not os.path.isdir(d):
                continue
            for f in sorted(os.listdir(d)):
                path = os.path.join(d, f)
                if os.path.isfile(path) and (LICENSE_FILE.match(f) or (d.endswith("fonts") and f.endswith(".txt"))):
                    texts.append(path)
        if not texts:
            # Some crates' packages leave out their license file. Give the
            # standard terms of the license their metadata names.
            spdx = p.get("license") or ""
            authors = ", ".join(p.get("authors") or []) or f"the {p['name']} authors"
            if "MIT" in spdx:
                notice = f"Copyright (c) {authors}\n\nUnder the MIT License:"
                if MIT_TERMS in printed:
                    out.write(f"\n{notice} the same terms as printed for {printed[MIT_TERMS]}\n")
                else:
                    printed[MIT_TERMS] = f"{p['name']} {p['version']}"
                    out.write(f"\n{notice}\n\n{MIT_TERMS}\n")
            elif spdx == "Unlicense":
                out.write(f"\n{UNLICENSE}\n")
            elif "Apache-2.0" in spdx:
                apache = next((w for t, w in printed.items() if "Version 2.0, January 2004" in t[:300]), None)
                where = f", printed in full for {apache}" if apache else ""
                out.write(f"\nBy {authors}, under the Apache License, Version 2.0{where}.\n")
        for path in texts:
            with open(path, encoding="utf-8", errors="replace") as fh:
                text = fh.read().strip()
            name = os.path.relpath(path, base)
            here = f"{p['name']} {p['version']}, {name}"
            if text in printed:
                out.write(f"\n--- {name}: the same text as {printed[text]}\n")
            else:
                printed[text] = here
                out.write(f"\n--- {name}\n\n{text}\n")
        out.write("\n")


if __name__ == "__main__":
    main()
