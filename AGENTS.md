# AGENTS.md

## What this is
Single-binary Python app: a native GUI PDF reader with integrated Markdown notes and a built-in local AI layer. Entry point is `main.py`.

## Build & run
- Install: `pip install -r requirements.txt`
- GPU build (NVIDIA): `CMAKE_ARGS="-DGGML_CUDA=on" pip install --upgrade --force-reinstall llama-cpp-python`
- Run: `python main.py` (opens welcome screen) or `python main.py <file.pdf>` (auto-opens a file)
- No tests, no formatter/lint config, no CI. Match the existing zero-config style.

## Architecture
- `main.py` (~500 lines) — PyQt6 application. `MainWindow` with toolbar, left sidebar (bookmarks/info), right notes panel, scrollable page view, keyboard handling, search, highlights, capture, and the AI menu.
- `pdf_engine.py` (~120 lines) — `PdfEngine` wraps PyMuPDF; handles loading, rendering to `QImage`, text extraction with char bounds, search, and bookmarks.
- `page_view.py` (~160 lines) — `PageView` custom `QLabel` subclass. Displays a page, renders text-selection and highlight overlays, and emits right-click / capture signals.
- `storage.py` (~95 lines) — `PdfStorage` manages per-PDF folders at `notes/<pdf-name>/` with `notes.md`, `highlights/`, `captures/`, and `annotations.json`.
- `notes_panel.py` (~75 lines) — `NotesPanel` plain-text Markdown editor that auto-saves `notes.md`, plus streaming helpers for AI output.
- `ai_layer.py` (~220 lines) — `AILayer` detects RAM/accel, picks a model from `TIERS`, resolves a GGUF quant via Hugging Face, loads `llama-cpp-python` in-process with KV-cache quantization, and runs 6 prompt commands.
- `ai_workers.py` (~50 lines) — `LoadWorker` and `InferWorker` `QThread`s so the UI never blocks; tokens stream as `pyqtSignal`.
- Rendered page bitmaps are cached in `PdfEngine` with an LRU eviction policy (`MAX_CACHE = 50`).

## Conventions specific to this repo
- Dark theme via `QPalette` in `main.py`.
- Window starts at 1600×900 but is resizable; fit-to-width zoom adapts to viewport width.
- Keyboard shortcuts are centralized in `MainWindow.keyPressEvent`.
- Default zoom state: `fit_to_width = True`, `zoom_index = 5` (ZOOM_LEVELS index for 1.0). `ZOOM_LEVELS` is at `pdf_engine.py:4`.
- Each opened PDF gets its own folder at `notes/<pdf-name>/`.
- Highlights and captures are saved as PNGs and appended to the PDF's `notes.md` in Markdown format.
- AI model is per-machine (one load, reused across PDFs). Model tier table and quant preference order live in `ai_layer.py` (`TIERS`, `resolve_quant`). KV-cache uses `type_k=q8_0`/`type_v=q4_0` (tightened to `q4_0`/`q4_0` below 12 GB).

## Files worth knowing
- Entry point: `main.py`
- Engine: `pdf_engine.py`
- Page widget: `page_view.py`
- Storage: `storage.py`
- Notes panel: `notes_panel.py`
- AI layer: `ai_layer.py`, `ai_workers.py`
