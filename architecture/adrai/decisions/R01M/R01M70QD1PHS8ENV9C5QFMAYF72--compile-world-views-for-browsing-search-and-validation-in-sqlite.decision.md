+++
schema = "adrai/decision/v1"
adr = "A01M70QD1JRSAVPHVEQC7Y77111"
record = "R01M70QD1PHS8ENV9C5QFMAYF72"
title = "Compile world views for browsing search and validation in SQLite"
summary = "Use versioned, disposable SQLite databases as compiled views for reads, search and validation."
domains = ["world.projection.sqlite"]
+++

Compile the Markdown source manifest and typed records into versioned SQLite views with the metadata, indexes and validation results needed for browsing, search and validation. Keep exact source identity and revision provenance with each generation. SQLite can store and serve validation results and support validation queries while canonical source and native semantic rules define their meaning. Retain equivalent diagnostic and admission behavior across clean builds, incremental builds and reads.

Build and verify a complete generation before atomic publication. A request pins a single read-only generation; search, facets, counts and displayed records come from that same state and access context. Refuse incomplete or untrusted artifacts and rebuild from verified source when needed. Separate disposable cache retention and cleanup from native authoring and recovery state.

<!-- @adrai:eyJhIjp7ImkiOiJjb2RleC1kZXZlbG9wZXIiLCJrIjoibGxtIiwibSI6ImdwdC02LjEtc29sIn0sImIiOiJlNzdhYzBhOTg2NzMwNDc5MWQyNjhiYTUzNmQxN2I5MWFkZGE1MjUyIiwiaSI6InNoYTI1Njp4bkFIMlRYRWZCcmRzaVpGQ05hQ3lBRzRyM3FxMnBxWHFZeE9kNEI2NXRzIiwiayI6ImRlY2lzaW9uLmNyZWF0ZSIsIm8iOiJSMDFNNzBRRDFQSFM4RU5WOUM1UUZNQVlGNzIiLCJvcCI6Ik8wMU03MFFEMVBIUzhFTlY5QzVRRk1BWUY3MiIsInIiOiJtYXN0ZXIiLCJzIjoic2hhMjU2OlQ5UjVJU05HajFXc0lGbWVrYmVqUzlwUUc5VnF0X2VBMGxpTFhrbVpoSkEiLCJ0IjoxNzk0MjQ3MTMyODgxLCJ2IjoxLCJ4IjoiYWRyYWkvMS4wLjAifQ -->
