# AI-PDF

A fast, native PDF reader with a graphical interface, built in Rust.

## Features

- **GPU-accelerated rendering** via PDFium and wgpu
- **Page-by-page scrolling** with virtual rendering and LRU cache
- **Full-text search** across all pages using regex
- **Bookmark / table of contents** navigation in sidebar
- **Zoom controls** with predefined levels (25%–400%) and fit-to-width mode
- **Keyboard shortcuts** for all common actions
- **Dark theme** by default
- **Cross-platform** — Linux, macOS, Windows

## Installation

Requires the [Rust toolchain](https://rustup.rs).

```bash
git clone https://github.com/anomalyco/ai-pdf.git
cd ai-pdf
cargo build --release
```

On first run, the PDFium native library is downloaded automatically.

## Usage

```bash
# Open the welcome screen
cargo run --release

# Open a PDF directly
cargo run --release -- document.pdf
```

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+O` | Open file |
| `Ctrl+0` | Toggle fit-to-width |
| `Ctrl+=` / `Ctrl++` | Zoom in |
| `Ctrl+-` | Zoom out |
| `Alt+Right` | Next page |
| `Alt+Left` | Previous page |
| `Esc` | Clear search |

## Tech Stack

Rust, [Iced](https://iced.rs/), [PDFium](https://pdfium.googlesource.com/pdfium/), [lopdf](https://crates.io/crates/lopdf)

## License

MIT
