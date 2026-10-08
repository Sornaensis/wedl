# Working with generational history

Run commands in the WEDL world repository, or pass its path with `--repo PATH`. Begin with a world using v0.7 and `generational-core-v1`. Inspect the closed authoring catalogue before choosing fields:

```sh
wedl generational schema --repo PATH
wedl generational scaffold --repo PATH --output intent.json
```

The scaffold uses the current HEAD. Edit its explicit organization choices or replace it with another supported intent. Retain its current `expectedHead`, choose a unique `idempotencyKey`, and use real names or stable IDs from that exact world. The following parentage intent is illustrative; replace the placeholder HEAD and names before preview:

```json
{
  "action": "generational.create",
  "expectedHead": "0123456789abcdef0123456789abcdef01234567",
  "idempotencyKey": "adoptive-parent-1",
  "kind": "parentage",
  "title": "Mara adopts Ilyra",
  "tags": ["adoptive"],
  "audience": ["public"],
  "perspectives": ["ordinary"],
  "fields": {"child_id": "Ilyra Sorn", "parent_id": "Mara Vale"},
  "payload": {"basis": "adoptive"},
  "at": {"timeline": "main", "tick": "-7", "order": "2"}
}
```

Preview the edited file, inspect diagnostics and the complete proposed changes, then use the returned confirmation token with the identical file:

```sh
wedl author request preview intent.json --repo PATH
wedl author request apply intent.json --repo PATH --confirm TOKEN
```

Apply only a valid preview. After editing the file or changing the world HEAD, preview again. Keep the same key and file for a retry. Generational requests require the confirmation token; `--yes` cannot replace it. For HTTP use `POST /api/authoring/preview` with `X-Wedl-Token`, then `POST /api/authoring/apply` with that token and `X-Wedl-Confirmation: TOKEN`.

Use the catalogue to choose `generational.create`, `generational.append`, `generational.correct`, or `generational.batch`. For a batch that links newly created records, choose their stable IDs explicitly before preview. Enter intended parentage, membership, tenure and claims as explicit choices rather than relying on a derived successor or target holder.

## Read selected history

Read commands accept raw `wedl-generational/v1` request JSON files, or `-` for stdin. Use the selected world's exact revision and declared capabilities, timeline and mode. Enter StoryTime ticks and orders as signed decimal strings. For example, after constructing a parents request for the chosen subject:

```sh
wedl generational parents request.json --repo PATH
wedl generational parents request.json --repo PATH --viewpoint CHARACTER
wedl generational discover request.json --repo PATH
wedl generational labels request.json --repo PATH
wedl generational context request.json --repo PATH
```

The second command selects a local-author character POV for that request. Put the character-mode exact time in the request JSON and select the viewpoint through the command option. Use that character's learned labels for names; inspect closed `unknown` outcomes before building on a read. Choose the matching operation and fields in each request rather than reusing a parents body for another leaf. HTTP offers corresponding protected routes and `GET /api/generational/bootstrap` for revision, capability and timeline discovery.

## Author explicit learned assertions

Use the same preview and confirmed apply commands for the four existing knowledge actions. Inspect their closed payloads in the authoring catalogue:

- `generational.knowledge.opt-in` enables the explicit optional knowledge capability on an existing v0.7 world.
- `generational.knowledge.create` records a knower, literal typed assertion, its separate applicability interval, learning time and affirmative state. Enter learned labels and exact admitted evidence when known.
- `generational.knowledge.state` appends a state transition to an existing assertion.
- `generational.knowledge.replace` retires an old assertion and creates its replacement for the same knower in one previewed request.

Do not use a canonical title as a substitute for a character's learned label. A mistaken belief can be authored literally; it does not edit canonical parentage, tenure or other facts. Retain the old assertion when correcting it so earlier POV reads retain their authored history.

## Packaged examples

Create a fresh named world with an explicit example when desired:

```sh
wedl init NAME --example ash-archive
wedl init NAME --example frontiersmen
```

The -v07 names select the same Ash Archive and Frontiersmen examples; the default is Ash Archive. Those two v0.7 packages preserve their authored stories and add no canonical generational fact records. [Tideglass](../stories/tideglass/queries.md) (`--example tideglass`) includes explicit parentage, a union, organization affiliations, a keeper tenure and disputed claim, and known vital history. Its recipes use the world's complete capability declaration and explicit horizons. Author additional records explicitly after inspecting your world. Legacy migration defaults do not opt into typed knowledge. An empty initialization uses v0.3, so choose the appropriate existing migration procedure before authoring v0.7 records.

## Durable contracts

In the WEDL development/source checkout, use ADRAI to read source, projection, query/viewpoint/context, validation and confirmed authoring contracts. The historical benchmark and its raw result are retained in [the report](../reports/generational-benchmark.md); its missed latency target remains open.

`adrai --repo WEDL_SOURCE_CHECKOUT show A01M491X2B6KF0PZFZ4HJMBFSSR --json`
`adrai --repo WEDL_SOURCE_CHECKOUT show A01M491Y1RVN98VZDF318XJ1ARW --json`
`adrai --repo WEDL_SOURCE_CHECKOUT show A01M491YZG40BC65JPVT4JMYR7D --json`
`adrai --repo WEDL_SOURCE_CHECKOUT show A01M491ZCQ1E4HTMCF2NHN87323 --json`
`adrai --repo WEDL_SOURCE_CHECKOUT show A01M491ZXP5DQ61KMG2YC66G8MM --json`
