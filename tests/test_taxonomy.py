"""The public taxonomy describes statements, independently of discovery methods."""
import random

import polars as pl
import pytest
import onceover
from _findings import select, metrics


def related_pair(n=2000):
    return pl.DataFrame({'segment':['premium' if i%3==0 else 'standard' for i in range(n)],
                         'shipping':['expedite' if i%3==0 else 'normal' for i in range(n)]})


def test_equivalent_rules_have_multiple_sources_and_one_statement():
    report=onceover.profile(related_pair())
    rules=[f for f in select(report,'conditional_rules',target='shipping')
           if f['specification']['outcome']['value']=='expedite'
           and f['specification']['conditions']==[{'column':'segment','operator':'eq','value':'premium'}]]
    assert len(rules)==1
    methods={e['method']['detector'] for e in rules[0]['evidence']}
    assert {'prism','association_rules','rule_evaluator'} <= methods
    assert sum(e['method']['detector']=='rule_evaluator' for e in rules[0]['evidence'])==1
    assert report.data['selection']['distinct_statements'] < report.data['selection']['verified_candidates']


def test_predictive_statements_merge_methods_without_losing_direction():
    report=onceover.profile(related_pair())
    statements=select(report,'dependencies',subtype='predictive',inputs=['segment'],target='shipping')
    assert len(statements)==1
    assert {'gbm','decision_tables'} <= {e['method']['detector'] for e in statements[0]['evidence']}
    assert select(report,'dependencies',subtype='predictive',inputs=['shipping'],target='segment')
    assert not any(f['category']=='related_columns' for f in report.findings)
    assert report.groups


def test_typed_predicates_keep_numeric_strings_and_boolean_outcomes():
    df=pl.DataFrame({'tier':['1' if i%3==0 else '2' for i in range(2000)],
                     'flag':[i%3==0 for i in range(2000)]})
    report=onceover.profile(df,detectors=['association_rules'])
    f=next(f for f in select(report,'conditional_rules',target='flag')
           if f['specification']['outcome']['value'] is True)
    assert f['specification']['conditions']==[{'column':'tier','operator':'eq','value':'1'}]
    assert metrics(f,contains='conditional_rate')['conditional_rate']==1


def test_duplicate_statement_records_fields_equality_and_missing_policy():
    report=onceover.profile(pl.DataFrame({'a':[None,None,None]}),budget='quick')
    f=select(report,'duplicates')[0]
    assert f['subject']=={'columns':['a'],'unit':'rows'}
    assert f['specification']['definition']['comparison']=='exact_equality'
    assert f['specification']['definition']['missing_values_equal'] is True
    assert metrics(f,contains='n_redundant')['n_redundant']==2


def test_gap_statement_records_the_reference():
    import datetime as dt
    days=[dt.date(2024,1,1)+dt.timedelta(days=i) for i in range(100)]
    days=[d for d in days if d.weekday()<5 and d!=dt.date(2024,2,7)]
    report=onceover.profile(pl.DataFrame({'date':days}),budget='quick')
    gap=select(report,'gaps')[0]
    definition=gap['specification']['definition']
    assert definition['domain']=='temporal'
    assert metrics(gap,contains="reference_source")["reference_source"]=="inferred"
    assert definition['reference']['calendar']=='weekdays'


def test_detector_selection_and_global_selection_limits():
    registry={d['id']:d for d in onceover.available_detectors()}
    assert registry['prism']['categories']==['conditional_rules']
    assert registry['prism']['requires']==['gbm']
    df=related_pair()
    all_rules=onceover.profile(df)
    limited=onceover.profile(df,max_rules=1)
    rules=select(limited,'conditional_rules')
    assert len(rules)<=1
    assert {f['id'] for f in rules} <= {f['id'] for f in all_rules.findings}
    assert not onceover.profile(df,detectors=[]).findings
    with pytest.raises(ValueError,match='unknown detector'):
        onceover.profile(df,detectors=['does_not_exist'])


def test_predictive_inputs_are_not_truncated_to_top_three():
    rng=random.Random(845)
    data={f'x{i}':[rng.uniform(-1,1) for _ in range(1800)] for i in range(4)}
    data['y']=[sum((i+1)*data[f'x{i}'][r] for i in range(4)) for r in range(1800)]
    report=onceover.profile(pl.DataFrame(data),detectors=['gbm'])
    f=select(report,'dependencies',subtype='predictive',target='y')[0]
    assert set(f['subject']['inputs'])=={'x0','x1','x2','x3'}
    m=metrics(f,contains='top_features')
    assert len(m['top_features'])<=3
    assert set(m['inputs'])==set(f['subject']['inputs'])


def test_opaque_columns_do_not_become_missingness_or_duplicate_claims():
    report=onceover.profile(pl.DataFrame({'opaque':[[1],[2],[3]]}),budget='quick')
    assert not select(report,'missingness')
    assert not select(report,'duplicates')
    assert report.coverage['row_duplicates']['status']=='not_applicable'


@pytest.mark.parametrize('cadence, removed, expected', [(7,{10},1),(3,{10,11},2)])
def test_gap_counts_exclude_observed_endpoints(cadence, removed, expected):
    import datetime as dt
    start=dt.date(2024,1,1)
    days=[start+dt.timedelta(days=cadence*i) for i in range(40) if i not in removed]
    report=onceover.profile(pl.DataFrame({'date':days}),budget='quick')
    gap=select(report,'gaps')[0]
    assert metrics(gap,contains='missing')['missing']==expected
    assert gap['specification']['definition']['reference']['cadence_days']==cadence
