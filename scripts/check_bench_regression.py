#!/usr/bin/env python3
"""Compare two bencher-format benchmark files and fail if any measurement
regressed beyond a configurable threshold.

Bencher output format (one line per measurement):
  test <name> ... bench:   <value> ns/iter (+/- <variance>)

Usage:
  check_bench_regression.py baseline.txt current.txt [--max-regression-pct N]
"""

import argparse
import re
import sys


BENCH_RE = re.compile(
    r"^test\s+(\S+)\s+\.\.\.\s+bench:\s+([\d,]+)\s+ns/iter"
)


def parse_file(path: str) -> dict[str, float]:
    """Return {benchmark_name: ns_per_iter} from a bencher-format file."""
    results: dict[str, float] = {}
    with open(path) as fh:
        for line in fh:
            m = BENCH_RE.match(line.strip())
            if m:
                name = m.group(1)
                value = float(m.group(2).replace(",", ""))
                results[name] = value
    return results


def check_regression(
    baseline: dict[str, float],
    current: dict[str, float],
    max_pct: float,
) -> list[str]:
    """Return a list of human-readable regression messages (empty = all OK)."""
    regressions: list[str] = []
    for name, base_val in baseline.items():
        if name not in current:
            # Benchmark removed — not treated as a regression.
            continue
        cur_val = current[name]
        if base_val == 0:
            continue
        pct_change = (cur_val - base_val) / base_val * 100.0
        if pct_change > max_pct:
            regressions.append(
                f"  {name}: {base_val:.1f} → {cur_val:.1f} ns/iter "
                f"(+{pct_change:.1f}% > {max_pct}% limit)"
            )
    return regressions


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", help="Baseline bencher output file")
    parser.add_argument("current", help="Current bencher output file")
    parser.add_argument(
        "--max-regression-pct",
        type=float,
        default=10.0,
        metavar="N",
        help="Maximum allowed regression in percent (default: 10)",
    )
    args = parser.parse_args()

    baseline = parse_file(args.baseline)
    current = parse_file(args.current)

    if not baseline:
        print(f"WARNING: no benchmarks parsed from {args.baseline!r}", file=sys.stderr)
        return 0
    if not current:
        print(f"WARNING: no benchmarks parsed from {args.current!r}", file=sys.stderr)
        return 0

    regressions = check_regression(baseline, current, args.max_regression_pct)
    if regressions:
        print(
            f"BENCHMARK REGRESSION: {len(regressions)} benchmark(s) exceeded "
            f"the {args.max_regression_pct}% threshold:",
            file=sys.stderr,
        )
        for msg in regressions:
            print(msg, file=sys.stderr)
        return 1

    print(
        f"Benchmark check passed: {len(current)} benchmark(s) within "
        f"{args.max_regression_pct}% of baseline."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
