# Compendium links

Open `/` or `/compendium` on the local WEDL server for a fresh Author directory.
Directory, story graph and record links can be opened directly, including in a
new tab. Each record link retains its Author or Character context and is checked
again against the source on every open. Author views link to the Workbench at
`/workbench`, and the diagnostic browser is available at `/legacy-author`.

By default, links work for the lifetime of the server. To keep links across
restarts and cache rebuilds, provision a key ring outside the world and pass
its absolute path when starting the server.

```sh
wedl-rs --repo tideglass-world serve --compendium-keyring /absolute/private/wedl-links.json
```

Use a JSON object with these three fields.

| Field | Value |
| --- | --- |
| `schema` | `wedl-compendium-keyring/v1` |
| `installation` | 16 random bytes as 32 lowercase hexadecimal digits |
| `keys` | An array of one to eight distinct keys, each 32 random bytes as 64 lowercase hexadecimal digits |

Generate the installation identifier and each key separately with an OS
cryptographic random generator. Store the file in a directory controlled by the
account that runs WEDL. On Unix, set the file mode to `0600`. On Windows, use a
private directory and file ACL limited to the account that runs WEDL and its
administrators. Use a regular file and a path with no symbolic links or
junctions.

Keep the installation identifier stable when rotating keys. Put the new key
first and keep any older keys needed to open issued links. Restart WEDL to load
the updated file. Retiring a key expires the links it issued. A saved link is
bound to its source and server mode. A fixed historical link also requires the
original revision. If a named moment moves or a saved selection loses
admission, the link opens a generic recovery page.
