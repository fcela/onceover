"""Command-line entry point: `onceover FILE [FILE ...]`.

One file is profiled with `onceover.profile` (Parquet, CSV or IPC,
read at the source with the same sampling rules as the Python API)
and printed as a digest by default. Several files are read as a set
of related tables and profiled with `onceover.profile_many`;
renderings for a set are the digest and the JSON.
"""

import argparse
import sys
from pathlib import Path

import onceover

_FORMATS = ("digest", "json", "markdown", "html")


def _build_parser():
    parser = argparse.ArgumentParser(
        prog="onceover",
        description=(
            "Profile a data file (Parquet, CSV, IPC) and print a "
            "rendering of the report. With several files, profile "
            "them together as related tables and report candidate "
            "foreign keys between them."
        ),
    )
    parser.add_argument(
        "files",
        nargs="+",
        metavar="FILE",
        help="data file(s) to profile: .parquet, .csv or .ipc",
    )
    parser.add_argument(
        "--format",
        choices=_FORMATS,
        default=None,
        help="rendering to print (default: digest)",
    )
    parser.add_argument(
        "--digest",
        type=int,
        metavar="CHARS",
        default=None,
        help="print the digest budgeted to CHARS characters",
    )
    parser.add_argument(
        "--output",
        "-o",
        metavar="PATH",
        default=None,
        help="write the rendering to PATH instead of stdout",
    )
    parser.add_argument(
        "--budget",
        choices=("quick", "standard", "deep"),
        default="standard",
        help="profiling budget (default: standard)",
    )
    parser.add_argument(
        "--deadline-seconds",
        type=float,
        default=None,
        help="soft wall-clock budget; skipped work is reported in coverage",
    )
    parser.add_argument(
        "--target",
        metavar="COLUMN",
        default=None,
        help="designate a response column (single file only)",
    )
    parser.add_argument(
        "--source-row-cap",
        type=int,
        default=None,
        help="most rows collected from the file (default: 200000)",
    )
    parser.add_argument(
        "--exact",
        action="store_true",
        help="collect the whole file regardless of the source row cap",
    )
    parser.add_argument(
        "--sample-seed",
        type=int,
        default=0,
        help="seed for the source sample (default: 0)",
    )
    parser.add_argument(
        "--version",
        action="version",
        version=f"onceover {onceover.__version__}",
    )
    return parser


def _read_table(path):
    import polars as pl

    suffix = path.suffix.lower()
    if suffix == ".parquet":
        return pl.read_parquet(path)
    if suffix == ".csv":
        return pl.read_csv(path)
    if suffix in (".ipc", ".feather", ".arrow"):
        return pl.read_ipc(path)
    raise ValueError(
        f"unsupported file type {suffix or '(none)'} for {path}; "
        "use .parquet, .csv or .ipc"
    )


def _render_single(path, args, fmt):
    report = onceover.profile(
        str(path),
        budget=args.budget,
        deadline_seconds=args.deadline_seconds,
        target=args.target,
        source_row_cap=args.source_row_cap,
        exact=args.exact,
        sample_seed=args.sample_seed,
    )
    if fmt == "digest":
        return report.to_digest(args.digest if args.digest is not None else 4000)
    if fmt == "json":
        return report.to_json()
    if fmt == "markdown":
        return report.to_markdown()
    if fmt == "html":
        return report.to_html()
    raise AssertionError(fmt)


def _render_many(paths, args, fmt):
    if fmt not in ("digest", "json"):
        raise ValueError(
            f"format {fmt!r} is not available for multiple files; "
            "use digest or json"
        )
    if args.target is not None:
        raise ValueError("--target applies to a single file only")
    tables = {}
    for path in paths:
        name = path.stem
        if name in tables:
            raise ValueError(
                f"two files give the same table name {name!r}; "
                "rename one of them"
            )
        tables[name] = _read_table(path)
    report = onceover.profile_many(tables)
    if fmt == "digest":
        return report.to_digest(args.digest if args.digest is not None else 4000)
    return report.to_json()


def main(argv=None) -> int:
    """Run the `onceover` console script. Returns the exit code."""
    parser = _build_parser()
    args = parser.parse_args(argv)
    if args.digest is not None and args.format not in (None, "digest"):
        parser.error("--digest applies to the digest format only")
    fmt = args.format or "digest"
    paths = [Path(f) for f in args.files]
    for path in paths:
        if not path.is_file():
            print(f"onceover: no such file: {path}", file=sys.stderr)
            return 2
    try:
        if len(paths) == 1:
            text = _render_single(paths[0], args, fmt)
        else:
            text = _render_many(paths, args, fmt)
    except (ValueError, ImportError) as exc:
        print(f"onceover: {exc}", file=sys.stderr)
        return 1
    if args.output:
        Path(args.output).write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text if text.endswith("\n") else text + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
