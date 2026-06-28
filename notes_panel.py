from PyQt6.QtWidgets import QWidget, QVBoxLayout, QLabel, QTextEdit


class NotesPanel(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(6)
        layout.addWidget(QLabel("Notes"))
        self.editor = QTextEdit()
        self.editor.setAcceptRichText(False)
        self.editor.setStyleSheet(
            "QTextEdit { background-color: #1a1a1a; color: #eee; border: 1px solid #444; }"
        )
        self.editor.textChanged.connect(self._on_change)
        layout.addWidget(self.editor)
        self._callback = None
        self._stream_heading = None
        self._stream_body = ""

    def set_text(self, text):
        self.editor.blockSignals(True)
        self.editor.setPlainText(text)
        self.editor.blockSignals(False)

    def get_text(self):
        return self.editor.toPlainText()

    def append_markdown(self, block):
        text = self.get_text()
        if text and not text.endswith("\n"):
            text += "\n"
        text += block.rstrip() + "\n\n"
        self.set_text(text)
        if self._callback:
            self._callback(text)

    def stream_start(self, heading):
        text = self.get_text()
        if text and not text.endswith("\n\n"):
            text += "\n" if text.endswith("\n") else "\n\n"
        self._stream_heading = heading
        self._stream_body = ""
        text += heading + "\n"
        self.set_text(text)

    def stream_token(self, tok):
        if self._stream_heading is None:
            return
        self._stream_body += tok
        text = self.get_text()
        idx = text.find(self._stream_heading)
        if idx >= 0:
            text = text[:idx] + self._stream_heading + "\n" + self._stream_body
            self.set_text(text)

    def stream_end(self):
        if self._stream_heading is None:
            return
        text = self.get_text()
        if not text.endswith("\n"):
            text += "\n"
        text += "\n"
        self.set_text(text)
        if self._callback:
            self._callback(text)
        self._stream_heading = None
        self._stream_body = ""

    def on_save(self, callback):
        self._callback = callback

    def _on_change(self):
        if self._callback:
            self._callback(self.get_text())
