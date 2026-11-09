+++
schema = "adrai/decision/v1"
adr = "A01M70QDRQ54XM77TRT2MJJNRH6"
record = "R01M70QDS2K4Z3WSTRVAGKVE31A"
title = "Serve the world through read-only searchable cards"
summary = "Present world records in a read-only card browser with character, tick and authored calendar filters."
domains = ["world.browser"]
+++

The web serve command exposes read-only browsing. HTTP endpoints and browser controls provide no authoring mutations to Markdown, private authoring state or Git. Requests query a single pinned read-only SQLite generation. A separate builder may publish disposable cache artifacts so a running server can reflect completed CLI edits without committing or restarting.

Present all supported browsable world record kinds in searchable, scannable cards, including characters, locations, items, lore and typed spatial relations between locations. Keep filtering and navigation legible. Support explicit character point of view, story tick and authored calendar time through canonical temporal resolution and existing visibility rules, including honest ambiguity and unsupported cases.

Apply admission before deriving cards, matches, counts and filter facets. Records outside the selected visibility context cannot leak through search or navigation. Preserve coherent provenance and source identity across the entire response.

<!-- @adrai:eyJhIjp7ImkiOiJjb2RleC1kZXZlbG9wZXIiLCJrIjoibGxtIiwibSI6ImdwdC02LjEtc29sIn0sImIiOiIzMjY5YzA2NGI4MDZmMjUxYmFlN2QxYTdhY2YyNDE0NzgxYzdlMzJlIiwiaSI6InNoYTI1NjpxM1QxX2hacDlLUG1wZVhYb3dWMThQbDF4aEVsUXhjYjFaRkstaXJ0VlRRIiwiayI6ImRlY2lzaW9uLmNyZWF0ZSIsIm8iOiJSMDFNNzBRRFMySzRaM1dTVFJWQUdLVkUzMUEiLCJvcCI6Ik8wMU03MFFEUzJLNFozV1NUUlZBR0tWRTMxQSIsInIiOiJtYXN0ZXIiLCJzIjoic2hhMjU2OmFvczJ3XzU5aDhheWpGQW9QQUxjSlMwZ3pONEh6STN4MUc2RUQ5RHpGdWMiLCJ0IjoxNzk0MjQ3MTU2ODE5LCJ2IjoxLCJ4IjoiYWRyYWkvMS4wLjAifQ -->
