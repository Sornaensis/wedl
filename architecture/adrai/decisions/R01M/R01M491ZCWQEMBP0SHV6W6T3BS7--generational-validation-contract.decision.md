+++
schema = "adrai/decision/v1"
adr = "A01M491ZCQ1E4HTMCF2NHN87323"
record = "R01M491ZCWQEMBP0SHV6W6T3BS7"
title = "Generational validation contract"
summary = "Preserve the existing validation contract with current v0.7 behavior and original approval provenance."
domains = ["generational", "validation"]
+++

# Generational validation contract

This record migrates the existing `docs/GENERATIONAL_VALIDATION.md` contract into ADRAI. The original ADR 0005 (`A01M48NQZ50KH9V094Q5A3138R0`) remains the authority for its 2026-08-30 approved proposal and historical five-token decision vectors. Its existing 2026-10-05 typed-knowledge extension remains separately identified; this migration does not invent a new approval or rewrite the original approval stage. Current implementation statements below distinguish that stage from the implemented v0.7 component.

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

Typed knowledge validation also checks the optional final capability and its core prerequisite, the seven closed literal assertion payloads, exact applicability independently from first affirmative learning, endpoint kinds, bounded learned labels and exact same-knower evidence at learning time. It permits authored mistaken, conflicting and cyclic beliefs without validating them against canonical relationships. Original canonical parentage and organization cycle checks remain in force for canonical facts. The original accepted-contract-only five-token YAML oracle remains unchanged; existing current knowledge tests cover the extension separately.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiMGY5NzBmYmI2NDNmZjBjODM3ZGY3YTNlYTQ4ZTk2OThlNjE1MTFiZSIsImkiOiJzaGEyNTY6MDFpeEZWcnp3RkUyVzdNTFRKMjFVZEhXNzVxUVV0UW1PV2tKdUxFLXNBWSIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ5MVpDV1FFTUJQMFNIVjZXNlQzQlM3Iiwib3AiOiJPMDFNNDkxWkNXUUVNQlAwU0hWNlc2VDNCUzciLCJyIjoibWFzdGVyIiwicyI6InNoYTI1NjpQYmJUcGphbjhpMVlBQlYxNjlnc2JuYUhnTWNINFhwMFNLYXlGYzJnV3AwIiwidCI6MTc5MTMwNTQyOTkxMSwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
