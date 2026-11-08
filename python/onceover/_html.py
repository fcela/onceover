"""Self-contained HTML rendering of a profile report.

The HTML is a rendering of the same report dict the JSON contract
(`docs/report-schema.md`) defines: every claim, count and statistic
comes from `ProfileReport.data`, and nothing is recomputed from the
source table. Charts are Altair / Vega-Lite with the Vega bundle
inlined once in the page head, so the file opens offline and makes
no external requests.

Rendering needs the optional `html` extra (Altair and
vl-convert-python, which supplies the bundle):

    pip install 'onceover[html]'
"""

import html as _html
import json as _json

_PAGE_CSS = """
body { font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
       margin: 0; color: #1a1a1a; background: #fff; }
main { max-width: 1200px; margin: 0 auto; padding: 24px 16px 80px; }
h1 { font-size: 1.5rem; margin-bottom: 0.2rem; }
h2 { font-size: 1.15rem; margin-top: 2.2rem; border-bottom: 1px solid #ddd;
     padding-bottom: 0.3rem; }
h3 { font-size: 1rem; margin-bottom: 0.3rem; }
.headline { font-size: 1.05rem; }
.meta, .stats, footer { color: #555; font-size: 0.9rem; }
table { border-collapse: collapse; width: 100%; margin: 0.6rem 0 1rem; }
th, td { text-align: left; vertical-align: top; padding: 4px 8px;
         border-bottom: 1px solid #eee; font-size: 0.92rem; }
th { border-bottom: 1px solid #bbb; }
td.num, th.num { text-align: right; font-variant-numeric: tabular-nums; }
.columns-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(360px, 1fr));
                gap: 0 24px; }
.column-card { break-inside: avoid; }
.dtype { color: #555; font-weight: normal; font-size: 0.85rem; }
.role { background: #eef2f7; border-radius: 3px; padding: 0 5px;
        font-size: 0.8rem; }
.chart { margin: 0.3rem 0 0.6rem; }
footer { margin-top: 3rem; border-top: 1px solid #ddd; padding-top: 0.6rem; }
@media print { .column-card { break-inside: avoid; } main { max-width: none; } }
"""


def _import_render_stack():
    try:
        import altair as alt
        import vl_convert as vlc
    except ImportError as exc:
        raise ImportError(
            "HTML reports need the optional html extra: "
            "pip install 'onceover[html]' (altair and vl-convert-python)"
        ) from exc
    return alt, vlc


def _esc(text) -> str:
    return _html.escape(str(text), quote=True)


def _num(value) -> str:
    if value is None:
        return "—"
    if isinstance(value, float):
        return f"{value:,.4g}"
    if isinstance(value, int):
        return f"{value:,}"
    return str(value)


class _Charts:
    """Collects Altair charts and emits one embed block per chart.

    The Vega bundle is inlined once by `render`; each chart adds only
    its spec and a scoped embed call, so a report with many charts
    carries the ~0.9 MB bundle a single time.
    """

    def __init__(self, alt):
        self._alt = alt
        self._count = 0

    def embed(self, chart) -> str:
        self._count += 1
        div_id = f"vis-{self._count}"
        spec = _json.dumps(chart.to_dict()).replace("</", "<\\/")
        return (
            f'<div class="chart vega-visualization" id="{div_id}"></div>\n'
            '<script type="text/javascript">\n'
            f"  (function () {{ const spec = {spec}; "
            f"vegaEmbed('#{div_id}', spec, {{\"mode\": \"vega-lite\"}})"
            ".catch(console.error); })();\n"
            "</script>"
        )


def _category_chart(alt, findings):
    import polars as pl

    counts = {}
    for finding in findings:
        category = finding.get("category") or "—"
        counts[category] = counts.get(category, 0) + 1
    rows = [
        {"category": category, "findings": n}
        for category, n in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))
    ]
    chart = (
        alt.Chart(pl.DataFrame(rows))
        .mark_bar(color="#4c78a8")
        .encode(
            x=alt.X("findings:Q", title="findings"),
            y=alt.Y("category:N", title=None, sort="-x"),
            tooltip=["category:N", "findings:Q"],
        )
        .properties(width=380, height=max(60, 24 * len(rows) + 20),
                    title="Selected findings by category")
    )
    return chart


def _top_values_chart(alt, card):
    import polars as pl

    rows = [
        {"value": str(entry.get("value")), "count": entry.get("count")}
        for entry in card.get("top_values") or []
        if isinstance(entry.get("count"), (int, float))
    ]
    if not rows:
        return None
    chart = (
        alt.Chart(pl.DataFrame(rows))
        .mark_bar(color="#4c78a8")
        .encode(
            x=alt.X("count:Q", title="count"),
            y=alt.Y("value:N", title=None, sort="-x"),
            tooltip=["value:N", "count:Q"],
        )
        .properties(width=330, height=max(50, 24 * len(rows) + 16),
                    title="Most frequent values")
    )
    return chart


