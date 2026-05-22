#!/usr/bin/env python3
"""Check criterion benchmark results against hard performance budgets.

Parses criterion JSON output from target/criterion/*/estimates.json,
extracts mean.point_estimate (nanoseconds), and exits non-zero if any
budget is exceeded.

Benchmarks whose path contains "startup" are checked against
--startup-max-ms; benchmarks whose path contains "p99" are checked
against --latency-max-p99-us.

Usage:
  check_perf_regression.py --startup-max-ms 50 --latency-max-p99-us 1000
"""

import argparse
import json
import sys
from pathlib import Path


def find_estimates(criterion_dir: Path) -> list[tuple[str, float]]:
    """Return [(relative_path, mean_ns)] for every estimates.json found."""
    results: list[tuple[str, float]] = []
    for estimates_file in criterion_dir.rglob("estimates.json"):
        try:
            data = json.loads(estimates_file.read_text())
            mean_ns: float = data["mean"]["point_estimate"]
            rel_path = str(estimates_file.relative_to(criterion_dir))
            results.append((rel_path, mean_ns))
        except (KeyError, json.JSONDecodeError, OSError):
            continue
    return results


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--startup-max-ms",
        type=float,
        required=True,
        metavar="MS",
        help="Maximum allowed mean startup time in milliseconds",
    )
    parser.add_argument(
        "--latency-max-p99-us",
        type=float,
        required=True,
        metavar="US",
        help="Maximum allowed p99 per-key latency in microseconds",
    )
    parser.add_argument(
        "--criterion-dir",
        default="target/criterion",
        metavar="DIR",
        help="Path to criterion output directory (default: target/criterion)",
    )
    args = parser.parse_args()

    startup_max_ns = args.startup_max_ms * 1_000_000.0
    latency_max_ns = args.latency_max_p99_us * 1_000.0

    criterion_dir = Path(args.criterion_dir)
    if not criterion_dir.exists():
        print(
            f"WARNING: criterion directory {str(criterion_dir)!r} not found; "
            "no benchmarks to check.",
            file=sys.stderr,
        )
        return 0

    estimates = find_estimates(criterion_dir)
    if not estimates:
        print(
            f"WARNING: no estimates.json files found under {str(criterion_dir)!r}.",
            file=sys.stderr,
        )
        return 0

    violations: list[str] = []
    checked = 0

    for path, mean_ns in estimates:
        path_lower = path.lower()
        if "startup" in path_lower:
            checked += 1
            if mean_ns > startup_max_ns:
                mean_ms = mean_ns / 1_000_000.0
                violations.append(
                    f"  {path}: mean {mean_ms:.3f} ms "
                    f"> {args.startup_max_ms} ms startup budget"
                )
        elif "p99" in path_lower:
            checked += 1
            if mean_ns > latency_max_ns:
                mean_us = mean_ns / 1_000.0
                violations.append(
                    f"  {path}: mean {mean_us:.1f} µs "
                    f"> {args.latency_max_p99_us} µs p99 latency budget"
                )

    if violations:
        print(
            f"PERFORMANCE BUDGET EXCEEDED: {len(violations)} benchmark(s) over limit:",
            file=sys.stderr,
        )
        for msg in violations:
            print(msg, file=sys.stderr)
        return 1

    print(
        f"Performance budget check passed: "
        f"{checked} benchmark(s) checked, all within limits."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
