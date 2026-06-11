use std::path::Path;
use std::sync::OnceLock;

use pdfium_render::prelude::*;

use super::EngineError;

#[derive(Clone, Debug)]
pub struct PageInfo {
    pub width_pt: f32,
    pub height_pt: f32,
}

static PDFIUM: OnceLock<Pdfium> = OnceLock::new();

pub fn ensure_pdfium() -> Result<(), EngineError> {
    if PDFIUM.get().is_some() {
        return Ok(());
    }

    let lib_path = pdfium_auto::ensure_pdfium_library(Some(&|downloaded, total| {
        if let Some(total) = total {
            let pct = if total > 0 {
                (downloaded as f64 / total as f64 * 100.0) as u32
            } else {
                0
            };
            tracing::info!("PDF engine: {downloaded}/{total} bytes ({pct}%)");
        } else {
            tracing::info!("PDF engine: {downloaded} bytes downloaded");
        }
    }))
    .map_err(|e| EngineError::Pdfium(format!("Failed to download PDF engine: {e}")))?;

    let bindings = Pdfium::bind_to_library(&lib_path)
        .map_err(|e| EngineError::Pdfium(format!("Failed to bind PDF engine: {e}")))?;

    let pdfium = Pdfium::new(bindings);
    PDFIUM.set(pdfium)
        .map_err(|_| EngineError::Pdfium("PDF engine already initialized".into()))?;

    tracing::info!("PDF engine ready");
    Ok(())
}

fn get_pdfium() -> Result<&'static Pdfium, EngineError> {
    ensure_pdfium()?;
    Ok(PDFIUM.get().unwrap())
}

pub struct PdfDoc {
    pub pdfium: &'static Pdfium,
    file_data: Vec<u8>,
    page_count: usize,
    pages: Vec<PageInfo>,
    path: std::path::PathBuf,
}

impl std::fmt::Debug for PdfDoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfDoc")
            .field("page_count", &self.page_count)
            .field("path", &self.path)
            .finish()
    }
}

impl PdfDoc {
    pub fn open(path: &Path) -> Result<Self, EngineError> {
        let file_data = std::fs::read(path)?;
        let pdfium = get_pdfium()?;

        let (page_count, pages) = {
            let doc = pdfium
                .load_pdf_from_byte_slice(&file_data, None)
                .map_err(|e| EngineError::Pdfium(format!("Failed to load PDF: {e}")))?;

            let page_count = doc.pages().len() as usize;
            let mut pages = Vec::with_capacity(page_count);

            for page in doc.pages().iter() {
                let width = page.width().value;
                let height = page.height().value;
                pages.push(PageInfo {
                    width_pt: width,
                    height_pt: height,
                });
            }

            (page_count, pages)
        };

        tracing::info!(
            "Loaded PDF with {} pages from {}",
            page_count,
            path.display()
        );

        Ok(Self {
            pdfium,
            file_data,
            page_count,
            pages,
            path: path.to_path_buf(),
        })
    }

    pub fn page_count(&self) -> usize {
        self.page_count
    }

    pub fn page_info(&self, index: usize) -> Option<&PageInfo> {
        self.pages.get(index)
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn with_document<R>(&self, f: impl FnOnce(&PdfDocument) -> R) -> Result<R, EngineError> {
        let doc = self
            .pdfium
            .load_pdf_from_byte_slice(&self.file_data, None)
            .map_err(|e| EngineError::Pdfium(format!("Failed to load PDF: {e}")))?;
        Ok(f(&doc))
    }

    pub fn render_page(
        _pdfium: &Pdfium,
        doc: &PdfDocument,
        index: usize,
        render_width: u32,
    ) -> Result<image::RgbaImage, EngineError> {
        let page = doc
            .pages()
            .get(index as PdfPageIndex)
            .map_err(|_| EngineError::InvalidPage(index))?;

        let config = PdfRenderConfig::new()
            .set_target_width(render_width as Pixels)
            .set_maximum_height(30_000_i32);

        let bitmap = page
            .render_with_config(&config)
            .map_err(|e| EngineError::Pdfium(format!("Render failed: {e}")))?;

        let image = bitmap
            .as_image()
            .map_err(|e| EngineError::Pdfium(format!("Image conversion failed: {e}")))?;

        Ok(image.into_rgba8())
    }

    pub fn extract_page_text(
        _pdfium: &Pdfium,
        doc: &PdfDocument,
        index: usize,
    ) -> Result<String, EngineError> {
        let page = doc
            .pages()
            .get(index as PdfPageIndex)
            .map_err(|_| EngineError::InvalidPage(index))?;

        let text = page
            .text()
            .map_err(|e| EngineError::Pdfium(format!("Text extraction failed: {e}")))?
            .all()
            .to_string();

        Ok(text)
    }
}
