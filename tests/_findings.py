"""Queries against the public statement/evidence schema."""


def default_plus(*extra: str) -> list[str]:
    """The default detector set plus explicitly requested detectors.

    For tests whose subject is an opt-in detector's interaction with
    the default passes (the wide-form recursion, the long-format FD
    demotion): the detector under test is named, the rest of the
    default set comes from the registry so the list tracks it.
    """
    import onceover

    ids = [
        d["id"]
        for d in onceover.available_detectors()
        if d["activation"] in ("default", "hint_gated")
    ]
    return ids + [e for e in extra if e not in ids]


def select(report, category, *, subtype=None, form=None, target=None, inputs=None, column=None):
    return [f for f in report.findings
            if f['category'] == category
            and (subtype is None or f.get('type') == subtype)
            and (form is None or f['specification'].get('form') == form)
            and (target is None or f['subject'].get('target') == target)
            and (inputs is None or set(f['subject'].get('inputs', [])) == set(inputs))
            and (column is None or column in f['subject']['columns'])]


def metrics(finding, *, contains=None, validation=None, detector=None):
    evidence = [e for e in finding['evidence']
                if (contains is None or contains in e['metrics'])
                and (validation is None or e['validation']['method'] == validation)
                and (detector is None or e['method']['detector'] == detector)]
    if not evidence:
        raise AssertionError(f'No matching evidence in {finding}')
    evidence.sort(key=lambda e: (e['method']['detector'] != 'rule_evaluator', e['method']['detector']))
    return evidence[0]['metrics']
