# Run and inspect the configured tests

From the WEDL source checkout on Windows, run the functional runner:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tools/test.ps1
```

It uses local `.venv` Python when available. The default execution deadline is
800 seconds; `-TimeoutSeconds` accepts 1–800 for diagnosis. Deadline expiry
exits 124. Termination and diagnostic cleanup may extend total wall time.
Read the reported live collection, shard A–E and executed counts rather than
assuming an old suite total. The configured exact inventory is in `pytest.ini`.

For focused diagnosis or roster discovery:

```powershell
.venv\Scripts\python.exe -m pytest -q tests/test_cli_completion.py
.venv\Scripts\python.exe -m pytest --collect-only -q --wedl-suite=all
.venv\Scripts\python.exe -m pytest --collect-only -q --wedl-suite=normal
.venv\Scripts\python.exe -m pytest --collect-only -q --wedl-suite=performance
```

Use `--wedl-strict` when applying the configured outcome gate. A focused pytest
pass is diagnostic evidence; it does not establish full-runner deadline or
complete release acceptance. When changing test IDs or selection, consult the
registry checks in `tests/conftest.py` and the configured verification decision.

After functional acceptance, the configured performance aggregate is:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tools/test-performance.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tools/test-performance.ps1 -ValidateOnly
```

Its hard default deadline is 1800 seconds and operating target is 1500 seconds.
`-TimeoutSeconds` accepts 1–1800. `-ValidateOnly` checks enrollment and runtime,
browser/package/input preflight; it does not execute benchmarks. Read each
reported stage result and hash. Individual stages do not replace the aggregate.

## Temporary and retained outputs

The functional runner removes successful temporary artifacts and prints retained
failure paths. Current failure retention is indefinite. The performance runner
currently retains scratch and evidence on success and ValidateOnly too; it has
no removal path. Record those limitations when reporting results. Keep existing
release, benchmark and failure receipts under their established custody; do not
infer deletion authority from age or a later passing diagnostic.

For ordinary local diagnosis use one explicit contained task-owned temporary
directory with a named owner and finite lifetime. Clean only verified owned
scratch after it is no longer needed; preserve preexisting outputs and worktrees.
Do not claim descendants were stopped if runner cleanup reports access or
process-identity ambiguity. Preserve the specific unresolved failure until its
diagnosis is resolved.

Read gate details and retention limits in the WEDL development/source checkout:
`adrai --repo WEDL_SOURCE_CHECKOUT show A01M494QCT41GF34E92FKKHQ8AS --json`.
