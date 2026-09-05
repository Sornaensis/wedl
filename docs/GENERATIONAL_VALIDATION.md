# Generational validation

Generational validation runs as part of homogeneous `wedl/v0.7` source
validation, alongside inherited chronology/thread checks and the independent
spatial component validator.

It reports deterministic field-addressed diagnostics for closed envelopes and
payloads, capability gating, IDs, references, applicability and transition
ordering, causes and replacements, parentage and organization cycles, union
cardinality, reciprocal claims, and authored vital-history bounds. Invalid
source is reported rather than repaired, sorted, inferred, or converted.

The typed accessors in `wedl.generational` are immutable literal views and use
the same closed leaf shapes for applicability and transitions. They are not
query or fold APIs.
