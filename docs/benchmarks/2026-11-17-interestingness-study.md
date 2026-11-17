# Interestingness study: which statements reach the reader first, and what each statement lets the reader do

This document is the study behind issues #347 and #349. Issue #347 asks for a principled rule for ranking a profile's statements when only some of them fit in the briefing a reader actually sees. Issue #349 asks for a closed vocabulary of the actions a statement can license, so the rule can be stated in terms of what the reader can do.

The study derives the action taxonomy from the decision each action settles, tests it against all 64 finding types the library can state, and then tests two candidate ranking principles against the digest gate's labeled statements: compression gain per character, and action class. Both candidates fail as the whole rule, in different and measurable ways. The document states the rule that survives both tests, applies it by hand to one gate table's current briefing, and gives the protocol by which a later implementation is to be judged.

It decides the design questions in #347 and #349. It changes no library code. All measurements were run on main at `610812ef` with the gate's own matching code; the scripts are named in section 10 and every number they produced is reproduced in the appendices, so the claims can be checked from this document alone.

## 1. The problem

A profile computes every finding it can (43 registered detectors at the base commit). The briefing — the digest an agent or a person reads in one pass — holds a few thousand characters of them. Selection is therefore the product: a true statement left out of the briefing might as well not have been computed, and a statement that takes briefing characters without changing what the reader does next has spent the budget that a better statement needed.

