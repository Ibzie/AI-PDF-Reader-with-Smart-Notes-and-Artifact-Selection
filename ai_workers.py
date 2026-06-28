from PyQt6.QtCore import QThread, pyqtSignal
from ai_layer import AILayer


class LoadWorker(QThread):
    status = pyqtSignal(str)
    progress = pyqtSignal(int, int, str)   # done_bytes, total_bytes, label
    done = pyqtSignal(str)                  # model label
    failed = pyqtSignal(str)               # error message

    def __init__(self, ai, parent=None):
        super().__init__(parent)
        self.ai = ai
        self.repo_id = None
        self.filename = None

    def run(self):
        try:
            self.ai.load_model(
                on_status=lambda m: self.status.emit(m),
                on_progress=lambda d, t, l: self.progress.emit(int(d), int(t), l),
                repo_id=self.repo_id, filename=self.filename,
            )
            self.done.emit(self.ai.model_label())
        except Exception as e:
            self.failed.emit(str(e))


class InferWorker(QThread):
    token = pyqtSignal(str)
    heading = pyqtSignal(str)
    finished_ok = pyqtSignal(str)   # full heading line written
    failed = pyqtSignal(str)

    def __init__(self, ai, command, parent=None, **kwargs):
        super().__init__(parent)
        self.ai = ai
        self.command = command
        self.kwargs = kwargs

    def run(self):
        try:
            messages, heading = self.ai.build_request(self.command, **self.kwargs)
            self.heading.emit(heading)
            # Lazily buffer so callers can also get a clean string, but stream
            # tokens to the UI as they arrive for live feedback.
            self.ai.generate(messages, on_token=lambda t: self.token.emit(t))
            self.finished_ok.emit(heading)
        except Exception as e:
            self.failed.emit(str(e))