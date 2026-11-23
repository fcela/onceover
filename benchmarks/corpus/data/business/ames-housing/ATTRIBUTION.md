# Ames Housing — attribution

- **Source**: Dean De Cock (2011), *Ames, Iowa: Alternative to the Boston
  Housing Data as an End of Semester Regression Project*, Journal of
  Statistics Education, 19(3),
  https://jse.amstat.org/v19n3/decock.pdf
- **Published file**: `AmesHousing.txt` (tab-separated), archived via
  Journal of Statistics Education / OpenML / standard educational mirrors.
- **Original data**: Ames Assessor's Office public residential property
  sales records (2006–2010), compiled by Dean De Cock for statistical
  education.
- **sha256 (`data.tsv`)**: `6cfe6cb525ba437de428653a1040e2aed7d696640bf75203786a6d7a0e67cfcc`

## Content

2,930 residential property sales in Ames, Iowa from January 2006 through
July 2010 across 82 columns: identifiers (`Order`, `PID`), lot and
neighborhood descriptors, architectural and quality ratings, basement and
garage dimensions, porch/deck areas, sale metadata (`Mo Sold`, `Yr Sold`,
`Sale Type`, `Sale Condition`), and `SalePrice` (USD).

## Preprocessing log

None: `data.tsv` stores the published tab-separated file unchanged.
Literal `"NA"` and `""` entries are kept as published; callers loading
with Polars pass `separator="\t", null_values=["NA", ""]`. The quirks
documented in the case study (`Garage Yr Blt = 2207`, `Year Remod/Add`
left-censoring at `1950`, partial-sale mega-houses in `Edwards`, structural
vs. MAR missingness) are the dataset's published character and are kept
untouched.
