import sys, json, os, logging, logging.handlers
from datetime import datetime
from pathlib import Path
from PyQt6.QtCore import Qt, QPoint
from PyQt6.QtGui import QColor, QKeyEvent, QCursor
from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QToolBar, QLabel, QLineEdit, QPushButton, QScrollArea, QFrame,
    QFileDialog, QMessageBox, QMenu, QListWidget, QListWidgetItem,
    QStatusBar, QSplitter, QInputDialog, QProgressBar, QWidgetAction
)
from pdf_engine import PdfEngine, ZOOM_LEVELS
from page_view import PageView
from notes_panel import NotesPanel
from storage import PdfStorage
from ai_layer import AILayer, TIERS, detect_capacity, fit_level
from ai_workers import LoadWorker, InferWorker


class MainWindow(QMainWindow):
    def __init__(self, cli_path=None):
        super().__init__()
        self.engine = PdfEngine()
        self.storage = None
        self.pages = []
        self.zoom_index = 5
        self.zoom_level = 1.0
        self.fit_width = True
        self.current_page = 0
        self.search_results = []
        self.search_index = 0
        self.capture_mode = False
        self.ai = AILayer()
        self.ai_loader = None
        self.ai_infer = None
        self.setWindowTitle("AI-PDF Reader")
        self.resize(1600, 900)
        self.setStyleSheet("background-color: #121212; color: #eeeeee;")
        self._build_toolbar()
        self._build_sidebar()
        self._build_notes_panel()
        self._build_viewer()
        self.status = QStatusBar()
        self.setStatusBar(self.status)
        self.status.showMessage("Ready — Open a PDF file (Ctrl+O)")
        if cli_path:
            self.load_pdf(cli_path)

    def _build_toolbar(self):
        self.toolbar = QToolBar()
        self.addToolBar(self.toolbar)
        self.toolbar.setStyleSheet(
            "QToolBar { background: #1e1e1e; border: none; spacing: 4px; padding: 4px; }"
            "QPushButton { background: #333; color: #eee; border: none; padding: 4px 8px; }"
            "QPushButton:hover { background: #444; }"
            "QLineEdit { background: #2a2a2a; color: #eee; border: 1px solid #444; padding: 2px; }"
        )
        self.page_label = QLabel("0 / 0")
        self.zoom_label = QLabel("Fit Width")
        self.search_box = QLineEdit()
        self.search_box.setPlaceholderText("Search...")
        self.search_box.setFixedWidth(200)
        self.search_box.returnPressed.connect(self.search)
        self.search_info = QLabel("")
        self.toolbar.addWidget(QPushButton("Open", clicked=self.open_file))
        self.toolbar.addWidget(QPushButton("◀", clicked=self.prev_page))
        self.toolbar.addWidget(self.page_label)
        self.toolbar.addWidget(QPushButton("▶", clicked=self.next_page))
        self.toolbar.addSeparator()
        self.toolbar.addWidget(QPushButton("−", clicked=lambda: self._adjust_zoom(-1)))
        self.toolbar.addWidget(self.zoom_label)
        self.toolbar.addWidget(QPushButton("+", clicked=lambda: self._adjust_zoom(1)))
        self.toolbar.addWidget(QPushButton("⊞", clicked=self.toggle_fit_width))
        self.toolbar.addSeparator()
        self.toolbar.addWidget(self.search_box)
        self.toolbar.addWidget(self.search_info)
        self.toolbar.addWidget(QPushButton("↓", clicked=lambda: self._navigate_search(1)))
        self.toolbar.addWidget(QPushButton("↑", clicked=lambda: self._navigate_search(-1)))
        self.toolbar.addWidget(QPushButton("✕", clicked=self.clear_search))
        self.toolbar.addSeparator()
        self.toolbar.addWidget(QPushButton("☰", clicked=self.toggle_sidebar))
        self.toolbar.addWidget(QPushButton("📝", clicked=self.toggle_notes))
        self.toolbar.addSeparator()
        self.model_label = QPushButton("AI: idle")
        self.model_label.setStyleSheet(
            "QPushButton { background: #2a2a2a; color: #aaa; border: 1px solid #444; "
            "padding: 4px 10px; text-align: left; } QPushButton:hover { background: #333; }"
        )
        self.model_label.clicked.connect(self.show_model_menu)
        self.toolbar.addWidget(self.model_label)
        self.toolbar.addWidget(QPushButton("AI", clicked=self.show_ai_menu))
        self.ai_progress = QProgressBar()
        self.ai_progress.setFixedWidth(200)
        self.ai_progress.setTextVisible(True)
        self.ai_progress.setStyleSheet(
            "QProgressBar { background: #2a2a2a; border: 1px solid #444; height: 16px; }"
            "QProgressBar::chunk { background: #4a90d9; }"
        )
        self.ai_progress.setVisible(False)
        self.ai_progress_lbl = QLabel("")
        self.ai_progress_lbl.setStyleSheet("color: #aaa;")
        self.ai_progress_lbl.setVisible(False)
        self.toolbar.addWidget(self.ai_progress_lbl)
        self.toolbar.addWidget(self.ai_progress)

    def _build_sidebar(self):
        self.sidebar = QFrame()
        self.sidebar.setFixedWidth(260)
        self.sidebar.setStyleSheet("background-color: #1a1a1a;")
        layout = QVBoxLayout(self.sidebar)
        layout.setContentsMargins(8, 8, 8, 8)
        self.bookmark_list = QListWidget()
        self.bookmark_list.itemClicked.connect(self.bookmark_clicked)
        self.info_label = QLabel("")
        self.info_label.setWordWrap(True)
        layout.addWidget(QLabel("Bookmarks"))
        layout.addWidget(self.bookmark_list)
        layout.addSpacing(16)
        layout.addWidget(QLabel("Document Info"))
        layout.addWidget(self.info_label)
        layout.addStretch()

    def _build_notes_panel(self):
        self.notes_panel = NotesPanel()
        self.notes_panel.on_save(self._save_notes)
        self.notes_panel.setMaximumWidth(420)
        self.notes_panel.setMinimumWidth(260)

    def _build_viewer(self):
        central = QWidget()
        self.setCentralWidget(central)
        hbox = QHBoxLayout(central)
        hbox.setContentsMargins(0, 0, 0, 0)
        hbox.setSpacing(0)
        self.splitter = QSplitter(Qt.Orientation.Horizontal)
        self.splitter.addWidget(self.sidebar)
        self.scroll = QScrollArea()
        self.scroll.setWidgetResizable(True)
        self.scroll.setStyleSheet("background-color: #121212; border: none;")
        self.scroll.verticalScrollBar().valueChanged.connect(self.update_current_page)
        self.pages_widget = QWidget()
        self.pages_layout = QVBoxLayout(self.pages_widget)
        self.pages_layout.setSpacing(8)
        self.pages_layout.setContentsMargins(20, 20, 20, 20)
        self.pages_layout.addStretch()
        self.scroll.setWidget(self.pages_widget)
        self.splitter.addWidget(self.scroll)
        self.splitter.addWidget(self.notes_panel)
        self.splitter.setSizes([260, 960, 340])
        hbox.addWidget(self.splitter)

    def _save_notes(self, text):
        if self.storage:
            self.storage.save_notes(text)

    def open_file(self):
        path, _ = QFileDialog.getOpenFileName(self, "Open PDF", "", "PDF Files (*.pdf)")
        if path:
            self.load_pdf(path)

    def load_pdf(self, path):
        try:
            self.engine.open(path)
        except Exception as e:
            QMessageBox.critical(self, "Error", f"Failed to load PDF:\n{e}")
            return
        self.storage = PdfStorage(path)
        self.current_page = 0
        self.zoom_index = 5
        self.zoom_level = 1.0
        self.fit_width = True
        self.search_results = []
        self.search_index = 0
        self.search_box.clear()
        self.search_info.setText("")
        self.notes_panel.set_text(self.storage.load_notes())
        self.notes_panel.set_base_dir(self.storage.folder)
        self._populate_bookmarks()
        self._update_info()
        self._rebuild_pages()
        self._apply_persisted_highlights()
        self.status.showMessage(f"Loaded {Path(path).name} — {self.engine.page_count} pages")

    def _populate_bookmarks(self):
        self.bookmark_list.clear()
        for bm in self.engine.bookmarks:
            item = QListWidgetItem("  " * bm["level"] + bm["title"])
            item.setData(Qt.ItemDataRole.UserRole, bm["page"])
            self.bookmark_list.addItem(item)

    def _update_info(self):
        size = os.path.getsize(self.engine.path)
        size_str = f"{size / 1024:.1f} KB" if size >= 1024 else f"{size} B"
        lines = [
            f"File: {Path(self.engine.path).name}",
            f"Size: {size_str}",
            f"Pages: {self.engine.page_count}",
            f"Version: {self.engine.version}",
        ]
        self.info_label.setText("\n".join(lines))

    def _rebuild_pages(self):
        for p in self.pages:
            p.deleteLater()
        self.pages = []
        while self.pages_layout.count() > 1:
            self.pages_layout.takeAt(0).widget().deleteLater()
        if not self.engine.doc:
            return
        for i in range(self.engine.page_count):
            w_pt, h_pt = self.engine.page_sizes[i]
            page = PageView(i, w_pt, h_pt)
            page.rightClicked.connect(self.show_context_menu)
            page.captureDone.connect(self.complete_capture)
            page.set_chars(self.engine.get_text_chars(i))
            self.pages_layout.insertWidget(self.pages_layout.count() - 1, page)
            self.pages.append(page)
        self._apply_zoom()

    def _apply_zoom(self):
        if self.fit_width:
            self.zoom_level = (self.scroll.viewport().width() - 60) / self.engine.page_sizes[0][0]
            self.zoom_label.setText("Fit Width")
        else:
            self.zoom_level = ZOOM_LEVELS[self.zoom_index]
            self.zoom_label.setText(f"{int(self.zoom_level * 100)}%")
        self.engine.invalidate_cache()
        for page in self.pages:
            page.set_zoom(self.zoom_level)
            img = self.engine.render_page(page.page_idx, page.width())
            page.set_image(img)
        self.update_current_page()

    def _apply_persisted_highlights(self):
        if not self.storage:
            return
        for page in self.pages:
            hls = [h["bbox"] for h in self.storage.get_highlights_for_page(page.page_idx)]
            page.set_highlights(hls)

    def update_current_page(self):
        if not self.pages:
            return
        y = self.scroll.verticalScrollBar().value()
        mid = y + self.scroll.viewport().height() // 2
        for i, page in enumerate(self.pages):
            top = page.mapTo(self.pages_widget, QPoint(0, 0)).y()
            bottom = top + page.height()
            if top <= mid <= bottom:
                self.current_page = i
                break
        self.page_label.setText(f"{self.current_page + 1} / {self.engine.page_count}")

    def go_to_page(self, idx):
        if not self.pages or idx < 0 or idx >= len(self.pages):
            return
        self.current_page = idx
        y = self.pages[idx].mapTo(self.pages_widget, QPoint(0, 0)).y()
        self.scroll.verticalScrollBar().setValue(y)
        self.page_label.setText(f"{idx + 1} / {self.engine.page_count}")

    def next_page(self):
        self.go_to_page(min(self.current_page + 1, self.engine.page_count - 1))

    def prev_page(self):
        self.go_to_page(max(self.current_page - 1, 0))

    def _adjust_zoom(self, delta):
        if self.fit_width:
            self.fit_width = False
            self.zoom_index = 5
        else:
            self.zoom_index = max(0, min(len(ZOOM_LEVELS) - 1, self.zoom_index + delta))
        self._apply_zoom()

    def toggle_fit_width(self):
        self.fit_width = not self.fit_width
        if not self.fit_width:
            self.zoom_index = 5
        self._apply_zoom()

    def toggle_sidebar(self):
        self.sidebar.setVisible(not self.sidebar.isVisible())

    def toggle_notes(self):
        self.notes_panel.setVisible(not self.notes_panel.isVisible())

    def bookmark_clicked(self, item):
        self.go_to_page(item.data(Qt.ItemDataRole.UserRole))

    def search(self):
        self.search_results = self.engine.search(self.search_box.text())
        self.search_index = 0
        if self.search_results:
            self.search_info.setText(f"1 / {len(self.search_results)}")
            self.go_to_page(self.search_results[0])
        else:
            self.search_info.setText("0 / 0")
            self.status.showMessage(f'No results for "{self.search_box.text()}"')

    def _navigate_search(self, direction):
        if self.search_results:
            self.search_index = (self.search_index + direction) % len(self.search_results)
            self.search_info.setText(f"{self.search_index + 1} / {len(self.search_results)}")
            self.go_to_page(self.search_results[self.search_index])

    def clear_search(self):
        self.search_box.clear()
        self.search_results = []
        self.search_index = 0
        self.search_info.setText("")

    def show_context_menu(self, page_idx, global_pos, pdf_pos):
        page = self.pages[page_idx]
        menu = QMenu(self)
        menu.setStyleSheet(
            "QMenu { background-color: #1e1e1e; color: #eee; border: 1px solid #444; }"
            "QMenu::item:selected { background-color: #333; }"
        )
        selected_text = page.get_selection_text()
        screen_pos = QPoint(int(pdf_pos.x() * page.zoom), int(pdf_pos.y() * page.zoom))
        hl_idx = page.highlight_at(screen_pos)
        if selected_text:
            menu.addAction("Copy", lambda: self.copy_selection(page))
            menu.addAction("Add to Notes", lambda: self.add_highlight_to_notes(page))
        if hl_idx is not None:
            menu.addAction("Remove Highlight", lambda: self.remove_highlight(page_idx, hl_idx))
        if not selected_text and hl_idx is None:
            menu.addAction("Capture Screen", self.start_capture)
        menu.addSeparator()
        menu.addAction("Dismiss", menu.close)
        menu.exec(global_pos)

    def copy_selection(self, page):
        text = page.get_selection_text()
        if text:
            QApplication.clipboard().setText(text)
            self.status.showMessage("Copied to clipboard")
        page.clear_selection()

    def add_highlight_to_notes(self, page):
        text = page.get_selection_text()
        bbox = page.get_selection_bbox()
        if not text or not bbox or not self.storage:
            page.clear_selection()
            return
        img = self.engine.render_page(page.page_idx, page.width())
        x, y, x1, y1 = bbox
        sx, sy = int(x * page.zoom), int(y * page.zoom)
        sw, sh = int((x1 - x) * page.zoom), int((y1 - y) * page.zoom)
        cropped = img.copy(sx, sy, sw, sh)
        filepath, entry = self.storage.save_highlight(page.page_idx, cropped, bbox, text)
        page.highlights.append(bbox)
        page.update()
        rel = filepath.relative_to(self.storage.folder).as_posix()
        self.notes_panel.append_markdown(
            f"### Highlight — Page {page.page_idx + 1}\n\n"
            f"> {text}\n\n"
            f"![highlight]({rel})\n\n"
            f"_Page {page.page_idx + 1}_"
        )
        self.status.showMessage("Highlight added to notes")
        page.clear_selection()

    def remove_highlight(self, page_idx, hl_idx):
        if not self.storage:
            return
        self.storage.remove_highlight(page_idx, hl_idx)
        self._apply_persisted_highlights()
        self.status.showMessage("Highlight removed")

    def start_capture(self):
        self.capture_mode = True
        self.status.showMessage("Capturing: drag to select region, Esc to cancel")
        for page in self.pages:
            page.set_capture_mode(True)

    def cancel_capture(self):
        self.capture_mode = False
        for page in self.pages:
            page.set_capture_mode(False)
        self.status.showMessage("Capture cancelled")

    def complete_capture(self, page_idx):
        page = self.pages[page_idx]
        rect = page.get_capture_rect()
        if not rect or rect.width() < 10 or rect.height() < 10 or not self.storage:
            page.clear_selection()
            self.cancel_capture()
            return
        img = self.engine.render_page(page.page_idx, page.width())
        cropped = img.copy(rect)
        filepath, entry = self.storage.save_capture(page.page_idx, cropped)
        rel = filepath.relative_to(self.storage.folder).as_posix()
        self.notes_panel.append_markdown(
            f"### Capture — Page {page.page_idx + 1}\n\n"
            f"![capture]({rel})\n\n"
            f"_Page {page.page_idx + 1}_"
        )
        self.status.showMessage(f"Capture saved: {filepath.name}")
        page.clear_selection()
        self.cancel_capture()

    # ── AI layer ────────────────────────────────────────────────────────────
    def show_model_menu(self):
        ram, _ = detect_capacity()
        menu = QMenu(self)
        menu.setStyleSheet(
            "QMenu { background-color: #1e1e1e; border: 1px solid #444; padding: 4px; }")
        for i, tier in enumerate(TIERS):
            fit, color = fit_level(tier["footprint"], ram)
            btn = QPushButton(
                f"\u25cf  {tier['name']:<26} {fit:<10} ~{tier['footprint']:.1f} GB")
            btn.setStyleSheet(
                "QPushButton { text-align: left; padding: 8px 16px; border: none; "
                "background: transparent; color: #eee; font-family: monospace; font-size: 13px; }"
                "QPushButton:hover { background: #333; }"
                f"QPushButton {{ color: {color}; }}"
            )
            idx = i
            btn.clicked.connect(
                lambda checked=False, idx=idx: (menu.close(), self._load_ai(idx)))
            wa = QWidgetAction(menu)
            wa.setDefaultWidget(btn)
            menu.addAction(wa)
        if self.ai.is_loaded():
            menu.addSeparator()
            menu.addAction("Unload Model", self._unload_ai)
        menu.exec(QCursor.pos())

    def show_ai_menu(self):
        menu = QMenu(self)
        menu.setStyleSheet(
            "QMenu { background-color: #1e1e1e; color: #eee; border: 1px solid #444; }"
            "QMenu::item:selected { background-color: #333; }"
        )
        if not self.ai.is_loaded():
            menu.addAction("Load AI Model", self._load_ai)
        else:
            notes = self.notes_panel.get_text()
            menu.addAction("Summarize Notes", lambda: self._run_ai("summarize_notes", notes=notes))
            if self.engine.doc:
                idx = self.current_page
                txt = self.engine.extract_page_text(idx)
                menu.addAction("Summarize Current Page",
                               lambda: self._run_ai("summarize_page", page_text=txt, page_idx=idx))
            menu.addAction("Ask…", self._ai_ask)
            menu.addAction("Extract To-Dos", lambda: self._run_ai("extract_todos", notes=notes))
            menu.addAction("Draft Follow-up", lambda: self._run_ai("draft", notes=notes))
            menu.addAction("Suggest Tags", lambda: self._run_ai("suggest_tags", notes=notes))
        menu.addSeparator()
        if self.ai_infer and self.ai_infer.isRunning():
            menu.addAction("Cancel AI", self._cancel_ai)
        menu.exec(QCursor.pos())

    def _load_ai(self, tier_idx=None):
        if self.ai_loader and self.ai_loader.isRunning():
            return
        if self.ai.is_loaded():
            self.ai.unload()
        self.model_label.setText("AI: loading…")
        self.ai_loader = LoadWorker(self.ai, tier_idx=tier_idx)
        self.ai_loader.status.connect(self._ai_load_status)
        self.ai_loader.progress.connect(self._ai_load_progress)
        self.ai_loader.done.connect(self._ai_loaded)
        self.ai_loader.failed.connect(self._ai_failed)
        self.ai_progress.setVisible(True)
        self.ai_progress_lbl.setVisible(True)
        self.ai_progress.setRange(0, 0)
        self.ai_progress.setValue(0)
        self.ai_progress_lbl.setText("AI: preparing…")
        self.ai_loader.start()

    def _unload_ai(self):
        self.ai.unload()
        self.model_label.setText("AI: idle")
        self.model_label.setStyleSheet(
            "QPushButton { background: #2a2a2a; color: #aaa; border: 1px solid #444; "
            "padding: 4px 10px; text-align: left; } QPushButton:hover { background: #333; }")
        self.status.showMessage("AI model unloaded")

    def _ai_load_status(self, msg):
        if msg.startswith(("Detected", "Listing", "Downloading", "Loading")):
            self.ai_progress_lbl.setText(msg)
            if msg.startswith("Downloading"):
                self.model_label.setText("AI: downloading…")
            elif msg.startswith("Loading"):
                self.model_label.setText("AI: loading…")
        else:
            self.status.showMessage(msg)

    def _ai_load_progress(self, done, total, label):
        if total <= 0:
            self.ai_progress.setRange(0, 0)
            self.ai_progress.setValue(0)
            self.model_label.setText("AI: downloading…")
        else:
            self.ai_progress.setRange(0, total)
            self.ai_progress.setValue(done)
            pct = int(done / total * 100) if total else 0
            self.model_label.setText(f"AI: downloading {pct}%")
        self.ai_progress_lbl.setText(label or self.ai_progress_lbl.text())

    def _ai_loaded(self, label):
        self.ai_progress.setVisible(False)
        self.ai_progress_lbl.setVisible(False)
        self.model_label.setText(label.replace("AI: ", ""))
        self.model_label.setStyleSheet(
            "QPushButton { background: #2a2a2a; color: #4caf50; border: 1px solid #444; "
            "padding: 4px 10px; text-align: left; } QPushButton:hover { background: #333; }")
        self.status.showMessage(label)

    def _ai_failed(self, msg):
        self.ai_progress.setVisible(False)
        self.ai_progress_lbl.setVisible(False)
        self.model_label.setText("AI: error")
        self.model_label.setStyleSheet(
            "QPushButton { background: #2a2a2a; color: #f44336; border: 1px solid #444; "
            "padding: 4px 10px; text-align: left; } QPushButton:hover { background: #333; }")
        self.status.showMessage(f"AI error: {msg}")

    def _ai_ask(self):
        q, ok = QInputDialog.getText(self, "Ask AI", "Question:")
        if ok and q:
            self._run_ai("answer", question=q, notes=self.notes_panel.get_text())

    def _run_ai(self, command, **kwargs):
        if not self.ai.is_loaded():
            self.status.showMessage("Load AI model first (toolbar → AI)")
            return
        if self.ai_infer and self.ai_infer.isRunning():
            self.status.showMessage("AI busy — press Esc to cancel")
            return
        self.ai_infer = InferWorker(self.ai, command, **kwargs)
        self.ai_infer.heading.connect(self.notes_panel.stream_start)
        self.ai_infer.token.connect(self.notes_panel.stream_token)
        self.ai_infer.finished_ok.connect(lambda _: self.notes_panel.stream_end())
        self.ai_infer.failed.connect(self._ai_failed)
        self.ai_infer.start()
        self.status.showMessage(f"AI: {command.replace('_', ' ')}…  (Esc to cancel)")

    def _cancel_ai(self):
        self.ai.request_cancel()
        self.status.showMessage("AI: cancelling…")

    def keyPressEvent(self, event: QKeyEvent):
        if event.key() == Qt.Key.Key_Escape:
            if self.capture_mode:
                self.cancel_capture()
            elif self.ai_infer and self.ai_infer.isRunning():
                self._cancel_ai()
            else:
                self.clear_search()
            return
        if event.modifiers() == Qt.KeyboardModifier.ControlModifier:
            if event.key() == Qt.Key.Key_O:
                self.open_file()
            elif event.key() == Qt.Key.Key_0:
                self.toggle_fit_width()
            elif event.key() in (Qt.Key.Key_Equal, Qt.Key.Key_Plus):
                self._adjust_zoom(1)
            elif event.key() == Qt.Key.Key_Minus:
                self._adjust_zoom(-1)
        elif event.modifiers() == Qt.KeyboardModifier.AltModifier:
            if event.key() == Qt.Key.Key_Right:
                self.next_page()
            elif event.key() == Qt.Key.Key_Left:
                self.prev_page()

    def resizeEvent(self, event):
        super().resizeEvent(event)
        if self.fit_width and self.pages:
            self._apply_zoom()

    def closeEvent(self, event):
        # Stop AI workers and free the model so failed runs don't leak RAM.
        for w in (self.ai_loader, self.ai_infer):
            if w and w.isRunning():
                self.ai.request_cancel()
                w.quit()
                w.wait(3000)
        self.ai.unload()
        super().closeEvent(event)


