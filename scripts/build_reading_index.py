#!/usr/bin/env python3
"""Build reading index from mozc OSS dictionary files.

Usage:
    python3 scripts/build_reading_index.py \
        --mozc-dir vendor/mozc_oss \
        --output vendor/mozc_oss/reading_index.pkl
"""

from __future__ import annotations

import argparse
import pickle
from collections import defaultdict
from pathlib import Path


def build_index(mozc_dir: Path) -> dict[str, list[tuple[str, int]]]:
    index: dict[str, list[tuple[str, int]]] = defaultdict(list)
    total = 0
    for i in range(10):
        dict_file = mozc_dir / f"dictionary0{i}.txt"
        if not dict_file.exists():
            print(f"[WARN] {dict_file} not found, skipping")
            continue
        with open(dict_file, encoding="utf-8") as f:
            for line in f:
                parts = line.rstrip("\n").split("\t")
                if len(parts) < 5:
                    continue
                reading, _lid, _rid, cost, surface = parts[:5]
                try:
                    index[reading].append((surface, int(cost)))
                    total += 1
                except ValueError:
                    continue
    print(f"[INFO] Total entries: {total}")
    print(f"[INFO] Unique readings: {len(index)}")
    return dict(index)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mozc-dir", type=Path, default=Path("vendor/mozc_oss"))
    parser.add_argument("--output", type=Path, default=Path("vendor/mozc_oss/reading_index.pkl"))
    args = parser.parse_args()

    if not args.mozc_dir.exists():
        print(f"[ERROR] mozc dir not found: {args.mozc_dir}")
        raise SystemExit(1)

    index = build_index(args.mozc_dir)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with open(args.output, "wb") as f:
        pickle.dump(index, f, protocol=pickle.HIGHEST_PROTOCOL)

    size_mb = args.output.stat().st_size / (1024 * 1024)
    print(f"[INFO] Written to {args.output} ({size_mb:.1f} MB)")


if __name__ == "__main__":
    main()
