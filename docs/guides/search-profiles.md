# Choose and use a search profile

Compile the profile needed for your session:

```powershell
wedl compile --profile state
wedl compile --profile fts
wedl compile --profile vector
wedl compile --profile hybrid
```

Use `state` for validation and temporal queries, `fts` for names and phrases,
`vector` for semantic recall, or `hybrid` for writing with both kinds of search.
Match search mode to the compiled profile:

```powershell
wedl search "archive custody" --mode fts
wedl search "archive custody" --mode vector
wedl search "archive custody" --mode hybrid
```

The default vector provider is local LSA. To use the optional Sentence
Transformers provider, install its extra, then compile with an explicit model:

```powershell
pip install 'wedl[sentence-transformers]'
wedl compile --profile hybrid --vector-provider sentence-transformers --vector-model sentence-transformers/all-MiniLM-L6-v2
```

For an OpenAI-compatible embedding service, set the endpoint and credentials in
the current shell and select the provider explicitly:

```powershell
$env:WEDL_EMBEDDING_ENDPOINT = 'http://127.0.0.1:8000/v1/embeddings'
$env:WEDL_EMBEDDING_API_KEY = '<your-service-key>'
wedl compile --profile vector --vector-provider openai-compatible --vector-model text-embedding-3-small
```

Keep credentials out of committed files. Model weights are obtained separately
from the WEDL package. Use `hash-test` only for tests and diagnostics. Measure
your actual world before choosing a profile based on historical reports.

Read the retrieval contract with
`adrai --repo . search 'Authorized independent search lanes' --mode fts --json`
in this development repository.
