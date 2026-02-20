# rzip

A fast, cross-platform file compression and archiving tool written in Rust. Uses [Zstandard (zstd)](https://github.com/facebook/zstd) for high-performance compression with support for extracting ZIP and RAR archives.

## Features

- **Fast compression** — multithreaded zstd compression with 5 profiles
- **Directory support** — compresses entire folders into a single `.zst` archive (via tar)
- **Extract ZIP and RAR** — extract third-party archives alongside native `.zst` files
- **Progress bars** — live byte-level progress for all operations
- **Safe extraction** — Zip Slip protection, path traversal prevention
- **Compression stats** — shows before/after size and space saved
- **Preserves metadata** — file permissions and modification timestamps (Unix)
- **Clean on failure** — incomplete output files are removed on error or Ctrl+C
- **Cross-platform** — Windows, macOS, Linux

## Installation

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- `unrar` — only required for RAR extraction
  - **Linux:** `sudo apt install unrar`
  - **macOS:** `brew install rar`
  - **Windows:** [Download from rarlab.com](https://www.rarlab.com/download.htm)

### Build from source

```bash
git clone https://github.com/yourusername/rzip
cd rzip
cargo build --release
```

The binary will be at `target/release/rzip` (or `rzip.exe` on Windows).

## Usage

### Compress

```bash
# Compress a file (creates file.txt.zst)
rzip --compress file.txt

# Compress a directory (creates mydir.zst)
rzip --compress mydir/

# Custom output path
rzip --compress file.txt --output-path archive.zst

# Choose compression profile
rzip --compress file.txt --compress-profile ultra
```

### Extract

```bash
# Extract a .zst file
rzip --extract archive.zst

# Extract a ZIP file
rzip --extract archive.zip

# Extract a RAR file
rzip --extract archive.rar

# Custom output path
rzip --extract archive.zst --output-path ./output/
```

## Compression Profiles

| Profile    | Level | Description                        |
|------------|-------|------------------------------------|
| `fastest`  | 1     | Minimal compression, maximum speed |
| `fast`     | 5     | Light compression, very fast       |
| `balanced` | 10    | Good ratio and speed *(default)*   |
| `high`     | 15    | High compression, slower           |
| `ultra`    | 22    | Maximum compression, slowest       |

## Archive Format

Native `.zst` archives use a custom 6-byte header:

| Bytes | Value    | Meaning              |
|-------|----------|----------------------|
| 0–4   | `RZIPF`  | Single file archive  |
| 0–4   | `RZIPD`  | Directory archive    |
| 5     | `0x01`   | Format version       |

The header is followed by raw zstd-compressed data (single file) or a zstd-compressed tar stream (directory).

## Building for all platforms (GitHub Actions)

```yaml
jobs:
  build-linux:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - run: cargo build --release
      - uses: actions/upload-artifact@v3
        with:
          name: rzip-linux
          path: target/release/rzip

  build-windows:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v3
      - run: cargo build --release
      - uses: actions/upload-artifact@v3
        with:
          name: rzip-windows
          path: target/release/rzip.exe

  build-macos:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v3
      - run: cargo build --release
      - uses: actions/upload-artifact@v3
        with:
          name: rzip-macos
          path: target/release/rzip
```

## Dependencies

| Crate       | Purpose                          |
|-------------|----------------------------------|
| `zstd`      | Zstandard compression/decompression |
| `tar`       | TAR archive creation/extraction  |
| `zip`       | ZIP archive extraction           |
| `clap`      | CLI argument parsing             |
| `indicatif` | Progress bars and spinners       |
| `num_cpus`  | Multithreaded compression        |
| `ctrlc`     | Ctrl+C signal handling           |

## License

MIT
