# Build a writing brief

From a WEDL world repository, select a character and optionally a scene. For the
packaged Ash Archive, a focused request looks like:

```powershell
wedl context "Mara Vale" --query "custody of the archive" --max-characters 5000
wedl context "Mara Vale" --perspective author --max-characters 5000
wedl context "Mara Vale" --perspective dramatic-irony --max-characters 5000
```

Use `--repo PATH` when running elsewhere and `--scene SCENE` when the world has
multiple active scenes. Use the returned Markdown packet as the writing brief;
inspect its provenance when following up on a detail. In dramatic-irony output,
hand `characterPrompt` to the character writer and keep `authorMargin` available
to the author separately.

Choose a smaller `--max-items` when the packet feels crowded. Use `--mode fts`,
`vector`, or `hybrid` to match the profile compiled for the repository. For
historical work, pass `--tick`, with `--timeline` and `--order` as needed.
Run `wedl context --help` for the current argument bounds.

In the historical Ash Archive writing exercises, shorter packets helped Mara's
writer focus on one current exchange, one relevant memory, and one present
relationship. Sister Ansel's packet was smaller because she entered the active
conversation later and had fewer accumulated Archive beliefs.

The selection and perspective contract is read through ADRAI; use
`adrai --repo . search 'Context packet selection' --mode fts --json` in this
development repository.
