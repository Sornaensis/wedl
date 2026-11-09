+++
schema = "adrai/decision/v1"
adr = "A01M70QDCTP5B05R3VEJD9QE70R"
record = "R01M70QDD10FZ6500HADMBT3ZXZ"
title = "Incrementally publish SQLite snapshots after Git commits"
summary = "Reuse verified unchanged inputs and atomically publish a full commit-keyed generation after each WEDL-produced Git commit."
domains = ["world.projection.publication"]
+++

Key immutable SQLite snapshots by the full original Git commit ID and the compatibility identity for schema, parsing, validation and projection. Use a verified compatible cached ancestor or reusable source inputs to avoid reparsing unchanged records. Keep path-sensitive identity correct for moves, additions and deletions. Rebuild from authoritative source when reuse cannot be proved.

A WEDL command that creates a Git commit builds and attempts to publish the next database after the commit lands. Build away from published generations and expose only a complete, validated artifact atomically. If the build fails, report Git commit success and database publication failure separately, keep previous coherent generations available under their own identities, and allow safe retry for the new commit. Git history remains authoritative.

A live worktree has a separate fenced source identity that accounts for uncommitted changes even while HEAD is unchanged. A separate builder may reconcile and publish disposable live cache generations on demand. Prove freshness using reliable signals and bounded source verification where needed, or refuse the current read when freshness cannot be established.

<!-- @adrai:eyJhIjp7ImkiOiJjb2RleC1kZXZlbG9wZXIiLCJrIjoibGxtIiwibSI6ImdwdC02LjEtc29sIn0sImIiOiJkNWYxYjEzYTEyOWYzOGY1YjhkMjczMjg1ZDY3MjM0ODRlZTFmYmUzIiwiaSI6InNoYTI1NjpfSFhRaVhIcEgwNzkwZzhjSFZLb3FQcXZvQ2RuRFVjM1JmeWNrODQwNmFjIiwiayI6ImRlY2lzaW9uLmNyZWF0ZSIsIm8iOiJSMDFNNzBRREQxMEZaNjUwMEhBRE1CVDNaWFoiLCJvcCI6Ik8wMU03MFFERDEwRlo2NTAwSEFETUJUM1pYWiIsInIiOiJtYXN0ZXIiLCJzIjoic2hhMjU2Om93bVZmdUpuWGoxMlpyaW5tTmhUcU95ak1RY1J3RGh6bUV5bUZ3WlZ1QU0iLCJ0IjoxNzk0MjQ3MTQ0NDgwLCJ2IjoxLCJ4IjoiYWRyYWkvMS4wLjAifQ -->