def _numeric_strip_chart(alt, card):
    import polars as pl

    stats = card.get("numeric_stats") or {}
    keys = ("min", "q25", "q50", "q75", "max")
    if any(not isinstance(stats.get(k), (int, float)) for k in keys):
        return None
    if stats["min"] == stats["max"]:
        return None
    row = {k: stats[k] for k in keys}
    if isinstance(stats.get("mean"), (int, float)):
        row["mean"] = stats["mean"]
    base = alt.Chart(pl.DataFrame([row]))
    whisker = base.mark_rule(color="#666").encode(x="min:Q", x2="max:Q")
    box = base.mark_bar(height=12, color="#4c78a8").encode(
        x="q25:Q", x2="q75:Q",
        tooltip=["min:Q", "q25:Q", "q50:Q", "q75:Q", "max:Q"],
    )
    median = base.mark_tick(color="#111", thickness=2, size=16).encode(x="q50:Q")
    layers = whisker + box + median
    if "mean" in row:
        layers = layers + base.mark_point(
            shape="diamond", size=70, color="#e45756", filled=True
        ).encode(x="mean:Q", tooltip=["mean:Q"])
    return layers.properties(width=330, height=34,
                             title="Range, quartiles and mean")


def _findings_table(findings, grouped):
    parts = []
    if findings:
        rows = []
        for finding in findings:
            subject = finding.get("subject") or {}
            columns = ", ".join(subject.get("columns") or [])
            score = finding.get("interestingness")
            rows.append(
                "<tr>"
                f"<td>{_esc(finding.get('claim'))}</td>"
                f"<td>{_esc(finding.get('category'))}</td>"
                f"<td>{_esc(finding.get('type'))}</td>"
                f"<td>{_esc(columns)}</td>"
                f'<td class="num">{score:.2f}</td>'
                "</tr>"
                if isinstance(score, (int, float))
                else "<tr>"
                f"<td>{_esc(finding.get('claim'))}</td>"
                f"<td>{_esc(finding.get('category'))}</td>"
                f"<td>{_esc(finding.get('type'))}</td>"
                f"<td>{_esc(columns)}</td>"
                '<td class="num">—</td>'
                "</tr>"
            )
        parts.append(
            "<table><thead><tr><th>Finding</th><th>Category</th>"
            "<th>Type</th><th>Columns</th>"
            '<th class="num">Interestingness</th></tr></thead>'
            f"<tbody>{''.join(rows)}</tbody></table>"
        )
    if grouped:
        rows = []
        for finding in grouped:
            rows.append(
                "<tr>"
                f"<td>{_esc(finding.get('claim'))}</td>"
                f"<td>{_esc(finding.get('category'))}</td>"
                f"<td>{_esc(finding.get('type'))}</td>"
                f"<td>{_esc(finding.get('grouped_under'))}</td>"
                "</tr>"
            )
        parts.append(
            "<h3>Grouped statements</h3>"
            "<p class=\"stats\">Statements carried under a verified "
            "relationship family's lead finding.</p>"
            "<table><thead><tr><th>Finding</th><th>Category</th>"
            "<th>Type</th><th>Grouped under</th></tr></thead>"
            f"<tbody>{''.join(rows)}</tbody></table>"
        )
    if not parts:
        parts.append("<p>No findings were selected for this report.</p>")
    return "".join(parts)


def _column_card(alt, charts, name, card):
    parts = [f'<article class="column-card"><h3>{_esc(name)}']
    dtype = card.get("dtype")
    if dtype:
        parts.append(f' <span class="dtype">{_esc(dtype)}</span>')
    role = card.get("semantic_role") or card.get("role")
    if role:
        parts.append(f' <span class="role">{_esc(role)}</span>')
    parts.append("</h3>")
    n = card.get("n")
    null_count = card.get("null_count")
    null_fraction = card.get("null_fraction")
    stats = [f"distinct {_num(card.get('distinct'))}"]
    if isinstance(null_count, (int, float)) and isinstance(n, (int, float)):
        stats.insert(0, f"nulls {_num(null_count)} of {_num(n)}")
    elif isinstance(null_fraction, (int, float)):
        stats.insert(0, f"nulls {100.0 * null_fraction:.1f}%")
    parts.append(f'<p class="stats">{" · ".join(stats)}</p>')
    numeric = card.get("numeric_stats") or {}
    if numeric:
        line = (
            f"min {_num(numeric.get('min'))} · q25 {_num(numeric.get('q25'))}"
            f" · median {_num(numeric.get('q50'))} · q75 {_num(numeric.get('q75'))}"
            f" · max {_num(numeric.get('max'))} · mean {_num(numeric.get('mean'))}"
            f" · std {_num(numeric.get('std'))}"
        )
        parts.append(f'<p class="stats">{line}</p>')
    chart = None
    if numeric:
        chart = _numeric_strip_chart(alt, card)
    if chart is None:
        chart = _top_values_chart(alt, card)
    if chart is not None:
        parts.append(charts.embed(chart))
    parts.append("</article>")
    return "".join(parts)


