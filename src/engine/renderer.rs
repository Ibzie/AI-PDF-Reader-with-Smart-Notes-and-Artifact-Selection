use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use parking_lot::Mutex;

use super::document::PdfDoc;
use super::EngineError;

const MAX_CACHED_BITMAPS: usize = 50;

pub type RenderedPage = ::image::RgbaImage;

pub struct PageRenderer {
    document: Arc<PdfDoc>,
    bitmap_cache: Mutex<BitmapCache>,
    lru_order: Mutex<VecDeque<usize>>,
}

struct BitmapCache {
    bitmaps: HashMap<usize, Arc<RenderedPage>>,
}

impl BitmapCache {
    fn new() -> Self {
        Self {
            bitmaps: HashMap::with_capacity(MAX_CACHED_BITMAPS),
        }
    }
}

impl PageRenderer {
    pub fn new(document: Arc<PdfDoc>) -> Self {
        Self {
            document,
            bitmap_cache: Mutex::new(BitmapCache::new()),
            lru_order: Mutex::new(VecDeque::with_capacity(MAX_CACHED_BITMAPS)),
        }
    }

    pub fn page_count(&self) -> usize {
        self.document.page_count()
    }

    pub fn page_render_size(&self, index: usize, zoom: f32) -> Option<(u32, u32)> {
        let info = self.document.page_info(index)?;
        let width = (info.width_pt * zoom) as u32;
        let aspect = info.height_pt / info.width_pt;
        let height = (width as f32 * aspect) as u32;
        Some((width.max(1), height.max(1)))
    }

    pub fn get_or_render_sync(
        &self,
        index: usize,
        zoom: f32,
    ) -> Result<Arc<RenderedPage>, EngineError> {
        if index >= self.document.page_count() {
            return Err(EngineError::InvalidPage(index));
        }

        {
            let cache = self.bitmap_cache.lock();
            if let Some(bitmap) = cache.bitmaps.get(&index) {
                let (expected_w, _) = self.page_render_size(index, zoom).unwrap_or((1, 1));
                if bitmap.width() == expected_w {
                    self.touch_lru(index);
                    return Ok(Arc::clone(bitmap));
                }
            }
        }

        let (render_w, _) = self.page_render_size(index, zoom).unwrap_or((1, 1));

        let bitmap = self.document.with_document(|doc| {
            PdfDoc::render_page(self.document.pdfium, doc, index, render_w)
        })??;

        let bitmap = Arc::new(bitmap);

        {
            let mut cache = self.bitmap_cache.lock();
            self.evict_lru_if_needed(&mut cache);
            cache.bitmaps.insert(index, Arc::clone(&bitmap));
        }

        self.touch_lru(index);

        Ok(bitmap)
    }

    fn touch_lru(&self, index: usize) {
        let mut order = self.lru_order.lock();
        if let Some(pos) = order.iter().position(|&x| x == index) {
            order.remove(pos);
        }
        order.push_back(index);
    }

    fn evict_lru_if_needed(&self, cache: &mut BitmapCache) {
        let mut order = self.lru_order.lock();
        while order.len() > MAX_CACHED_BITMAPS {
            if let Some(oldest) = order.pop_front() {
                cache.bitmaps.remove(&oldest);
            }
        }
    }

    pub fn invalidate_zoom(&self) {
        let mut cache = self.bitmap_cache.lock();
        cache.bitmaps.clear();
        let mut order = self.lru_order.lock();
        order.clear();
    }
}
