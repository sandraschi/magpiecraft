# PRD

The binding product requirements are maintained in the fleet documentation repository, not duplicated
here:

| Document | Path | What it covers |
|---|---|---|
| PRD v2 (MagpieCraft / MediaCraft) | `mcp-central-docs/projects/mediacraft/PRD.md` | Pillars A to F, principles, tiers, decisions D1 to D7 |
| PRD v1 (kept verbatim) | `mcp-central-docs/projects/mediacraft/PRD_v1_agy_2026-11-24.md` | The original knowledge-graph framing, now pillar E |
| Comics module scope | `mcp-central-docs/projects/comiccraft/PRD.md` | Scanner, reader and formats for comics and manga |
| Metadata research | `mcp-central-docs/projects/mediacraft/RESEARCH_METADATA.md` | Local dumps first, APIs second, provenance |
| Sources and legal lines | `mcp-central-docs/projects/mediacraft/RESEARCH.md` | Terms, legitimate sources |

This repository's own documents:

- [ARCHITECTURE.md](ARCHITECTURE.md) - how the code is organised.
- [READER_PARITY.md](READER_PARITY.md) - behaviour contract against the browser prototype.
- [BENCHMARKS.md](BENCHMARKS.md) - measured numbers.
- [TOOLS.md](TOOLS.md) - CLI and MCP reference.
- [ATTRIBUTION.md](ATTRIBUTION.md) - provider terms and tool licences.
- [DEVELOPMENT.md](DEVELOPMENT.md) - gates and conventions.

Section 14 of the PRD (decisions of 2026-11-24) is binding for this repository: the comics module,
shared adapters, no name clashes, OpenSubtitles opt-in, lyrics via sidecars, local subtitle generation,
and no DRM-tool integration of any kind.
