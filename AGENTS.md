# Project preferences

- Use Altair for charts and notebook visualizations. Never use Matplotlib.
- Use Great Tables for notebook tables. Wrap long text and preserve complete strings.
- Keep the example notebook runnable against this repository's installed package;
  do not add imports or runtime paths into the former `data-profilers` repository.
- Before publishing, validate large-table behavior using the reproducible scale
  benchmark, including planted relationships in columns beyond position 64.
