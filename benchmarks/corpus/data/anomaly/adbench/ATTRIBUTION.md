# ADBench — status (test-only, not wired)

- **Suite**: ADBench (Han et al., NeurIPS 2022), 57 tabular datasets
  with binary anomaly labels, https://github.com/Minqi824/ADBench
  (repository code license BSD-2-Clause).
- **Status**: not wired. Two reasons, both recorded rather than
  smoothed over: (1) the datasets are not in the repository tree —
  distribution is via external download, not yet fetched and audited;
  (2) the license gate applies per dataset: ADBench aggregates
  datasets from many original sources (UCI, Kaggle, and others) under
  different terms, so members are bundled or test-only one by one,
  never as a suite wholesale. The scoring mapping in the harness
  (anomaly_coverage) is suite-agnostic and applies to ADBench
  members as they are admitted.