The digest gate (issue #317) labeled 89 reference statements over 15 datasets as must (28) or should (61): statements a good briefing is expected to carry, and statements it should carry when there is room. Those labels are the test data for any proposed ranking rule. A rule that cannot reproduce the labels' broad shape on the gate's own tables has no claim to govern selection on unseen tables.

Issue #347 records the cornerstone the rule must respect: interestingness is scored against what the reader can do. Three layers of action were distinguished there. Data actions (recode a column, drop a duplicate, split a series) and modeling actions (add a term, fit a slice separately, treat missingness as signal) are knowable from the data alone. World actions (raise prices, change a campaign) enter only through a declared purpose, which the default pass does not have. A statement is actionable for an agent reader only if it carries the handles the action needs: the exact columns, levels, rows, or dates.

The candidate frame put to this study is compression: a profile is a compressed description of the dataset, the frame account is its header, a finding is a term that shortens the description of what remains, and interestingness per character is the briefing's selection problem. Section 4 tests that frame as a ranking rule. It does not survive as the whole rule; section 7 states what remains of it.

A different taxonomy already exists in `docs/benchmarks/2026-10-28-interestingness-taxonomy.md`: it classifies structures worth detecting. This study classifies actions worth licensing and ranks statements for a reader. The detection taxonomy is not changed by anything here.

## 2. The data-actions taxonomy (issue #349)

An action is classified by the decision it settles for the analyst who receives the statement — the question that is closed by acting on it. Reversibility is the secondary axis: it sets how much care the action demands, not which family it belongs to. Seven families result. Each is stated in one plain line; a family that could not be stated in one line would be suspect, and none failed that test.

**Admit or refuse.** The decision: whether a column, a set of values, a set of rows, or a period enters the analysis at all. One line: *this does not enter the analysis as it stands.* Reversibility: the only family whose careless execution destroys information — deleting rows or overwriting values cannot be undone from the table alone; executed as an exclusion flag or as a recode that keeps the original values, it is reversible. Example: `sentinel_values` — in bank-marketing, the value −1 in `days_since_previous_campaign_contact` is a code for "no previous contact", not a duration; it is recoded before the column is used as a measure.

**Identify.** The decision: what a column or a value is — its role, its key status, whether two names are one variable. One line: *this is what this column is.* Reversibility: a declaration; fully reversible. Example: `duplicate_encoding` — in adult-census-income, `education` and `education_num` are one variable under a value mapping.

**Re-express.** The decision: the same information in a form that can be computed on — a parse, a reshape, a normalization of formats. One line: *same information, usable form.* Reversibility: deterministic; the original is recoverable. Example: `long_format` — the frame is a pivot of a wider table (an attribute column's levels and a measure column's values) and is reshaped before cross-measure work.

**Partition.** The decision: where one pooled analysis becomes two — the rows on either side of the split are not summarized together. One line: *these rows are not one population.* Reversibility: a grouping choice; reversible. Example: `level_shift` — a series is split at the shift, and the two levels are never averaged together.

**Simplify.** The decision: what can be removed while losing nothing, because what remains determines it. One line: *this can go; nothing is lost.* Reversibility: lossless by construction, with one exception — removing duplicate rows is lossless only if the duplication is not itself the recording error under investigation. Example: `arithmetic_identity` — in bike-sharing-hourly, `cnt` is exactly `casual + registered`; one side of the identity is redundant.

**Specify.** The decision: how a model of the data must bend — a term, a shape, an interaction, a slice fitted on its own, missingness used as signal. One line: *the model needs this term, this shape, or this slice.* Reversibility: a modeling choice; fully reversible. Example: `axis_shape` — a dependence that rises along an ordered axis and then flattens becomes a saturating term, not a straight line.

**Escalate.** The decision: what leaves the table — a value to verify at the source, a provenance question to ask the owner, a window to monitor, more data to collect. One line: *this cannot be settled from the table; it goes back to the source.* Reversibility: no data change; the decision is deferred to whoever owns the source. Example: `benford_deviation` — leading digits depart from Benford's law; the table cannot say why.

Three groupings of the families matter for the ranking rule. Admit or refuse, partition, simplify, and specify are **analysis-changing**: acting on them changes the dataset or the model. Identify and re-express are **bookkeeping**: they settle what the data are, and their facts are the frame account's own content. Escalate is **off-table**: it settles nothing in the data and opens a question elsewhere. A statement is **action-free** when it licenses no action in any family. The category is part of the vocabulary; section 3 shows it is empty at the level of finding types, and section 5 shows it is not empty at the level of individual statements.

## 3. The mapping test

Every finding type returned by `onceover.taxonomy()` on the base commit (65 patterns, 64 types) was mapped by hand to the primary family whose decision a finding of that type settles, with a secondary family recorded where the finding licenses a second action. The mapping was executed as a check: a script asserts that the mapped set equals the taxonomy's type set exactly, so an unmapped or misspelled type fails the run.

Outcome: all 64 types map to at least one family. Primary-family counts: escalate 19, specify 15, partition 11, simplify 7, identify 6, admit or refuse 3, re-express 3. No family is empty, so no family is removed, and no detector gap follows from an empty family. No type is action-free. The full mapping is Appendix A.

Two consequences follow.

First, actionability cannot serve as a pass/fail test for statements. Every type the library can state licenses an action, so "is it actionable" separates nothing. What varies is the family, whether the action is primary or secondary, and whether the action changes the analysis. The ranking rule in section 7 is built on those three, not on a binary.

Second, one gap inside escalate. The family's *verify* members have finding types in number (`heaping`, `benford_deviation`, `extreme_value`, and sixteen more). Its *ask* and *collect* members have none: no finding type states a question to the data's owner, and none states that more data are needed. The library states facts that force the question and leaves the question itself unstated. That is a gap in how recommendations are rendered, inside issue #349's own scope — the recommendation layer — not a missing detector. No new issue is filed for it; it is recorded here and in the summary comment on #349.

## 4. The compression test

**Setup.** The gate's 89 reference statements were matched against default-mode profiles of their 15 datasets, using the gate's own matching (`benchmarks/gate/components/reference.py`). The replication built for this study agreed with the scorecard on main on all 267 statement×scope cells, and the gate's checks failed on no dataset (check_failed 0 throughout). 65 statements are matched in at least one briefing scope (21 must, 44 should), 55 of them at the 4,000-character scope. The remaining 24 (7 must, 17 should) are matched in no scope: the profile does not state them at all. Those are detection and coverage gaps, not selection outcomes; they are excluded from the ranking and counted separately wherever they matter.

**The background.** A statement's gain is priced against the reader's state after the frame account: every column's own marginal distribution — missing rate, level shares, mean and spread, top shares — plus roles and counts, and no joint structure between columns. This is the maximum-entropy background with the marginals as constraints (section 6). A statement's **gain** is the log-likelihood ratio, in bits, of the data under the model that adds the stated structure against that background, computed on the statement's columns from its check expression and measured values. A statement that restates a marginal earns 0 by construction: the background already carries it.

**Gain formulas, by statement shape.** Numeric pair: −(n/2)·log2(1 − r²), r the Pearson correlation on complete pairs. Group against numeric: (n/2)·log2(σ²_total / σ²_within). Categorical pair: n × mutual information, plug-in estimate with Miller–Madow correction, floored at 0. Time split: two-segment Gaussian log-likelihood ratio, each segment's mean and variance free. Exact determination: n × H(determined column), H the empirical entropy of its observed values. Missingness pattern: n × I(missing indicator; the pattern variable). Single cell against independence: n × KL(cell share ‖ independence expectation); a measure-weighted cell uses the Kish effective sample size of the weight in place of n. Disjoint ranges: log2 C(nA + nB, nA). Order violations: n × KL(observed violation rate ‖ rate under a permuted pairing, seed 0). Outliers: the flagged values' robust-z self-information, capped at 20 bits per value. Duplicated keys: Poisson deviance of the observed duplicated-key count against the independence (birthday) expectation. **Characters**: the statement's rendered line length at the tightest scope where it is matched.

**What the proxy loses.** Five losses are structural, not fixable by better arithmetic.

1. *Semantics.* A sentinel value is a code, and a distributional background prices codes at 0: the value's mass is a legitimate part of the marginal. Three of the four sentinel statements in the gate set are must-tier.
2. *Dilution.* A brief regime in a long series prices low, because the two-segment model charges one mean and one variance to the whole remainder. The panel's covid-spike statement prices at 48.5 bits over the 608-month cross-state mean series; the daily collapse prices at 214.6 bits over its full series.
3. *Nulls.* An independence statement prices at its measured dependence, a few bits at most (the three noise pairs of column_order_stress sum to 0.7 bits). The proxy cannot distinguish "verified independent" from "not examined" — both are ≈ 0 — and it prices marginal_ranges' nested ranges at 2.0 bits against 5,993.4 bits for its disjoint ranges, although the reassurance the two statements carry is the same kind.
4. *Exactness inflation across tables.* An exact identity prices at the full coding cost of the determined column, so on the pooled ranking a statement's price grows with its table's rows: the pooled comparison is partly a table-size comparison. Within one table this is defensible — the column really is removable — and section 7 uses the measure only within a table's own selection.
5. *Distributional form.* The ANOVA and correlation prices assume Gaussian within-group variation, and the mutual-information prices are plug-in estimates. For a zero-inflated measure (adult-census-income's `capital_gain` is 0 on 91.7% of rows) the price mixes the zero mass with the tail and should be read as an order of magnitude.

**Results.** Ranking the 65 matched statements by gain per character:

- Must statements: median rank 23 of 65; should statements: median rank 37.5. Top-half share 0.62 for must, 0.43 for should. Median gain per character 8.29 bits (must) against 1.40 bits (should). The probability that a randomly drawn must statement outranks a randomly drawn should statement (AUC) is 0.610.
- Within datasets — the comparison selection actually faces, since a briefing is filled from one table's statements — the separation nearly vanishes: percentile AUC 0.552.
- The top of the ranking is exact relations: rank 1 bike-sharing-hourly's `cnt`-identity (1,484.59 bits/char), rank 2 adult-census-income's education-encoding (1,084.39), rank 3 planted_mix's affine-identity (450.88), rank 4 messy_intake's fee-is-2pct (238.43). Seven of the top nine are simplify statements. This is what the MDL price genuinely says: an exact identity removes a column from the code.
- Hygiene kinds do not sink as a class. Role statements sink (11 statements, median rank 58, all in the bottom half) and sentinel statements sink (median rank 48), because their content is a marginal or a code. But encoding is rank 2 and identity holds ranks 1 and 3: the hygiene kinds whose content is an exact relation top the ranking. What sinks is not hygiene; it is restating the background.
- The sentinel statements are the sharpest failure. bank-marketing's days-since-sentinel (must) prices at exactly 0 bits and ranks 50 of 65, tied at the bottom with every other zero-gain statement. messy_intake's amount-sentinel (must) is not stated by the default profile at all. planted_mix's sentinel-subpopulation prices at 49.0 bits (rank 45) only because its check adds a joint clause — the coded rows approve at a different rate. A pure compression rule places a must-tier refusal, the statement that keeps every later number from being computed over a code, at the bottom of the briefing.
- Zero gain is common: 18 of the 65 matched statements price at 0 (4 of 21 must, 14 of 44 should). In the current 4,000-character briefings, 2,055 of the 9,632 characters that carry matched reference statements (21%) carry zero-gain statements.
- Redundancy cannot be priced by subtraction. The collapse trio on daily_level_collapse — collapse-2023, collapse-sustained, recovery — prices standalone at 214.6 + 214.6 + 251.6 = 680.8 bits; the first two price the identical split, a pure duplication. The joint three-segment model (normal, collapsed, recovered) earns 970.2 bits: more than the sum of the standalone prices, because a single split cannot represent two transitions. Marginal gain must be computed against a joint model for a cluster of related statements, or clusters must be selected as units; subtracting standalone prices from each other gives the wrong number in both directions.

**Verdict.** Compression gain per character, as the rule for which statements reach the reader first, is refuted on the gate's own labels: the separation is weak (AUC 0.610 pooled, 0.552 within tables) and the errors are at the decision-relevant end — exact relations over-weighted, refusals invisible. Two things survive. Gain per character is the only measured quantity that orders statements at all within a class of like actions, and section 7 keeps it there. And the two-part discipline — a statement must pay its characters out of the belief change it buys — remains the right accounting for belief change, even though belief change is not the whole of a statement's worth.

## 5. The action route

Issue #347's recorded measurement route is the complementary test: classify the gate's statements by the action families they license, and check whether the must tier concentrates in the actionable classes while the statements the current briefing displaces do not.

All 89 statements were classified by hand to a primary family under uniform rules: a driver or contrast statement settles a model term (specify), except a near-collinear pair, which settles a drop (simplify), an exact relation (simplify), a time-level or regime fact (partition), and a concept-scale fact (partition — it settles whether a measure may be pooled across incomparable scales). A missing-rate statement is escalate; a missingness pattern that marks a subpopulation is partition; a completeness null is action-free. A null statement takes the family of the concrete step it forecloses; a null that forecloses no concrete step is action-free. The full classification, with the basis for each statement, is Appendix C.

| Population | n | Analysis-changing | Bookkeeping | Escalate | Action-free |
|---|---|---|---|---|---|
| must | 28 | 19 | 3 | 4 | 2 |
| should | 61 | 41 | 14 | 5 | 1 |
| in the 4k briefing | 55 | 40 | 9 | 6 | 0 |
| displaced to wider scopes | 10 | 6 | 2 | 2 | 0 |
| in no scope | 24 | 14 | 6 | 1 | 3 |

The analysis-changing share is 68% among must statements and 67% among should statements: action class does not separate the tiers. Displaced statements have the same class mix as included ones (6 of 10 analysis-changing), so the current briefing does not displace bookkeeping preferentially either. Action class alone is therefore also refuted as the ranking rule — it is a vocabulary for stating the rule, not the rule.

Two separations do appear. All three action-free statements — the two no-missing statements (daily_level_collapse, daily_regime_break) and marginal_ranges' level-spread — are absent from every briefing scope: the library already treats them as frame-account content, which is where section 7's rule puts them by construction. And the unmatched set is majority analysis-changing (14 of 24): most of what the gate wants and the profile does not state is a detection gap, not a selection failure. The largest single block is gse15258-expression, with four unmatched must statements (das28-improvement, das28-na, geo-restates-sample, probe-1556053-response) — the pointed-mode problem of issue #310, not a ranking problem.

## 6. Literature

Each source is cited for what this study takes from it and what it rejects. Bibliographic details were verified by web search on 2026-11-17, except where marked.

**Silberschatz & Tuzhilin, "What Makes Patterns Interesting in Knowledge Discovery Systems," IEEE Transactions on Knowledge and Data Engineering 8(6): 970–974, 1996 (DOI 10.1109/69.553165); companion paper at KDD 1995, pp. 275–281.** They argue interestingness is subjective — judged against a user's beliefs and goals — and propose two criteria: unexpectedness and actionability. *Taken:* the two axes themselves, and the subjectivity: a statement's worth is scored against a stated reader state (here, the frame account) and a stated set of open actions (here, the taxonomy). *Rejected:* their proposal to capture actionability through unexpectedness, on the grounds that actionable patterns tend to be unexpected. The gate measurements reject the proxy in both directions: the most actionable statements (the refusals) carry zero distributional unexpectedness, and the most unexpected statements (exact identities) are not the most decision-relevant.

**Geng & Hamilton, "Interestingness measures for data mining: A survey," ACM Computing Surveys 38(3), article 9, 2006 (DOI 10.1145/1132960.1132963).** The survey classifies measures as objective (data alone), subjective (against a user's beliefs), and semantic (against the meanings of the attributes), and lists nine criteria for rules, among them conciseness, peculiarity, novelty, surprisingness, utility, and actionability. *Taken:* the three-way split, which maps onto this study's machinery — the gain of section 4 is an objective measure; the background makes it subjective in their sense; and the sentinel result is precisely a semantic fact that no objective measure prices, which is why the rule needs the taxonomy beside the gain. *Rejected:* nothing structural; but the nine criteria are not jointly operational — several overlap, and none is defined against a reader's decision set, so the list cannot be turned into a selection rule without the additions made here.

**De Bie, "Maximum entropy models and subjective interestingness: an application to tiles in binary databases," Data Mining and Knowledge Discovery 23(3): 407–446, 2011 (DOI 10.1007/s10618-010-0209-3); Kontonasios, Vreeken & De Bie, "Maximum Entropy Models for Iteratively Identifying Subjectively Interesting Structure in Real-Valued Data," ECML PKDD 2013.** Background knowledge is encoded as constraints; the maximum-entropy distribution satisfying them is the reader's state; a pattern is interesting in proportion to how much it contrasts with that state, per unit of the effort to describe it (the Information Ratio). After a pattern is told, the background is updated to include it, so variations of it stop being interesting. The framework is named FORSIED in the EDBT/ICDT 2019 workshop literature (attested in secondary sources; the primary FORSIED paper was not fetched for this study — marked unverified). *Taken:* the construction itself — section 4's background is the maximum-entropy completion of the marginals, the gain per character is the Information Ratio, and the updating requirement is why section 4 prices the collapse cluster jointly instead of subtracting standalone prices. *Rejected:* the single-number ambition. One ratio for all patterns assumes the reader's only cost is description effort. It has no place for a statement whose distributional information is zero and whose decision content is total — a refusal — because a refusal is not a pattern in the data; it is a fact about what the data are.

**Itti & Baldi, "Bayesian surprise attracts human attention," Vision Research 49(10): 1295–1306, 2009 (conference version, NIPS 2005, pp. 547–554); Baldi & Itti, "Of bits and wows," Neural Networks 23: 649–666, 2010.** Surprise is the Kullback–Leibler divergence between an observer's posterior and prior over models; in their experiments 72% of gaze shifts went toward above-average-surprise locations. *Taken:* the form of the belief-change term — a divergence between the reader's state before and after the statement; section 4's log-likelihood ratio against the background is its frequentist twin. *Rejected as a selection rule:* surprise is defined over the observer's model space for the distribution. A sentinel value is not surprising to a distributional model — it is an ordinary mass point — yet it is the first thing a reader must know. Surprise measures belief change about the distribution; a briefing's first job is often belief change about what the columns are, which the distribution does not carry.

**Rissanen, "Modeling by shortest data description," Automatica 14(5): 465–471, 1978 (DOI 10.1016/0005-1098(78)90005-5); Grünwald, The Minimum Description Length Principle, MIT Press, 2007.** The MDL principle: the best model of data is the one giving the shortest total description — the model plus the data encoded with its help. *Taken:* the two-part discipline. A statement must pay for its own length (its characters) out of the compression it buys (its gain); that is the gain-per-character form, and it correctly prices exact identities at the top, because an identity really does remove a column from every later description. *Rejected:* total description length as the briefing's objective. A briefing is not a code for the data — no reader will decompress it — so minimizing total length fills it with identities and collinearities (seven of the top nine prices in section 4 are simplify-family statements) and drops every refusal. Compression is the right price for belief change and the wrong objective for selection.

## 7. The rule

Sections 4 and 5 refute each candidate as the whole rule: compression gain per character ranks refusals last, and action class does not separate the tiers at all. An arithmetic combination — a weighted sum of class and efficiency — would need an exchange rate between, say, a refusal and a driver statement, and no measurement in this study supplies one. The rule is therefore a lexicographic order: each measure is used only where the tests showed it works. Family decides the order of kinds; gain per character orders statements inside a kind; facts the background already carries are not given finding lines at all.

**Inputs**, all computed from the report and the frame account:

- *family(statement)*: the primary action family of the statement's finding type (Appendix A), with statement-level overrides where an instance's action differs from its type's usual action (Appendix C is the worked set of overrides for the gate statements).
- *objects(statement)*: the columns, values, row sets, and periods the statement's claim and evidence name. Operational for columns and values, which are fields of the finding; for row sets and periods it depends on the finding's evidence naming the filter — **partially operational, marked**.
- *gain(statement | already selected)*: the log-likelihood ratio in bits against the independence background of section 4, made marginal against the statements already selected. Operational today for identical split points (increment 0) and for nested time splits (the joint Gaussian model of section 4); elsewhere the standalone gain is used — **partially operational, marked**. No other component of the rule is non-operational.
- *chars(statement)*: the rendered line length.

**Selection.** A briefing of budget B is filled in this order. Each statement is charged its rendered line length and renders at most once. A statement renders only if it names its objects exactly — columns, levels, rows, dates; a statement without handles is not actionable for an agent reader, whatever its family.

1. **Refusals first.** An admit-or-refuse statement enters before everything else when any other statement in the report uses an object it qualifies — the same column, the same value, the same period. A driver statement computed over a sentinel value the reader has not been warned about is not a less interesting statement; it is a wrong one. A refusal whose object nothing else uses takes its place in step 3 instead.
2. **Analysis-changing statements by marginal efficiency.** Partition, specify, and simplify statements are ranked by marginal gain per character and fill the budget greedily in that order.
3. **Off-table statements.** Escalate statements, ranked by the same efficiency. An escalate statement whose content is a rate — a missingness rate, a completeness rate — does not render as a finding line: the frame account states rates once. An escalate statement that names a specific object (a date, a set of rows, a value) renders here.
4. **Bookkeeping as account, not findings.** Identify and re-express statements do not render as finding lines; their facts live in the frame account and the column cards. Two exceptions render in step 2's position, charged normally: an identify statement that licenses a removal (a duplicate encoding — one of the pair can be dropped), and an identify statement for a column that a statement selected in steps 1–2 names, where the column's name does not carry its meaning.
5. **Action-free statements never render.** Completeness and no-defect claims are properties of the frame account; the account states them once or not at all.

**Declared purpose.** The rule as stated ranks on data actions and modeling actions (layers 1 and 2 of issue #347). World actions (layer 3) enter only through a declared purpose, which the default pass does not have: in pointed mode, a purpose that names an object promotes the escalate statements about that object into step 1's position. That extension is stated here so the rule has a defined place for it; nothing in this study measures it.

## 8. What the rule changes: adult-census-income

The rule applied by hand to the current 4,000-character briefing of adult-census-income (48,842 rows × 15 columns, 91 findings; the briefing currently uses 3,889 characters). The table has eight reference statements; seven are rendered today, on four lines totaling 863 characters.

- **Stays, and its position becomes a requirement:** `capital-gain-99999` (admit or refuse, 367 characters) — the pile of 244 values at 99999 in `capital_gain` is a code, not an amount. It qualifies `capital_gain`, which the income line below uses as a driver. In the current briefing the pile line already precedes the income line; under step 1 of the rule that order is mandatory, not an accident of the section layout.
- **Stays:** the income line (235 characters) carrying four specify statements — income-marital, income-education, income-capital-gain (must) and income-age-hours (should) — combined standalone gain 15,868.6 bits.
- **Stays, by the bookkeeping exception:** education-encoding (132 characters). Its family is identify, but it licenses a removal (one of `education`, `education_num` can be dropped) and the income line names `education_num`, whose name does not carry its meaning.
- **Stays, last in its class:** native-country-concentration (129 characters) — simplify family at 0 bits per character. It enters because its class precedes escalate in the fill order, and it is the weakest line the rule keeps on this table. Sensitivity: had this statement been classified identify — its fact is the population's composition, a marginal — it would move to the account and free its 129 characters. Appendix C records it as simplify because the action it licenses is to pool the tail levels or drop the column.
- **Enters:** occupation-workclass-missing (221 characters) — escalate family. `occupation` and `work_class` are missing on the same rows: mutual information 0.3147 bits per row, gain 15,369.9 bits, 69.55 bits per character, the most efficient escalate statement in the gate set. The current briefing carries only the account line "missing values in 3 column(s), most in occupation (5.8%)", which states the rate and not the joint fact — and the joint fact is the one that says the two gaps are a single recording event, not two.

Character accounting: reference-statement lines go from 863 to 1,084 characters (+221). The briefing stands at 3,889 of 4,000, so the entering line is paid from the tail: the last rendered finding line — the anomalies line, 324 characters, naming the three Married-AF-spouse rows inside `education` = Some-college and `relationship` = Husband — no longer fits and moves to the omitted list (19 omitted facts become 20), leaving the briefing at 3,786 characters. The 214 characters of slack would be refilled by the digest's normal order from the omitted facts; those lines were not priced in this study. No reference statement leaves this table's briefing under the rule.

## 9. What this study does not decide

- **Detection gaps.** The 24 statements matched in no scope are not selection failures and the rule does not address them. The largest block is gse15258-expression (four must statements). They belong to the pointed-mode work (issue #310) and the corpus work, not to this study.
- **Section structure.** The rule orders the fill of a briefing. Whether the briefing keeps its current sections (What is wrong, What is redundant, Other relationships, Anomalies) is a rendering decision the rule does not fix; the rule's steps cut across those sections.
- **Where recommendations live.** Issue #349 leaves open whether a recommendation is a field on the finding or a derived layer over findings. The rule consumes action families as statement metadata and is compatible with either. The escalate ask/collect gap of section 3, however, can only be closed by a derived layer: no finding states a question, so a layer that derives questions from findings is the only place they can be produced.

## 10. Evaluation protocol

A later implementation of the rule is judged on the gate, with the tiers treated as labeled judgments of briefing-worthiness. Because the rule deliberately moves rate facts from finding lines to the account, fact-recall (the fact appears anywhere in the briefing, account lines included) is scored separately from line-recall (the fact renders as a finding line). Selection is scored only over statements the report actually contains; detection is scored by the gate's existing machinery, not here.

On the 15 gated tables, at the 4,000-character scope, an implementation passes if all three hold:

1. **Must coverage does not regress, and the displaced must enters.** Every must statement the report contains has its fact in the briefing (21 of 21 on the base build, counting account lines; the current briefing holds 20), and every contained analysis-changing must statement renders as a finding line (18 of 18; the current briefing renders 17 — planted_mix's channel-driver is displaced to wider scopes).
2. **Zero-gain spending is capped.** Finding lines with zero compression gain that are not refusals take at most 5% of the characters carrying reference statements (current: 16% — 1,548 of 9,632 characters).
3. **Should coverage does not pay for 1 and 2.** Should-tier line-recall at 4k stays at or above the current 35 of 61.

The same three metrics are then computed on the gate's seven held-out datasets, with their reference statements written before the implementation is tuned, so the rule cannot be fitted to the gated tables.

**Second test — the Chicago answer key (issue #308).** The two selection-relevant statements of the manual pass over the food-inspections file — the fail-rate contrast by `inspection_type` (Complaint 45.2% against Canvass 27.3% among Pass/Fail outcomes) and the risk-class null (fail rates 24.8%, 25.3%, 24.9% across the three risk classes) — are both specify-family: one settles a group term on the outcome, the other forecloses one. On a profile that states them (stating them is issue #308's own detection work), the rule must place both in the pointed briefing ahead of any bookkeeping line. If it does not, the family definitions are wrong, not the tuning.

**Falsification.** The rule is falsified if an implementation meeting conditions 1–3 is beaten by the current digest on the held-out tables under the tier labels — in particular if must line-recall falls — or if the class order has to be inverted to pass. The cornerstone itself is falsified if agent task tests of the kind issue #309 defines (an agent with the briefing against an agent without it, on unseen tables) show no improvement in task accuracy or token cost from briefings selected by actionability: in that case actionability is the wrong basis for ranking, and not merely this rule's version of it.

**Reproducibility.** Measurements were run on 2026-11-17 against main at `610812ef`, in the study's scratch directory, with five scripts: `measure_gate.py` (profiling and matching, replicating `benchmarks/gate/components/reference.py`), `gains.py` (the gain computations of section 4, in Polars), `analyze.py` (the rankings and class counts of sections 4 and 5), `classification.py` (the statement classification of Appendix C), and `mapping.py` (the coverage check of section 3). Appendix B reproduces every per-statement input to sections 4 and 5 — gain, characters, rank, family — so the reported separations can be recomputed from this document alone.

## Appendix A. Finding types mapped to action families

The mapping test of section 3: all 64 types from `onceover.taxonomy()`, each with its primary family, a secondary family where the finding licenses a second action, and the decision a finding of that type settles.


| Finding type | Primary family | Also licenses | The decision it settles |
|---|---|---|---|
| `affine` | simplify | — | the target is a rescaling of the input: keep one |
| `arithmetic_identity` | simplify | — | the target is an exact combination of its parts: keep one side |
| `axis_shape` | specify | — | the shape along the axis (monotone, saturating) becomes the term's form |
| `benford_deviation` | escalate | — | leading digits depart from Benford's law: verify the values' provenance |
| `boundary_mass` | specify | admit or refuse | the pile at the boundary is modeled as censoring, or the piled value is a cap code to recode |
| `categorical_outlier` | escalate | specify | rows breaking a near-universal context are inspected |
| `categorical_role` | identify | — | the column is a grouping variable, not a measure |
| `column_ordering` | identify | — | the columns follow a stated order, which is what the column positions mean |
| `concentration_over_time` | partition | escalate | the top holders change by period: each period's dominant slice is its own segment, and the change is monitored |
| `conditioned_association` | specify | — | the association reverses or vanishes within the stratifier: the pooled term is replaced by conditioned terms |
| `content_anomaly` | re-express | escalate | values that do not parse are repaired to the column's format |
| `digit_preference` | escalate | — | terminal digits are not uniform: verify how the values were recorded |
| `duplicate_encoding` | identify | simplify | two columns are one variable under a mapping; one can then be dropped |
| `duplicate_rows` | simplify | escalate | exact repeats are removed; if the repetition is itself a recording error, that is checked first |
| `entry_block` | escalate | partition | a block of entities enters in one period: verify the onboarding event |
| `exit_block` | escalate | partition | a block of entities leaves in one period: verify the exit event |
| `extreme_value` | escalate | admit or refuse | the isolated value is verified, or its row is excluded |
| `first_activity` | escalate | partition | entities activate after their first period: verify the activation process |
| `format_variation` | re-express | — | the mixed formats are normalized to one parse |
| `functional` | simplify | — | the inputs determine the target: keep one side |
| `gap` | escalate | admit or refuse | a stretch of the axis is missing: verify collection, or restrict the period out |
| `group_contrast` | partition | specify | the groups differ enough to analyze apart, or with group terms |
| `heaping` | escalate | specify | values pile on round numbers: verify the measurement process before modeling the pile |
| `hotspot` | partition | — | the cell's rows are a distinct subpopulation, analyzed apart from the rest |
| `identifier_role` | identify | — | the column names rows; it is never a measure |
| `if_then` | specify | — | the rule becomes a slice-specific term or a separate fit on the matching rows |
| `interval_variation` | specify | escalate | the cadence changes: resample or weight before comparing stretches |
| `joint_distribution_change` | partition | — | the joint distribution changes at the named point: the stretches are not pooled |
| `leakage_signature` | admit or refuse | — | the column is refused as a model input: it encodes the outcome |
| `level_contrast` | specify | — | the level's location or scale differs: a level-specific term |
| `level_shift` | partition | — | the series is split at the shift: the two levels are not pooled |
| `long_format` | re-express | — | the frame is a pivot: reshape to wide before cross-measure work |
| `missing_combinations` | specify | escalate | the empty level combinations constrain which interactions can be fitted |
| `missing_values` | escalate | admit or refuse | whether the gaps are a recording failure to chase, or the column is too empty to use |
| `modal_share` | simplify | partition | one level dominates: pool the remaining levels or drop the column |
| `model_failure` | specify | escalate | the model must bend in the named slice, or the slice's data are checked |
| `near_duplicate_rows` | simplify | escalate | near repeats are merged or removed under the stated key |
| `one_way_flag` | identify | specify | the flag is an absorbing state: once on, always on, which is what the flag means |
| `order_constraint` | escalate | specify | the row-wise order is a validity rule: future violations are errors to catch |
| `order_violation` | escalate | — | the violating rows are recording errors to check |
| `ordered_partition` | partition | — | the recursive split of the axis defines the segments to analyze apart |
| `outlier_entity` | escalate | partition | one entity's total sits in the tail: inspect the entity |
| `partial_period` | admit or refuse | — | the trailing period is incomplete: it is excluded from rate computations |
| `period_association` | partition | — | the association differs by period: periods are not pooled |
| `predictable_missingness` | specify | partition | the missingness indicator enters the model as a term |
| `predictive` | specify | — | the fitted dependence (curve, step, interaction) becomes a model term |
| `predictor_drift` | escalate | partition | the recent window's distribution moved: verify the feed, or fit recent data apart |
| `regime_contrast` | partition | — | the named regimes are compared as separate populations, never pooled into one level |
| `residual_shift` | specify | partition | the model's residuals shift level: a segment term is added |
| `row_inconsistency` | escalate | — | rows breaking a relation the rest obey are pulled and checked |
| `sentinel_values` | admit or refuse | — | the coded values are recoded to null or the column is not used as measured |
| `sequence_gap` | escalate | admit or refuse | sequence values are missing: verify, or restrict the range out |
| `series_structure` | specify | — | autocorrelation and seasonality enter the model: seasonal terms, dependent errors |
| `slice_shift` | partition | specify | the slice's rows sit differently on the target: the slice is treated as its own population for the target |
| `slope_break` | specify | partition | a hinge term is added at the break: one population, a bent trend |
| `spike_cells` | escalate | partition | one cell sits far above the independence expectation: verify the cell before treating it as a segment |
| `spike_contrast` | escalate | partition | the spike periods are inspected against their surroundings |
| `statistical` | specify | — | the association becomes a term, or is recorded as absent so no term is added |
| `step_discontinuity` | specify | partition | a step term is added at the named point of the axis |
| `transient_excursion` | escalate | partition | the stretch outside the band is inspected before it is treated as a regime |
| `trend_change` | partition | — | the trend reverses: the stretches are separate regimes |
| `uniqueness` | identify | — | the named columns form the row key, which settles the grain |
| `value_concentration` | simplify | partition | one value dominates: pool the tail or drop the column |
| `variance_change` | partition | — | the spread changes: the stretches are separate regimes for any spread statement |

## Appendix B. Compression gains for the 65 matched statements

Gain and characters as defined in section 4, ranked by gain per character. Ranks inside the zero-gain tie (18 statements at 0.00 bits/char) are in dataset order and carry no meaning. The action family is the statement-level classification of Appendix C.


| Rank | Statement | Tier | Kind | Gain (bits) | Chars | Bits/char | Action family |
|---|---|---|---|---|---|---|---|
| 1 | bike-sharing-hourly / cnt-identity | must | identity | 154,397.6 | 104 | 1,484.59 | simplify |
| 2 | adult-census-income / education-encoding | must | encoding | 143,139.9 | 132 | 1,084.39 | identify |
| 3 | planted_mix / affine-identity | should | identity | 55,007.6 | 122 | 450.88 | simplify |
| 4 | messy_intake / fee-is-2pct | should | relates | 27,896.8 | 117 | 238.43 | simplify |
| 5 | bike-sharing-hourly / temp-atemp | should | relates | 46,496.2 | 270 | 172.21 | simplify |
| 6 | column_order_stress / a-collinearity | should | relates | 16,617.5 | 139 | 119.55 | simplify |
| 7 | abalone / size-collinear | must | relates | 18,030.5 | 177 | 101.87 | simplify |
| 8 | column_order_stress / b-collinearity | should | relates | 12,729.7 | 139 | 91.58 | simplify |
| 9 | column_order_stress / a3-negative | should | relates | 12,617.6 | 157 | 80.37 | specify |
| 10 | adult-census-income / occupation-workclass-missing | should | missingness | 15,369.9 | 221 | 69.55 | escalate |
| 11 | dominant_driver / rooms-collinear | should | relates | 7,402.4 | 170 | 43.54 | simplify |
| 12 | planted_mix / labor-total | should | relates | 4,833.5 | 122 | 39.62 | specify |
| 13 | daily_regime_break / signups-track | should | relates | 3,745.2 | 101 | 37.08 | specify |
| 14 | bike-sharing-hourly / cnt-hour | must | relates | 8,727.0 | 246 | 35.48 | specify |
| 15 | dominant_driver / size-dominates | must | relates | 5,899.8 | 170 | 34.70 | specify |
| 16 | adult-census-income / income-marital | must | relates | 7,664.0 | 235 | 32.61 | specify |
| 17 | sec-edgar-company-facts / concept-scale | must | relates | 5,297.5 | 192 | 27.59 | partition |
| 18 | daily_level_collapse / conversions-track | should | relates | 2,172.8 | 107 | 20.31 | specify |
| 19 | adult-census-income / income-education | must | relates | 4,486.1 | 235 | 19.09 | specify |
| 20 | bank-marketing / subscription-duration | must | relates | 5,517.6 | 294 | 18.77 | specify |
| 21 | planted_mix / followup-missingness | must | missingness | 2,398.2 | 161 | 14.90 | partition |
| 22 | column_order_stress / mix1-relates | should | relates | 1,913.1 | 177 | 10.81 | specify |
| 23 | abalone / rings-size | must | relates | 1,508.4 | 182 | 8.29 | specify |
| 24 | adult-census-income / income-age-hours | should | relates | 1,921.2 | 235 | 8.18 | specify |
| 25 | bank-marketing / subscription-month-contact | should | relates | 1,580.4 | 197 | 8.02 | specify |
| 26 | adult-census-income / income-capital-gain | must | relates | 1,797.3 | 235 | 7.65 | specify |
| 27 | bike-sharing-hourly / cnt-year | should | relates | 812.4 | 117 | 6.94 | partition |
| 28 | abalone / components-exceed-whole | should | order | 1,343.2 | 205 | 6.55 | escalate |
| 29 | bank-marketing / subscription-previous | should | relates | 1,136.6 | 226 | 5.03 | specify |
| 30 | state-unemployment-long / missing-month | must | completeness | 544.6 | 155 | 3.51 | admit_refuse |
| 31 | daily_regime_break / level-shift-2024 | should | relates | 882.5 | 266 | 3.32 | partition |
| 32 | bank-marketing / previous-contact-february-concentration | should | concentration | 804.5 | 298 | 2.70 | partition |
| 33 | abalone / rings-sex | must | relates | 646.3 | 254 | 2.54 | specify |
| 34 | planted_mix / channel-driver | must | relates | 422.4 | 171 | 2.47 | specify |
| 35 | daily_regime_break / variance-break-2022 | should | relates | 524.1 | 266 | 1.97 | partition |
| 36 | daily_level_collapse / recovery | should | relates | 251.6 | 157 | 1.60 | partition |
| 37 | daily_level_collapse / weekly-cycle | should | relates | 223.6 | 157 | 1.42 | specify |
| 38 | daily_level_collapse / collapse-2023 | should | relates | 214.6 | 157 | 1.37 | partition |
| 39 | daily_level_collapse / collapse-sustained | should | relates | 214.6 | 157 | 1.37 | partition |
| 40 | daily_regime_break / weekly-cycle | should | relates | 349.6 | 266 | 1.31 | specify |
| 41 | bank-marketing / subscription-housing-loan | should | relates | 629.0 | 594 | 1.06 | specify |
| 42 | sec-edgar-company-facts / balance-sheet-never-negative | must | — | 100.1 | 192 | 0.52 | escalate |
| 43 | state-unemployment-long / covid-spike | should | relates | 48.5 | 154 | 0.32 | partition |
| 44 | abalone / height-extreme | should | outlier | 46.1 | 264 | 0.17 | escalate |
| 45 | planted_mix / sentinel-subpopulation | must | sentinel | 49.0 | 372 | 0.13 | admit_refuse |
| 46 | dominant_driver / age-weak | should | relates | 34.5 | 275 | 0.12 | specify |
| 47 | daily_regime_break / spike-day | should | outlier | 20.0 | 266 | 0.07 | escalate |
| 48 | adult-census-income / capital-gain-99999 | should | sentinel | 0.0 | 367 | 0.00 | admit_refuse |
| 49 | adult-census-income / native-country-concentration | should | concentration | 0.0 | 129 | 0.00 | simplify |
| 50 | bank-marketing / days-since-sentinel | must | sentinel | 0.0 | 140 | 0.00 | admit_refuse |
| 51 | bike-sharing-hourly / instant-index | should | role | 0.0 | 140 | 0.00 | identify |
| 52 | state-unemployment-long / complete-grid | should | role | 0.0 | 115 | 0.00 | identify |
| 53 | south-german-credit / foreign-worker-concentration | should | concentration | 0.0 | 118 | 0.00 | simplify |
| 54 | sec-edgar-company-facts / fiscal-period-missing | must | missingness | 0.0 | 278 | 0.00 | escalate |
| 55 | gse15258-expression / sample-key | should | role | 0.0 | 88 | 0.00 | identify |
| 56 | gse15258-expression / disease-constant | should | role | 0.0 | 90 | 0.00 | simplify |
| 57 | column_order_stress / key | should | role | 0.0 | 85 | 0.00 | identify |
| 58 | planted_mix / key | should | role | 0.0 | 93 | 0.00 | identify |
| 59 | daily_regime_break / complete-series | should | role | 0.0 | 72 | 0.00 | identify |
| 60 | dominant_driver / hoa-missingness | must | missingness | 0.0 | 76 | 0.00 | escalate |
| 61 | dominant_driver / key | should | role | 0.0 | 89 | 0.00 | identify |
| 62 | daily_level_collapse / complete-series | should | role | 0.0 | 72 | 0.00 | identify |
| 63 | marginal_ranges / key | should | role | 0.0 | 85 | 0.00 | identify |
| 64 | messy_intake / key | should | role | 0.0 | 88 | 0.00 | identify |
| 65 | messy_intake / notes-blank | must | missingness | 0.0 | 74 | 0.00 | escalate |

## Appendix C. The 89 gate statements classified by action family

The classification behind section 5, with the basis for each statement.

| Statement | Primary family | Basis |
|---|---|---|
| abalone / components-exceed-whole | escalate | rows where a part exceeds the whole are recording errors to check |
| abalone / height-extreme | escalate | an isolated extreme and zeros: verify the values |
| abalone / rings-sex | specify | rings differs by sex: a group term |
| abalone / rings-size | specify | rings rises with a size measure: a driver term for rings |
| abalone / size-collinear | simplify | length and diameter near-collinear: drop one |
| adult-census-income / capital-gain-99999 | admit_refuse | the 99999 pile is a code, not an amount: recode before use |
| adult-census-income / education-encoding | identify | two encodings of one variable |
| adult-census-income / income-age-hours | specify | income depends on age or hours: a term |
| adult-census-income / income-capital-gain | specify | income depends on capital_gain: a term |
| adult-census-income / income-education | specify | income depends on education: a term |
| adult-census-income / income-marital | specify | income depends on marital status: a term |
| adult-census-income / native-country-concentration | simplify | one level holds 91.3%: pool the tail or drop the column |
| adult-census-income / occupation-workclass-missing | escalate | two columns missing on the same rows: one recording event to annotate |
| bank-marketing / days-since-sentinel | admit_refuse | -1 is a code for no previous contact: recode before use |
| bank-marketing / previous-contact-february-concentration | partition | one recency x month cell is a distinct subpopulation |
| bank-marketing / subscription-duration | specify | subscription depends on contact duration: a term |
| bank-marketing / subscription-housing-loan | specify | subscription depends on housing_loan: a term |
| bank-marketing / subscription-month-contact | specify | subscription depends on month or contact: a term |
| bank-marketing / subscription-previous | specify | subscription depends on prior campaign: a term |
| bike-sharing-hourly / cnt-hour | specify | cnt depends on hour: a term |
| bike-sharing-hourly / cnt-identity | simplify | cnt = casual + registered exactly: one side is redundant |
| bike-sharing-hourly / cnt-rush-hour-concentration | partition | the rush-hour working-day cell is a distinct regime |
| bike-sharing-hourly / cnt-temperature | specify | cnt rises with temperature: a term |
| bike-sharing-hourly / cnt-year | partition | the level grows from 2011 to 2012: do not pool the years for level estimates |
| bike-sharing-hourly / instant-index | identify | instant is the row number, dteday the axis |
| bike-sharing-hourly / temp-atemp | simplify | temp and atemp near-collinear: drop one |
| column_order_stress / a-collinearity | simplify | a1 and a2 near-collinear: drop one |
| column_order_stress / a3-negative | specify | a3 moves against a1: a signed term |
| column_order_stress / b-collinearity | simplify | b1 and b2 near-collinear: drop one |
| column_order_stress / key | identify | row_id is the key |
| column_order_stress / mix1-relates | specify | mix1 relates to a1: a term |
| column_order_stress / noise-independent | specify | the noise columns need no joint term (null; forecloses joint terms) |
| daily_level_collapse / collapse-2023 | partition | split the series at the collapse |
| daily_level_collapse / collapse-sustained | partition | the low regime is one segment of the partition |
| daily_level_collapse / complete-series | identify | date is the complete daily axis |
| daily_level_collapse / conversions-track | specify | conversions track sessions: a term (near-collinearity noted in the study) |
| daily_level_collapse / no-missing | action_free | asserts no defect; the frame account's null counts settle it |
| daily_level_collapse / recovery | partition | the recovery date ends the low segment |
| daily_level_collapse / weekly-cycle | specify | a weekly seasonal term |
| daily_regime_break / complete-series | identify | date is the complete daily axis |
| daily_regime_break / level-shift-2024 | partition | split the series at the shift |
| daily_regime_break / no-missing | action_free | asserts no defect; the frame account's null counts settle it |
| daily_regime_break / signups-track | specify | signups track visits: a term |
| daily_regime_break / spike-day | escalate | one planted spike day: inspect the window |
| daily_regime_break / variance-break-2022 | partition | the spread changes at 2022-07-01: two regimes |
| daily_regime_break / weekly-cycle | specify | a weekly seasonal term |
| dominant_driver / age-weak | specify | price falls weakly with age: a weak term, stated so it is not over-read |
| dominant_driver / hoa-missingness | escalate | an eighth of listings lack hoa_fee: a recording fact to annotate |
| dominant_driver / key | identify | listing_id is the key |
| dominant_driver / rooms-collinear | simplify | rooms near-collinear with size_sqft: drop one |
| dominant_driver / size-dominates | specify | size_sqft is the driver of price: the term that matters |
| dominant_driver / suburb-concentration | simplify | one suburb holds over a quarter of rows: pool the small suburbs |
| gse15258-expression / das28-improvement | specify | improvement differs by response group: a group term |
| gse15258-expression / das28-na | reexpress | das28_v3 must be parsed with 'NA' as null before use |
| gse15258-expression / disease-constant | simplify | a constant column distinguishes nothing: drop it |
| gse15258-expression / geo-restates-sample | identify | geo_accession restates sample_id: the same samples under two names |
| gse15258-expression / probe-1556053-response | specify | the probe differs by response group: a group term |
| gse15258-expression / sample-key | identify | sample_id is the key |
| marginal_ranges / inner-nested | identify | one band's range sits inside the other's |
| marginal_ranges / inner-wide-independent | specify | no joint term for the pair (null; forecloses a joint term the nesting invites) |
| marginal_ranges / key | identify | row_id is the key |
| marginal_ranges / level-spread | action_free | the level distribution is unremarkable; no concrete step changes |
| marginal_ranges / low-mid-disjoint | identify | the bands' ranges are disjoint: what the columns are relative to each other |
| marginal_ranges / low-mid-independent | specify | no joint term for the pair (null; forecloses a joint term the disjoint ranges invite) |
| marginal_ranges / mid-high-disjoint | identify | the bands' ranges are disjoint |
| messy_intake / amount-sentinel | admit_refuse | the -999 values are a code, not amounts: recode before use |
| messy_intake / code-flip | partition | status_code's meaning changed at 2024-01-01: split the file there |
| messy_intake / fee-is-2pct | simplify | fee is exactly 2% of amount: one side is redundant |
| messy_intake / key | identify | record_id is the key |
| messy_intake / notes-blank | escalate | notes blank on a quarter of rows: a recording fact to annotate |
| messy_intake / web-majority | simplify | one level holds over half of records: pool the rest |
| planted_mix / affine-identity | simplify | total_cost is an exact combination of its parts: one side is redundant |
| planted_mix / channel-driver | specify | approval differs by channel: a term |
| planted_mix / followup-missingness | partition | followup_days is missing exactly on the legacy rows: the missingness marks the subpopulation |
| planted_mix / hotspot-payout | partition | the kiosk x age 18-24 cell is a distinct subpopulation |
| planted_mix / key | identify | application_id is the key |
| planted_mix / labor-total | specify | total_cost rises with labor_cost: a term (the exact identity is stated separately) |
| planted_mix / sentinel-subpopulation | admit_refuse | -999 is a code on the legacy rows: recode before use |
| sec-edgar-company-facts / balance-sheet-never-negative | escalate | balance-sheet concepts are never negative: a validity rule; violations would be errors |
| sec-edgar-company-facts / concept-scale | partition | concept sets the scale of value: do not pool value across concepts |
| sec-edgar-company-facts / filed-after-period-end | escalate | filed always follows period_end: a provenance regularity to use as a validation rule |
| sec-edgar-company-facts / filing-key-not-unique | identify | the filing key does not identify a row: the grain is coarser than the key suggests |
| sec-edgar-company-facts / fiscal-period-missing | escalate | fiscal_period missing on about 1% of filings: a recording fact to annotate |
| south-german-credit / foreign-worker-concentration | simplify | one level holds 96.3% of rows: the column barely distinguishes |
| state-unemployment-long / complete-grid | identify | (state, month) is the key and the grid is complete |
| state-unemployment-long / covid-spike | partition | the COVID elevation is a separate regime: do not pool it with normal months |
| state-unemployment-long / missing-month | admit_refuse | 2025-10 is missing for every state: restrict that period out of trend computations |
| state-unemployment-long / state-comovement | specify | state rates move with the national cycle: a common-factor term |
| state-unemployment-long / state-spread | specify | long-run mean rates differ by state: a state term |
