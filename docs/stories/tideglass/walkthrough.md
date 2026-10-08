# A short Tideglass walkthrough

Install WEDL using the [source README](../../../README.md#install), then use a
fresh working directory for this PowerShell session. The commands create and
read your own copy; the packaged story remains unchanged. Continue with the
[query recipes](queries.md) in the same session.

## Start and validate

```powershell
$repo = 'tideglass-demo'
$created = wedl --compact init $repo --example tideglass --profile fts | ConvertFrom-Json
$validation = wedl --compact validate --repo $repo | ConvertFrom-Json
$status = wedl --compact status --repo $repo | ConvertFrom-Json
```

Initialization compiles 64 records. Validation has no diagnostics, the cache is
ready, and there is no active scene. The [world guide](world-guide.md) explains
the cast and the moments used below.

## 1. Follow a cause into state

```powershell
$lens10 = wedl --compact state obj_4C9HNZYKJG5QQ6DWTWDJ4NWJ8D --repo $repo --timeline main --tick 10 --order 0 --require-compiled | ConvertFrom-Json
$lens20 = wedl --compact state obj_4C9HNZYKJG5QQ6DWTWDJ4NWJ8D --repo $repo --timeline main --tick 20 --order 0 --require-compiled | ConvertFrom-Json
$lens30 = wedl --compact state obj_4C9HNZYKJG5QQ6DWTWDJ4NWJ8D --repo $repo --timeline main --tick 30 --order 0 --require-compiled | ConvertFrom-Json
$lens40 = wedl --compact state obj_4C9HNZYKJG5QQ6DWTWDJ4NWJ8D --repo $repo --timeline main --tick 40 --order 0 --require-compiled | ConvertFrom-Json
```

The lens is cracked at the Beacon at `10:0`, cracked at the Workshop at `20:0`,
repaired there at `30:0`, and repaired at the Beacon at `40:0`. Inspect the
citations: Storm Damage, Workshop Arrival, Lens Repaired and Beacon Lit account
for those changes.

## 2. Compare knowledge within one tick

```powershell
$jessaBefore = wedl --compact knowledge char_6D246WN3J75HBRJJXGQ5H6QAD4 --repo $repo --timeline main --tick 20 --order 1 --require-compiled | ConvertFrom-Json
$jessaAfter = wedl --compact knowledge char_6D246WN3J75HBRJJXGQ5H6QAD4 --repo $repo --timeline main --tick 20 --order 2 --require-compiled | ConvertFrom-Json
$orinAfter = wedl --compact knowledge char_62PCSFAX4YQE48JK476XKGG6B8 --repo $repo --timeline main --tick 20 --order 2 --require-compiled | ConvertFrom-Json
```

`know_0WYS8PMTPS0KNEX3SCQZ30E6JF` appears only in Jessa's second result.
Neither earlier Jessa nor Orin receives that private discrepancy.

## 3. Select concurrent contexts explicitly

```powershell
$minaContext = wedl --compact context char_4JVNDN6QW0BAFFV379AJJSMEPZ --repo $repo --scene scene_0QXK1AT5DC93TB0YZSB1KAEQVC --perspective character --timeline main --tick 20 --order 0 --query 'checked work' --mode fts --max-characters 8000 --require-compiled | ConvertFrom-Json
$orinContext = wedl --compact context char_62PCSFAX4YQE48JK476XKGG6B8 --repo $repo --scene scene_0M3G43J3775Q54BHQJH4KRWQCX --perspective character --timeline main --tick 20 --order 0 --query 'Tideglass log discrepancy' --mode fts --max-characters 8000 --require-compiled | ConvertFrom-Json
```

At the same global moment `20:0`, Mina's present moment names the Workshop and Tavi. Orin's names the Quay and
Jessa, without the private discrepancy. Inspect `promptText` to see what was retrieved.
The request metadata also echoes the query you supplied. Advancing Orin's
context past his explicit Quay presence is an expected refusal:

```powershell
wedl --compact context char_62PCSFAX4YQE48JK476XKGG6B8 --repo $repo --scene scene_0M3G43J3775Q54BHQJH4KRWQCX --perspective character --timeline main --tick 20 --order 2 --mode fts --require-compiled
```

This last command exits with code 2 and reports that Orin is not present.

## 4. Read beats and later recollections

```powershell
$quayFirst = wedl --compact conversation show conv_3G4YJGZW7NHJ2NB5PESMV0A6S5 --repo $repo --perspective author --timeline main --tick 20 --order 0 --require-compiled | ConvertFrom-Json
$quaySecond = wedl --compact conversation show conv_3G4YJGZW7NHJ2NB5PESMV0A6S5 --repo $repo --perspective character --character char_6D246WN3J75HBRJJXGQ5H6QAD4 --timeline main --tick 20 --order 1 --require-compiled | ConvertFrom-Json
$workshopLater = wedl --compact conversation show conv_4YFR36RGTKN7VE0R5XQFBKH6QT --repo $repo --perspective author --timeline main --tick 30 --order 0 --require-compiled | ConvertFrom-Json
$minaRecall = wedl --compact conversation show conv_4YFR36RGTKN7VE0R5XQFBKH6QT --repo $repo --perspective character --character char_4JVNDN6QW0BAFFV379AJJSMEPZ --timeline main --tick 30 --order 0 --require-compiled | ConvertFrom-Json
```

The Quay results contain one then two beats. The later workshop transcript
contains three beats, including Tavi's action at `20:2`, and one recollection.
Mina's subjective recollection at `30:0` includes one exact turn.

## 5–7. Read routes, family and a confirmed edit

Continue with [queries](queries.md) for complete JSON-building commands. They
read the two directed foot routes, compare the flood boundary, inspect explicit
parents, organization roles and keeper facts, and rename the survey staff on
this disposable copy through preview, confirmation and replay.
