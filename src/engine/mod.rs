pub mod document;
pub mod renderer;

pub use document::PdfDoc;
pub use renderer::PageRenderer;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("PDF error: {0}")]
    Pdfium(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid page index: {0}")]
    InvalidPage(usize),
}