def main():
    # Lean AI logging to ai.log (rotates at 1 MB, keeps 1 backup).
    os.environ.setdefault("HF_HUB_DISABLE_PROGRESS_BARS", "1")
    os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")
    logging.basicConfig(
        handlers=[logging.handlers.RotatingFileHandler(
            "ai.log", maxBytes=1_000_000, backupCount=1)],
        level=logging.INFO, format="%(asctime)s %(name)s %(levelname)s %(message)s",
    )
    app = QApplication(sys.argv)
    app.setStyle("Fusion")
    palette = app.palette()
    palette.setColor(palette.ColorRole.Window, QColor(18, 18, 18))
    palette.setColor(palette.ColorRole.WindowText, QColor(238, 238, 238))
    palette.setColor(palette.ColorRole.Base, QColor(30, 30, 30))
    palette.setColor(palette.ColorRole.AlternateBase, QColor(40, 40, 40))
    palette.setColor(palette.ColorRole.Text, QColor(238, 238, 238))
    palette.setColor(palette.ColorRole.Button, QColor(50, 50, 50))
    palette.setColor(palette.ColorRole.ButtonText, QColor(238, 238, 238))
    app.setPalette(palette)
    path = sys.argv[1] if len(sys.argv) > 1 else None
    window = MainWindow(path)
    window.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
