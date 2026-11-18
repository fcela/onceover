# Residual shift versus a linear date trend

Issue #378. A dated null with no break was being stated as a residual level shift.

## Why

The residual-shift scan fits a global linear model on the non-date predictors
and scans its residuals along the date axis. A pure trend in the target is not
a term in that model, so the residuals trend; a one-mean-break scan then reads
the ramp's midpoint as a step. The statement was true of the residuals but
misleading as a change in the target.

Before the fix, the deterministic trend-only probe in this record produced one
`changes/residual_shift` finding at the shipped bars (train gain 0.15, holdout
gain 0.15). That is the false statement in #378.

## What changed

The one-break scan still runs on the original residuals, so a real step keeps
its existing evidence. A global linear trend of those residuals against the
date axis is now fitted as a rival explanation on the training rows. The break
is kept only when it beats the trend by at least `TREND_MARGIN = 0.02` on both
training gain and holdout error reduction. The reported breakpoint, offsets and
error reduction remain those of the break model; `trend_train_error_reduction`,
`trend_holdout_error_reduction` and `trend_margin` are added to the evidence so
the comparison can be inspected.

No acceptance bar changed: a break still needs train gain 0.15 and holdout
error reduction 0.15.

## Measurement

Deterministic Rust probes, seed 1234, n = 2000, date axis `20000 + i`, target
`y = 3x + 4(i/n) + noise` with `x` independent and noise uniform on ±0.02.
The step probe adds 5.0 from the midpoint onward; the seasonal probe adds
`0.5 sin(2πi/50)`. The profile uses the standard regimes config (5000-row cap)
and the existing shortlisted predictors.

| shape | train break | train trend | holdout break | holdout trend | result |
|---|---:|---:|---:|---:|---|
| pure trend | 0.7502 | 0.9999 | not evaluated | — | no shift stated |
| trend + seasonality | — | — | — | — | no shift stated |
| planted step on trend | 0.9734 | 0.8762 | 0.9739 | 0.8740 | shift stated, holdout gain 97.4%, breakpoint within 10 days of the planted midpoint |

The smallest separation on a table that must keep its shift is 0.0972 train
and 0.0998 holdout. The 0.02 margin sits well below both, while the pure-trend
null is separated in the other direction by 0.2496 train gain. The margin is
therefore a tie-breaker, not a new effect-size bar.

## Reproduce

```sh
cargo test -p onceover-mines regimes::tests -- --nocapture
```

The three probes are `pure_date_trend_does_not_become_residual_shift`,
`trend_plus_seasonality_does_not_become_residual_shift` and
`planted_step_on_a_trending_series_still_reports_shift`. The first failed with
one false shift before the fix; all three pass with it.

## Limits

This is a linear date-trend rival, not a general trend/seasonality model. A
nonlinear smooth drift that a line cannot absorb is still outside the guard;
it may be read as a step. The seasonal null is silent here because no one-break
model clears the existing gain bars, not because seasonality is modelled in the
comparison. A planted step with an effect small enough to fall inside the 0.02
margin would be withheld; the existing 0.15 gain bars already withhold much
weaker steps.