def _coverage_table(coverage):
    if not coverage:
        return "<p>No coverage was recorded.</p>"
    rows = []
    for name in sorted(coverage):
        entry = coverage[name] or {}
        detail = entry.get("detail") or ""
        rows.append(
            "<tr>"
            f"<td>{_esc(name)}</td>"
            f"<td>{_esc(entry.get('status'))}</td>"
            f"<td>{_esc(detail)}</td>"
            "</tr>"
        )
    return (
        "<table><thead><tr><th>Pass</th><th>Status</th><th>Detail</th>"
        f"</tr></thead><tbody>{''.join(rows)}</tbody></table>"
    )


def render(data, *, version):
    """Render one report dict as a self-contained HTML document.

    `data` is `ProfileReport.data` — the versioned report object.
    The rendering reads only that dict, so the HTML and the JSON of
    the same report always agree.
    """
    alt, vlc = _import_render_stack()
    # Chart data comes from the report itself, already materialized;
    # the row cap Altair applies to raw frames does not apply here.
    alt.data_transformers.disable_max_rows()
    charts = _Charts(alt)

    overview = data.get("overview") or {}
    findings = data.get("findings") or []
    grouped = data.get("grouped_findings") or []
    columns = data.get("columns") or {}
    coverage = data.get("coverage") or {}
    source = data.get("source")
    schema_version = data.get("schema_version") or "—"

    parts = [
        "<!DOCTYPE html>",
        '<html lang="en">',
        "<head>",
        '<meta charset="utf-8">',
        "<title>onceover report</title>",
        f"<style>{_PAGE_CSS}</style>",
        '<script type="text/javascript">',
        vlc.javascript_bundle(),
        "</script>",
        "</head>",
        "<body>",
        "<main>",
        "<header>",
        "<h1>onceover report</h1>",
        f'<p class="headline">{_esc(overview.get("headline"))}</p>',
        '<p class="meta">'
        f"{_num(overview.get('n_rows'))} rows × "
        f"{_num(overview.get('n_columns'))} columns · "
        f"report schema {_esc(schema_version)} · onceover {_esc(version)}"
        "</p>",
    ]
    if source:
        sample = source.get("sample") or {}
        basis = source.get("basis") or "full"
        extra = ""
        if basis == "sampled" and sample:
            extra = (
                f" (sample: {sample.get('method')}, seed "
                f"{sample.get('seed')}, cap {sample.get('cap')})"
            )
        parts.append(
            '<p class="meta">'
            f"Source: {_esc(source.get('kind'))}, "
            f"{_num(source.get('rows'))} rows, "
            f"{_num(source.get('rows_profiled'))} profiled, "
            f"basis {_esc(basis)}{_esc(extra)}"
            "</p>"
        )
    parts.append("</header>")

    parts.append('<section id="findings"><h2>Findings</h2>')
    if findings:
        parts.append(charts.embed(_category_chart(alt, findings)))
    parts.append(_findings_table(findings, grouped))
    parts.append("</section>")

    parts.append('<section id="columns"><h2>Columns</h2>')
    parts.append('<div class="columns-grid">')
    for name, card in columns.items():
        parts.append(_column_card(alt, charts, name, card or {}))
    parts.append("</div></section>")

    parts.append('<section id="coverage"><h2>Coverage</h2>')
    parts.append(
        "<p class=\"stats\">What each pass did. An empty findings list "
        "means the passes ran and found nothing only when coverage "
        "says they ran.</p>"
    )
    parts.append(_coverage_table(coverage))
    parts.append("</section>")

    parts.append(
        "<footer>"
        f"Generated by onceover {_esc(version)} · report schema "
        f"{_esc(schema_version)} · Self-contained: the charts render "
        "offline and this file makes no external requests."
        "</footer>"
    )
    parts.append("</main></body></html>")
    return "\n".join(parts)
