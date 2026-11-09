+++
schema = "adrai/decision/v1"
adr = "A01M70QB8Y7GJSP13QSJFXKXY2E"
record = "R01M70QB923BA8WW0J7DB1QVZKY"
title = "Keep Markdown and Git authoritative for world state"
summary = "Store current world state in Markdown with YAML frontmatter and use original Git commits for history and provenance."
domains = ["world.source"]
+++

The world worktree represents current authored state in Markdown records with YAML frontmatter. Original Git commits, trees and blobs preserve immutable historical states and their provenance. Read historical source from the exact original commit, keeping source paths, fingerprints and blob citations stable.

Canonical authoring changes publish Markdown through the native writer and its validation and recovery rules. Compiled views and search artifacts are disposable and rebuildable from authoritative inputs. A missing, incompatible or damaged artifact does not change world state. Build and use derived artifacts without writing their contents back as authored records.

<!-- @adrai:eyJhIjp7ImkiOiJjb2RleC1kZXZlbG9wZXIiLCJrIjoibGxtIiwibSI6ImdwdC02LjEtc29sIn0sImIiOiIwNzAxNWI1NjdkY2Q3NjVkZGFkNDNmZDI5YjdlYTNlNmU2ZjRiYTUyIiwiaSI6InNoYTI1NjpHZXY5YlA1WVhSNEpITEJma3NMMUx3OFVya3BRa05qYXRIejFDUkZMOV9jIiwiayI6ImRlY2lzaW9uLmNyZWF0ZSIsIm8iOiJSMDFNNzBRQjkyM0JBOFdXMEo3REIxUVZaS1kiLCJvcCI6Ik8wMU03MFFCOTIzQkE4V1cwSjdEQjFRVlpLWSIsInIiOiJtYXN0ZXIiLCJzIjoic2hhMjU2OlFPWHliOW5IQ3g2NzJmeVFUTUxPTTJwNDIxT3lCV0RXdGhjaFNkTW51MzAiLCJ0IjoxNzk0MjQ3MDc0ODgzLCJ2IjoxLCJ4IjoiYWRyYWkvMS4wLjAifQ -->
