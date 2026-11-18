# Shared model basis

Issue #446, step 1.

## Change

`onceover_mines::ModelBasis` carries what the predictability screen already uses into every model-based pass:
- the inputs no model may use: scaffolding, redundant recodings and missingness indicators, but not the time axis or the columns it determines;
- the exact identity groups of #427 (`ModelBasis::excluded_for(target)`).

Calendar features of the axis pass every refit, as they pass the screen.

- **Interactions** (`hstat.rs`) refit the target with those exclusions, the designated target first, then by holdout score.
- **Model failures** exclude the target's identity partners from model inputs and slice variables.
- **Target shapes** do not compare the target with excluded columns or its identity partners.
- **Drift** refits with the same exclusions.

## Effect on bike sharing

Before this change, the interactions pass reported "3 fitted, 0 clearing the additive-refit gate". cnt was refit on registered and casual, its own sum's parts, so the surface was additive. With the identity partners excluded:

- **Designated target cnt:** "cnt depends on hr and workingday jointly: 8.3% of the pair's joint partial-dependence variance is interaction …, from a surrogate with holdout R² 0.85 against 0.68 for an additive refit".
- **Default run:** "registered depends on hr and workingday jointly … holdout R² 0.86 against 0.67 for an additive refit".
- **Model failures (target cnt)**, before: "within casual > 92 … errs more" and similar, restatements of the identity. After: the commute structure, for example "within workingday = '0' and hr = '8' (231 rows), the model of 'cnt' is one-sided: its mean out-of-fold residual there is −111.7".
- **Target shapes** no longer state "casual relates to cnt as a monotone increasing curve". The label on hr ("monotone increasing") is step 2.

## Validation

`cargo test --workspace` and `python -m pytest tests/ -q` (848 passed). Gate: see the pull request.
