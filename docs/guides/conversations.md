# Inspect and extend conversations

List conversation records, then use a returned ID or title:

```powershell
wedl entity list --kind conversation
wedl conversation show CONVERSATION --perspective author
wedl conversation show CONVERSATION --perspective character --character CHARACTER
wedl conversation show CONVERSATION --perspective author --all-time
```

Replace uppercase placeholders with a conversation or character from your world.
Use `--tick`, `--timeline`, and `--order` for a particular fictional moment. Use
`--all-time` for a complete author transcript; do not combine it with those time
arguments.

For an active scene, preview creating a conversation or appending a beat:

```powershell
wedl author conversation create "Archive exchange" --scene SCENE
wedl author conversation append CONVERSATION "The shutters are closing." --speaker CHARACTER
```

Inspect the preview and repeat the same request with `--confirm TOKEN` to apply
it. Run `wedl author conversation append --help` for action-beat arguments.
Historical or closed conversation edits use the ordinary changeset workflow.
The [follow-up changeset example](../../examples/choice-of-records-followup.json)
shows a multi-record interaction; use IDs and expected HEAD from your own world.

Read the transcript/recollection contract with
`adrai --repo . search 'Conversation transcript' --mode fts --json` in this
development repository.
