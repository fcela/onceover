"""Regenerate data.csv for the GSE15258 gene-expression benchmark table.

Source: NCBI GEO series GSE15258 (rheumatoid arthritis whole-blood
expression, anti-TNF therapy study), series matrix from
https://ftp.ncbi.nlm.nih.gov/geo/series/GSE15nnn/GSE15258/matrix/GSE15258_series_matrix.txt.gz
Public data (NCBI GEO). The CrossCat paper's gene-expression evaluation
used this series (87 patients, ~55k probes) at 1,000- and 5,000-column
subsets.

This bundle: one row per array (sample), sample annotations first
(sample_id, geo_accession, disease_state, response_to_therapy,
das28_v1, das28_v3 -- parsed from the series-matrix !Sample_
characteristics), then the FIRST 5,000 probes in file order. The
subset rule is deterministic and stated here rather than sampled:
the paper's subsets were random draws; a benchmark needs one fixed
table. No values altered.
"""
import gzip
import io
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = ("https://ftp.ncbi.nlm.nih.gov/geo/series/GSE15nnn/GSE15258/"
       "matrix/GSE15258_series_matrix.txt.gz")
N_PROBES = 5000
CHAR_MAP = {
    "disease state": "disease_state",
    "response to anti-tnf therapy": "response_to_therapy",
    "disease activity (das28_v1)": "das28_v1",
    "disease activity (das28_v3)": "das28_v3",
}


def unq(s: str) -> str:
    return s.strip().strip('"')


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    raw = CACHE / "GSE15258_series_matrix.txt.gz"
    if not raw.exists():
        urllib.request.urlretrieve(URL, raw)
    text = gzip.open(raw, "rt").read().splitlines()
    titles, accessions, chars = [], [], {}
    probes, in_table = [], False
    sample_count = None
    for line in text:
        if line.startswith("!Sample_title"):
            titles = [unq(x) for x in line.split("\t")[1:]]
        elif line.startswith("!Sample_geo_accession"):
            accessions = [unq(x) for x in line.split("\t")[1:]]
        elif line.startswith("!Sample_characteristics_ch1"):
            vals = [unq(x) for x in line.split("\t")[1:]]
            key = vals[0].split(":")[0].strip().lower()
            if key in CHAR_MAP:
                chars[CHAR_MAP[key]] = [v.split(":", 1)[1].strip() if ":" in v else v
                                        for v in vals]
        elif line.startswith("!series_matrix_table_begin"):
            in_table = True
        elif line.startswith("!series_matrix_table_end"):
            break
        elif in_table:
            parts = line.split("\t")
            if parts[0] == '"ID_REF"':
                sample_count = len(parts) - 1
                continue
            if len(probes) < N_PROBES:
                probes.append((unq(parts[0]), [unq(x) for x in parts[1:]]))
    n = sample_count or len(titles)
    assert len(titles) == n and len(accessions) == n, (len(titles), len(accessions), n)
    meta_cols = ["disease_state", "response_to_therapy", "das28_v1", "das28_v3"]
    out = io.StringIO()
    out.write(",".join(["sample_id", "geo_accession"] + meta_cols
                       + [p for p, _ in probes]) + "\n")
    for j in range(n):
        row = [titles[j], accessions[j]]
        row += [chars.get(c, [""] * n)[j] for c in meta_cols]
        row += [vals[j] for _, vals in probes]
        out.write(",".join(row) + "\n")
    (HERE / "data.csv").write_text(out.getvalue())
    print(f"wrote {n} samples x {len(probes)} probes")


if __name__ == "__main__":
    main()
