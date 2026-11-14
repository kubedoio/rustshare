#!/usr/bin/env python3
"""Compare immutable observer data across restore, allowing relay tombstone replay."""

import json
import sys


before_rows = json.load(open(sys.argv[1], encoding="utf-8"))
after_rows = json.load(open(sys.argv[2], encoding="utf-8"))
before = {row["event_id"]: row for row in before_rows}
after = {row["event_id"]: row for row in after_rows}
if before.keys() != after.keys():
    raise SystemExit(
        f"Observer event IDs changed: before={len(before)} after={len(after)}"
    )

mutable = {"event_type", "active", "observed_at"}
promotions = 0
timestamps = 0
for event_id, old in before.items():
    new = after[event_id]
    if {key: value for key, value in old.items() if key not in mutable} != {
        key: value for key, value in new.items() if key not in mutable
    }:
        raise SystemExit("Immutable observer event data changed across restore")
    if old["observed_at"] != new["observed_at"]:
        timestamps += 1
    previous = (old["event_type"], old["active"])
    current = (new["event_type"], new["active"])
    if previous == current:
        continue
    if previous in (("created", True), ("edited", True)) and current == (
        "deleted",
        False,
    ):
        promotions += 1
        continue
    raise SystemExit("Observer event state changed outside monotonic tombstone replay")

print(
    f"Observer restore preserved {len(before)} event IDs and immutable fields; "
    f"tombstone promotions={promotions}, refreshed timestamps={timestamps}."
)
