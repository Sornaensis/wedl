# WEDL

## Install

WEDL requires Python 3.11 or newer and Git 2.x. Run these commands from the
source checkout.

On Linux or macOS:

```bash
python -m venv .venv
source .venv/bin/activate
python -m pip install -e .
wedl --help
```

On Windows PowerShell:

```powershell
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install -e .
wedl --help
```

The browser interface is included in the Python package and needs no separate
build. If PowerShell blocks activation, use the virtual environment's
interpreter directly; the [manual](docs/guides/command-line.md#running-from-a-source-checkout)
shows how.

## Basic ideas

WEDL keeps track of a story's characters, places, objects and events. A world
is a folder of Markdown records, with Git keeping their history. You can work
through the command line or a local browser interface. SQLite makes the records
searchable.

Scenes bring characters together at a particular moment. Events record what
changes as the story moves forward. You can look up a character's state at an
earlier point without losing what happened later. Story time orders those
changes; calendar dates are recorded separately.

What the author knows and what a character knows are different views of the
same story. A character's view includes what they could know at the chosen
moment. Conversations preserve the words spoken, while recollections record
how each character remembers them. Those memories can disagree.

To try it, create a copy of the Frontiersmen story, look up Rhea at tick 210,
then open the story in your browser:

```bash
wedl init my-world --example frontiersmen-v07
wedl state Rhea --repo my-world --tick 210
wedl serve --repo my-world --open
```

The [manual](docs/README.md) covers creating a world, using the packaged stories,
reading and editing records, search and the browser interface.
