# Splice-junction gene sequences — attribution

- **Source**: UCI Machine Learning Repository, *Molecular Biology
  (Splice-junction Gene Sequences)* (dataset 69),
  https://archive.ics.uci.edu/static/public/69/data.csv
- **Creators**: assembled by G. Towell, M. Noordewier, and J. Shavlik
  (per RuleQuest's sample page and the UCI dataset documentation).
- **License**: CC BY 4.0 (UCI Machine Learning Repository terms).
- **Citation requested by the source**: DOI 10.24432/C5M888.
  Dataset page: https://archive.ics.uci.edu/dataset/69/molecular+biology+%28splice-junction+gene+sequences%29
- **Retrieved**: 2026-10-28.
- **GritBot reference**: two flagged cases (550, 839) breaking near-universal
  residue patterns for their junction class.

## Preprocessing log (decode/rename only)

1. Class codes decoded: EI -> exon_intron, IE -> intron_exon, N -> neither
   (UCI documentation).
2. `instancename` -> `instance_name`; `Base1..Base60` -> `base_1..base_60`.
