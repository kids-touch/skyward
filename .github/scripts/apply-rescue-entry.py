#!/usr/bin/env python3
"""Move the Data rescue entry from Home to Advanced for the Skyward UI."""
from pathlib import Path
import sys

path = Path("crates/skyward-ui/src/pages.rs")
if not path.is_file():
    sys.exit("ERROR: run this script from the Skyward repository root.")

src = path.read_text(encoding="utf-8")

home_row = '        Row::new(if l == Lang::Hans { "数据救援" } else { "Data rescue" }).icon(ORANGE, Icon::Restore),\n'
home_action = '        Action::Open(Screen::Rescue),\n'

advanced_root_row = '        Row::new(l.pick(texts::root::TITLE)).icon(RED, Icon::Shield).trail(root_value(cx).map_or(Trail::Arrow, Trail::Value)),\n'
advanced_update_row = '        Row::new(l.pick(texts::page::UPDATE)).icon(GREEN, Icon::Layers).trail(Trail::Value(update_value(cx))),\n'
advanced_root_action = '        Action::Open(Screen::Root),\n'
advanced_update_action = '        Action::Open(Screen::Update),\n'
rescue_row = '        Row::new(if l == Lang::Hans { "数据救援" } else { "Data rescue" }).icon(ORANGE, Icon::Restore),\n'
rescue_action = '        Action::Open(Screen::Rescue),\n'

# Safe to run twice: a completed patch is treated as success.
if rescue_row not in src and rescue_action not in src:
    if src.count(home_row) != 1 or src.count(home_action) != 1:
        sys.exit("ERROR: Home entry did not match the expected source; no file changed.")
    if src.count(advanced_root_row) != 1 or src.count(advanced_update_row) != 1:
        sys.exit("ERROR: Advanced tool rows did not match; no file changed.")
    if src.count(advanced_root_action) != 1 or src.count(advanced_update_action) != 1:
        sys.exit("ERROR: Advanced actions did not match; no file changed.")

    src = src.replace(home_row, "", 1)
    src = src.replace(home_action, "", 1)
    src = src.replace(advanced_update_row, rescue_row + advanced_update_row, 1)
    src = src.replace(advanced_update_action, rescue_action + advanced_update_action, 1)
    path.write_text(src, encoding="utf-8")
    print("Applied: Data rescue moved from Home to Advanced.")
elif rescue_row in src and rescue_action in src and home_row not in src and home_action not in src:
    print("Already applied: Data rescue is in Advanced.")
else:
    sys.exit("ERROR: Partial or unexpected patch state; no file changed.")

