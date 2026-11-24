# Install MagpieCraft

Two binaries, no runtime dependencies. Windows, Linux and macOS build from the same workspace; the
desktop app renders through `wgpu`, so it needs a working GPU driver like any other native app.

## 1. Build

```powershell
rustup update stable          # 1.82 or newer
git clone https://github.com/sandraschi/magpiecraft
cd magpiecraft
cargo build --release --workspace
```

Outputs:

| Binary | What it is |
|---|---|
| `target\release\magpiecraft.exe` | Desktop app (library grid and reader) |
| `target\release\magpiecraft-mcp.exe` | CLI with `--json`, plus the MCP stdio server |

`just ci` runs the three gates (`fmt-check`, `check`, `test`) if you have `just` installed.

## 2. Add your libraries

A library is a folder. MagpieCraft never writes into it.

```powershell
# name, root, kind (comics, manga, magazines, video, audio, ebook, photo, other)
.\target\release\magpiecraft-mcp.exe library-add Comics "L:\Multimedia Files\Comics" --kind comics
.\target\release\magpiecraft-mcp.exe library-add Manga  "L:\Multimedia Files\Manga"  --kind manga

# mature libraries are hidden everywhere until you allow them
.\target\release\magpiecraft-mcp.exe library-add "Comics - Porn" "L:\Multimedia Files\Comics - Porn" --kind comics --mature
```

Then index:

```powershell
.\target\release\magpiecraft-mcp.exe scan            # all libraries, incremental
.\target\release\magpiecraft-mcp.exe scan Comics     # one library
.\target\release\magpiecraft-mcp.exe scan --full     # re-read every item
.\target\release\magpiecraft-mcp.exe stats
```

A scan reads folder names, file sizes and modification times. Archives it has not seen before are
opened once for their page count and ComicInfo.xml. A second scan of an unchanged collection only
walks the tree.

## 3. Use the app

```powershell
.\target\release\magpiecraft.exe
```

First run shows an empty grid: `File > Add library...`, then `File > Scan all libraries`.

Keys: arrows and space page, `H` half page, `T` two pages, `R` right to left, `Home`/`End` first and
last page, `Escape` back, `/` search, `Ctrl` + mouse wheel zoom.

## 4. Register the MCP server

Claude Desktop (`%APPDATA%\Claude\claude_desktop_config.json`) or any stdio MCP client:

```json
{
  "mcpServers": {
    "magpiecraft": {
      "command": "D:/Dev/repos/magpiecraft/target/release/magpiecraft-mcp.exe",
      "args": ["mcp"]
    }
  }
}
```

No arguments beyond `mcp`, no ports, no tokens. Restart the host. The server logs to stderr only.

## 5. Optional external tools

MagpieCraft works without any of them; the features that need one say `not_configured`.

| Tool | Used for | Discovery |
|---|---|---|
| `ffprobe` | Codec, resolution and stream details | `MAGPIECRAFT_FFPROBE`, then `PATH` |
| `7z` | CBR/RAR extraction (Tier 2) | `MAGPIECRAFT_7Z`, then `PATH`, then `C:\Program Files\7-Zip\7z.exe` |
| `pdfcraft-cli` | PDF page count and rendering (Tier 2) | `MAGPIECRAFT_PDFCRAFT`, then `PATH`, then `C:\Program Files\PdfCraft\pdfcraft-cli.exe` |

Check with `magpiecraft-mcp tools`.

## Data directory

Default `%LOCALAPPDATA%\magpiecraft` on Windows, `$XDG_DATA_HOME/magpiecraft` or
`~/.local/share/magpiecraft` elsewhere.

| Variable | Effect |
|---|---|
| `MAGPIECRAFT_HOME` | Use this directory instead (tests and `--smoke` use it) |
| `MAGPIECRAFT_CONFIG` | Not used: settings live in `settings.json` under the home |
| `MAGPIECRAFT_FFPROBE`, `MAGPIECRAFT_7Z`, `MAGPIECRAFT_PDFCRAFT` | Explicit tool paths |

Contents: `catalogue.db`, `settings.json`, `cache/covers`, `manifests`. Delete the cache folder at any
time; delete `catalogue.db` to start over (you lose reading progress and overrides, never files).

## Uninstall

Delete the two binaries and the data directory. Nothing is registered anywhere else, and no library
folder was modified, so there is nothing else to undo.

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `no libraries yet` | Add a library first; `library-add` with a root that does not exist is rejected |
| Covers show "no cover" | The first page cannot be decoded (CBR/RAR, PDF, AVIF, or a corrupt page); the item is still listed |
| `not readable: 7-Zip extraction is Tier 2` | CBR/RAR page reading is not built yet; the archive is indexed |
| Scan skips a library | Its root is unreachable (unplugged drive). The index and reading progress are kept |
| `status` says a tool is missing | Install it, or set the `MAGPIECRAFT_*` variable |
| MCP client shows no tools | The client must start `magpiecraft-mcp.exe mcp` (with the `mcp` argument), stdout must be clean |

More in [HELP.md](HELP.md) and `docs/DEVELOPMENT.md`.
