# Credit Card Fraud (ULB) — attribution (test-only)

- **Source**: Kaggle, *Credit Card Fraud Detection* (Université Libre de
  Bruxelles / Worldline), https://www.kaggle.com/datasets/mlg-ulb/creditcardfraud
  Also distributed via OpenML (dataset 1597) and used in ADBench as `fraud`.
- **Content**: 284,807 transactions, 492 fraud (0.17%), features V1-V28
  (PCA components), Time, Amount, Class.
- **Status**: **test-only**. Distribution is through Kaggle account
  download; the data is not bundled in this repository. Expectations
  and attribution are recorded in the registry; `prepare.py` fetches
  via the Kaggle API when credentials are available.
