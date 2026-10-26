"""Render the executed example notebook with readable tables and offline Altair charts."""

import argparse
from pathlib import Path

import altair as alt
import nbformat
from nbconvert import HTMLExporter
from traitlets.config import Config


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    root = Path(__file__).resolve().parents[1]
    parser.add_argument("--notebook", type=Path, default=root / "notebooks/onceover_analysis.ipynb")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    notebook = nbformat.read(args.notebook, as_version=4)
    charts = 0
    for cell in notebook.cells:
        for output in cell.get("outputs", []):
            data = output.get("data", {})
            spec = next((value for key, value in data.items()
                         if key.startswith("application/vnd.vegalite.v")), None)
            if spec:
                output["data"] = {"text/html": alt.Chart.from_dict(spec).to_html(fullhtml=False, inline=True)}
                charts += 1
    config = Config()
    config.TagRemovePreprocessor.enabled = True
    config.TagRemovePreprocessor.remove_input_tags = {"hide-input"}
    exporter = HTMLExporter(template_name="lab", config=config)
    exporter.exclude_input_prompt = True
    exporter.exclude_output_prompt = True
    exporter.embed_images = True
    exporter.mathjax_url = ""
    exporter.require_js_url = ""
    html, _ = exporter.from_notebook_node(notebook, resources={"metadata": {"name": notebook.metadata.get("title", args.notebook.stem)}})
    shell_css = """<style>
    body {background:#ffffff;}
    .jp-Notebook {max-width:1200px;margin:0 auto;padding:24px 16px 80px;}
    .jp-RenderedHTMLCommon {line-height:1.65;}
    .jp-RenderedHTMLCommon p {max-width:100%;}
    .jp-RenderedHTMLCommon table {white-space:normal;}
    .gt_table td {white-space:normal!important;overflow-wrap:anywhere;}
    .jp-OutputArea-output {overflow-x:auto;}
    @media print {details {break-inside:avoid;} .jp-Notebook {max-width:none;padding:0;}}
    </style>"""
    html = html.replace("</head>", shell_css + "</head>")
    destination = args.output or args.notebook.with_suffix(".html")
    destination.write_text(html)
    print(f"Rendered {destination.resolve()} ({destination.stat().st_size:,} bytes; {charts} interactive charts)")


if __name__ == "__main__":
    main()
