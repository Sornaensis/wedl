"""wedl: versioned interactive-story state and conversation provenance."""

__version__ = "0.6.0"
SOURCE_SCHEMA = "wedl/v0.3"
THREAD_SOURCE_SCHEMA = "wedl/v0.5"
CHRONOLOGY_SOURCE_SCHEMA = "wedl/v0.6"
# Preserve the established generic parser/cache contract. The v0.6 validator
# dispatches before generic handling and the compiler uses the explicit set.
SUPPORTED_SOURCE_SCHEMAS = frozenset((SOURCE_SCHEMA, THREAD_SOURCE_SCHEMA))
COMPILED_SOURCE_SCHEMAS = frozenset((*SUPPORTED_SOURCE_SCHEMAS, CHRONOLOGY_SOURCE_SCHEMA))
V04_SOURCE_SCHEMA = "wedl/v0." + "4"
V04_RECOVERY_CONTRACT = "docs/THREAD_SCHEMA_CONTRACT.md#4-quarantined-v04-recovery"
# v0.7 is the coordinated generational/spatial source envelope.  Its optional
# authored capabilities are validated from the world record, rather than being
# guessed from the presence of individual records.
V07_SOURCE_SCHEMA = "wedl/v0.7"
SUPPORTED_SOURCE_SCHEMAS = frozenset((*SUPPORTED_SOURCE_SCHEMAS, V07_SOURCE_SCHEMA))
COMPILED_SOURCE_SCHEMAS = frozenset((*COMPILED_SOURCE_SCHEMAS, V07_SOURCE_SCHEMA))
SQLITE_SCHEMA = "wedl-sqlite/v14"
PROTOCOL_VERSION = "wedl-command/v2"
