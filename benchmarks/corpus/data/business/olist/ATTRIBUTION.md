# Olist Brazilian E-Commerce — attribution (test-only)

- **Source**: Kaggle, *Brazilian E-Commerce Public Dataset by Olist*,
  https://www.kaggle.com/datasets/olistbr/brazilian-ecommerce
- **License**: CC BY-NC-SA 4.0 (per the Kaggle dataset page).
- **Status**: **test-only** under the benchmark license gate. The
  non-commercial and share-alike terms conflict with this package's MIT
  license, so the data is never bundled. `prepare.py` fetches it with
  the Kaggle API when credentials are available; without them it stops
  with instructions. Attribution and expectations live in the registry.
- 9 files: orders, order items, payments, reviews, customers, sellers,
  products, geolocation, category-name translation.
