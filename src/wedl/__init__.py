"""wedl: versioned interactive-story state and conversation provenance."""

__version__ = "0.6.0"
SOURCE_SCHEMA = "wedl/v0.3"
THREAD_SOURCE_SCHEMA = "wedl/v0.5"
SUPPORTED_SOURCE_SCHEMAS = frozenset((SOURCE_SCHEMA, THREAD_SOURCE_SCHEMA))
V04_SOURCE_SCHEMA = "wedl/v0." + "4"
V04_RECOVERY_CONTRACT = "docs/THREAD_SCHEMA_CONTRACT.md#4-quarantined-v04-recovery"
SQLITE_SCHEMA = "wedl-sqlite/v5"
PROTOCOL_VERSION = "wedl-command/v2"
