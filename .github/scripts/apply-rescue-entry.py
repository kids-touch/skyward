#!/usr/bin/env python3
"""Move the Data rescue entry from Home to Advanced for the Skyward UI."""
from pathlib import Path
import sys

path = Path("crates/skyward-ui/src/pages.rs")
if not path.is_file():
    sys.exit("ERROR: run this script from the Skyward repository root.")

src = path.read_text(encoding="utf-8")

home_start = "pub fn home(cx: &Ctx) -> Frame {"
rescue_start = "/// Find and copy pictures, videos, documents and WeChat data out of /data."
advanced_start = "pub fn advanced(cx: &Ctx) -> Frame {"
relabel_start = "/// Giving the data partition's files the SELinux labels"

home_row = '        Row::new(if l == Lang::Hans { "数据救援" } else { "Data rescue" }).icon(ORANGE, Icon::Restore),\n'
rescue_action = '        Action::Open(Screen::Rescue),\n'
advanced_update_row = '        Row::new(l.pick(texts::page::UPDATE)).icon(GREEN, Icon::Layers).trail(Trail::Value(update_value(cx))),\n'
advanced_update_action = '        Action::Open(Screen::Update),\n'

try:
    h0 = src.index(home_start)
    h1 = src.index(rescue_start, h0)
    a0 = src.index(advanced_start)
    a1 = src.index(relabel_start, a0)
except ValueError:
    sys.exit("ERROR: Could not locate Home/Advanced function boundaries; no file changed.")

home = src[h0:h1]
advanced = src[a0:a1]

# Idempotency is checked within the relevant function bodies, not globally.
if home_row not in home and rescue_action not in home and home_row in advanced and rescue_action in advanced:
    print("Already applied: Data rescue is in Advanced.")
    sys.exit(0)

if home.count(home_row) != 1 or home.count(rescue_action) != 1:
    sys.exit("ERROR: Expected exactly one rescue entry in Home; no file changed.")
if advanced.count(home_row) != 0 or advanced.count(rescue_action) != 0:
    sys.exit("ERROR: Advanced already contains a rescue entry or source is unexpected; no file changed.")
if advanced.count(advanced_update_row) != 1 or advanced.count(advanced_update_action) != 1:
    sys.exit("ERROR: Advanced Update row/action did not match; no file changed.")

home = home.replace(home_row, "", 1).replace(rescue_action, "", 1)
advanced = advanced.replace(advanced_update_row, home_row + advanced_update_row, 1)
advanced = advanced.replace(advanced_update_action, rescue_action + advanced_update_action, 1)

src = src[:h0] + home + src[h1:a0] + advanced + src[a1:]
path.write_text(src, encoding="utf-8")
print("Applied: Data rescue moved from Home to Advanced.")
