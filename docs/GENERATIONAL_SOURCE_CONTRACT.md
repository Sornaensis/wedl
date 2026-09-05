# Generational source contract

`wedl/v0.7` generational facts are literal Markdown frontmatter records.  The
supported record kinds are `organization`, `parentage`, `union`,
`affiliation`, `legacy`, `tenure`, `claim`, and `vital-history`.

Each requires `generational-core-v1` in the canonical, world-only capabilities
array.  Its ID prefix and canonical generated directory are respectively:
`organization_`/`organizations`, `kinship_`/`kinships`, `union_`/`unions`,
`affiliation_`/`affiliations`, `legacy_`/`legacies`, `tenure_`/`tenures`,
`claim_`/`claims`, and `vital_`/`vitals`.

Records use an exact common envelope plus the kind's fields from ADR 0005.
`initialization` is one instant `*-initialize` transition and `transitions` is
an append-only list of closed transition payloads. Applicability is only
`static`, `instant`, or `inclusive-interval`; all timed values are exact
`{timeline,tick,order}` values. This source contract does not derive lineage,
inheritance, legitimacy, residence, or a successor.
