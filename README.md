# AI-PDF

A fast, native PDF reader with a graphical interface, built in Python.

## Features

- **Native PDF rendering** via PyMuPDF (MuPDF)
- **Page-by-page scrolling** with LRU render cache
- **Full-text search** across all pages
- **Bookmark / table of contents** navigation in sidebar
- **Zoom controls** with predefined levels (25%–400%) and fit-to-width mode
- **Text highlights** — saved as images per PDF and stored in a notes markdown file
- **Screen captures** — saved as images per PDF and embedded into notes
- **Notes panel** — editable Markdown notes for each PDF (Obsidian-style)
- **Built-in AI layer** — runs a local model in-process (llama.cpp): summarize
  notes, summarize a page, Q&A grounded in your notes, extract to-dos, draft
  follow-ups, suggest tags. Streams output straight into the notes file.
- **Keyboard shortcuts** for all common actions
- **Dark theme** by default
- **Cross-platform** — Linux, macOS, Windows

> The AI layer needs ≥ 8 GB RAM. On first use it downloads an open-weights
> GGUF model (Qwen3.5/3.6 or Gemma-4) sized to your machine, via Hugging Face.

## Installation

Requires Python 3.10+.

```bash
git clone https://github.com/anomalyco/ai-pdf.git
cd ai-pdf
python -m venv .venv
source .venv/bin/activate  # On Windows: .venv\Scripts\activate
pip install -r requirements.txt
```

For GPU acceleration (NVIDIA) build `llama-cpp-python` against CUDA:

```bash
CMAKE_ARGS="-DGGML_CUDA=on" pip install --upgrade --force-reinstall llama-cpp-python
```

On Apple Silicon `Metal` is picked up automatically from the prebuilt wheel.

## Usage

```bash
# Open the welcome screen
python main.py

# Open a PDF directly
python main.py document.pdf
```

For each opened PDF a folder is created at `notes/<pdf-name>/` containing:

- `notes.md` — editable Markdown notes
- `highlights/` — image snippets of highlighted text
- `captures/` — image snippets of screen captures
- `annotations.json` — metadata index

### Quick Action Menu (right-click)

- **Selected text:** Copy / Add to Notes
- **Existing highlight:** Remove Highlight
- **Empty area:** Capture Screen

### AI menu (toolbar → AI)

| Command | What it does |
|---------|---------------|
| Load AI Model | Detects RAM/accel, downloads a fitting GGUF, loads in-process |
| Summarize Notes | Markdown bullet summary of the whole `notes.md` |
| Summarize Current Page | Bullet summary of the page in view |
| Ask… | Free-form Q&A grounded in your notes |
| Extract To-Dos | Markdown checklist of action items |
| Draft Follow-up | A short connecting note |
| Suggest Tags | A line of `#tag` tokens |

AI output is streamed live into the notes panel and saved like any other note.
`Esc` cancels an active run.

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+O` | Open file |
| `Ctrl+0` | Toggle fit-to-width |
| `Ctrl+=` / `Ctrl++` | Zoom in |
| `Ctrl+-` | Zoom out |
| `Alt+Right` | Next page |
| `Alt+Left` | Previous page |
| `Esc` | Clear search / cancel capture / cancel AI |

## Tech Stack

Python, [PyQt6](https://riverbankcomputing.com/software/pyqt/), [PyMuPDF](https://pymupdf.readthedocs.io/), [llama.cpp](https://github.com/ggml-org/llama.cpp) (via `llama-cpp-python`)

## License

MIT
