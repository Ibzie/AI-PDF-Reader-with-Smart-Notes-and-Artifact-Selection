#![allow(mismatched_lifetime_syntaxes)]

mod engine;

use std::path::PathBuf;
use std::sync::Arc;

use iced::keyboard::{Key, Modifiers};
use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, row, scrollable, text,
    text_input, vertical_space,
};
use iced::widget::image as iced_image;
use iced::{Alignment, Element, Fill, Length, Subscription, Task};
use iced::keyboard::key;
use iced::time::Duration;

use crate::engine::{PdfDoc, PageRenderer};

const ZOOM_LEVELS: &[f32] = &[0.25, 0.33, 0.50, 0.67, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0];

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("ai_pdf=info".parse().unwrap()),
        )
        .with_target(false)
        .init();

    iced::application("AI-PDF Reader", AiPdfApp::update, AiPdfApp::view)
        .subscription(AiPdfApp::subscription)
        .window_size((1400.0, 900.0))
        .theme(|_| iced::Theme::Dark)
        .run_with(|| {
            let path = std::env::args().nth(1).map(PathBuf::from);
            let init_task = init_pdfium(path);
            (AiPdfApp::new(), init_task)
        })
}

fn init_pdfium(cli_path: Option<PathBuf>) -> Task<Message> {
    Task::perform(
        tokio::task::spawn_blocking(move || {
            if let Err(e) = engine::document::ensure_pdfium() {
                return Err(format!("Failed to initialize PDF engine: {e}"));
            }
            if let Some(path) = cli_path {
                Ok(Some(path))
            } else {
                Ok(None)
            }
        }),
        |result| match result {
            Ok(Ok(Some(path))) => Message::LoadFile(path),
            Ok(Ok(None)) => Message::PdfiumReady,
            Ok(Err(e)) => {
                tracing::error!("{e}");
                Message::PdfiumReady
            }
            Err(e) => {
                tracing::error!("Spawn error: {e}");
                Message::PdfiumReady
            }
        },
    )
}

#[derive(Debug, Clone)]
enum Message {
    OpenFileDialog,
    FileDialogResult(Option<PathBuf>),
    LoadFile(PathBuf),
    PdfiumReady,
    PdfLoaded(Result<Arc<PdfDoc>, String>),
    PageRendered(usize, Arc<crate::engine::renderer::RenderedPage>),
    GoToPage(usize),
    NextPage,
    PrevPage,
    ZoomIn,
    ZoomOut,
    FitWidth,
    ToggleSidebar,
    SearchChanged(String),
    Search,
    SearchNext,
    SearchPrev,
    ClearSearch,
    BookmarkClicked(usize),
    ScrollOffset(f32),
    Tick,
    Event(iced::Event),
}

struct AiPdfApp {
    pdfium_ready: bool,
    document: Option<Arc<PdfDoc>>,
    renderer: Option<PageRenderer>,
    zoom_level: f32,
    zoom_index: usize,
    fit_to_width: bool,
    current_page: usize,
    scroll_offset: f32,
    page_heights: Vec<f32>,
    rendered_textures: Vec<Option<iced_image::Handle>>,
    show_sidebar: bool,
    bookmarks: Vec<BookmarkEntry>,
    search_text: String,
    search_results: Vec<usize>,
    search_index: usize,
    status_message: String,
    file_path: Option<PathBuf>,
    file_name: String,
    file_size: String,
    pdf_version: Option<String>,
}

#[derive(Clone, Debug)]
struct BookmarkEntry {
    title: String,
    page: usize,
    level: usize,
}

impl AiPdfApp {
    fn new() -> Self {
        Self {
            pdfium_ready: false,
            document: None,
            renderer: None,
            zoom_level: 1.0,
            zoom_index: 5,
            fit_to_width: true,
            current_page: 0,
            scroll_offset: 0.0,
            page_heights: Vec::new(),
            rendered_textures: Vec::new(),
            show_sidebar: true,
            bookmarks: Vec::new(),
            search_text: String::new(),
            search_results: Vec::new(),
            search_index: 0,
            status_message: "Initializing PDF engine...".into(),
            file_path: None,
            file_name: String::new(),
            file_size: String::new(),
            pdf_version: None,
        }
    }
}

impl AiPdfApp {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenFileDialog => {
                return Task::perform(open_file_dialog(), Message::FileDialogResult);
            }
            Message::FileDialogResult(Some(path)) => {
                return Task::done(Message::LoadFile(path));
            }
            Message::FileDialogResult(None) => {}

            Message::LoadFile(path) => {
                let fname = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                self.status_message = format!("Loading {fname}...");
                return Task::perform(load_pdf(path), Message::PdfLoaded);
            }

            Message::PdfiumReady => {
                self.pdfium_ready = true;
                self.status_message = "Ready — Open a PDF file (Ctrl+O)".into();
            }

            Message::PdfLoaded(Ok(doc)) => {
                let page_count = doc.page_count();
                self.file_name = doc
                    .path()
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.file_size = format_size(
                    std::fs::metadata(doc.path())
                        .map(|m| m.len())
                        .unwrap_or(0),
                );
                self.file_path = Some(doc.path().to_path_buf());
                self.renderer = Some(PageRenderer::new(Arc::clone(&doc)));
                self.document = Some(doc);
                self.current_page = 0;
                self.scroll_offset = 0.0;
                self.page_heights.clear();
                self.rendered_textures
                    .resize(page_count, None);
                self.load_bookmarks();
                self.load_metadata();
                self.recalculate_page_heights();
                self.status_message =
                    format!("Loaded {} — {} pages", self.file_name, page_count);
                return self.render_visible_task();
            }

            Message::PdfLoaded(Err(e)) => {
                self.status_message = format!("Error: {e}");
            }

            Message::PageRendered(page_idx, bitmap) => {
                let handle = iced_image::Handle::from_rgba(
                    bitmap.width(),
                    bitmap.height(),
                    bitmap.as_raw().to_vec(),
                );
                if page_idx < self.rendered_textures.len() {
                    self.rendered_textures[page_idx] = Some(handle);
                }
            }

            Message::GoToPage(page) => {
                if let Some(doc) = &self.document {
                    let max = doc.page_count().saturating_sub(1);
                    self.current_page = page.min(max);
                    self.scroll_offset = self.calculate_page_offset(self.current_page);
                    return self.render_visible_task();
                }
            }
            Message::NextPage => {
                if let Some(doc) = &self.document {
                    let max = doc.page_count().saturating_sub(1);
                    if self.current_page < max {
                        self.current_page += 1;
                        self.scroll_offset =
                            self.calculate_page_offset(self.current_page);
                        return self.render_visible_task();
                    }
                }
            }
            Message::PrevPage => {
                if self.current_page > 0 {
                    self.current_page -= 1;
                    self.scroll_offset =
                        self.calculate_page_offset(self.current_page);
                    return self.render_visible_task();
                }
            }
            Message::ZoomIn => {
                if self.fit_to_width {
                    self.fit_to_width = false;
                    self.zoom_level = 1.0;
                    self.zoom_index = 5;
                } else if self.zoom_index + 1 < ZOOM_LEVELS.len() {
                    self.zoom_index += 1;
                    self.zoom_level = ZOOM_LEVELS[self.zoom_index];
                }
                return self.on_zoom_changed();
            }
            Message::ZoomOut => {
                if self.fit_to_width {
                    self.fit_to_width = false;
                    self.zoom_level = 1.0;
                    self.zoom_index = 5;
                } else if self.zoom_index > 0 {
                    self.zoom_index -= 1;
                    self.zoom_level = ZOOM_LEVELS[self.zoom_index];
                }
                return self.on_zoom_changed();
            }
            Message::FitWidth => {
                self.fit_to_width = !self.fit_to_width;
                if !self.fit_to_width {
                    self.zoom_level = 1.0;
                    self.zoom_index = 5;
                }
                return self.on_zoom_changed();
            }
            Message::ToggleSidebar => {
                self.show_sidebar = !self.show_sidebar;
            }
            Message::SearchChanged(query) => {
                self.search_text = query;
            }
            Message::Search => {
                self.perform_search();
                if !self.search_results.is_empty() {
                    self.search_index = 0;
                    return Task::done(Message::GoToPage(self.search_results[0]));
                }
            }
            Message::SearchNext => {
                if !self.search_results.is_empty() {
                    self.search_index =
                        (self.search_index + 1) % self.search_results.len();
                    return Task::done(Message::GoToPage(
                        self.search_results[self.search_index],
                    ));
                }
            }
            Message::SearchPrev => {
                if !self.search_results.is_empty() {
                    self.search_index = if self.search_index == 0 {
                        self.search_results.len() - 1
                    } else {
                        self.search_index - 1
                    };
                    return Task::done(Message::GoToPage(
                        self.search_results[self.search_index],
                    ));
                }
            }
            Message::ClearSearch => {
                self.search_text.clear();
                self.search_results.clear();
                self.search_index = 0;
            }
            Message::BookmarkClicked(page) => {
                return Task::done(Message::GoToPage(page));
            }
            Message::ScrollOffset(offset) => {
                self.scroll_offset = offset;
                let visible = self.visible_pages();
                if !visible.is_empty() {
                    let mid = visible[visible.len() / 2];
                    self.current_page = mid;
                }
                return self.render_visible_task();
            }
            Message::Tick => {
                return self.render_visible_task();
            }
            Message::Event(event) => {
                if let iced::Event::Keyboard(
                    iced::keyboard::Event::KeyPressed { key, modifiers, .. },
                ) = event
                {
                    return self.handle_key(key, modifiers);
                }
            }
        }
        Task::none()
    }

    fn handle_key(&mut self, key: Key, modifiers: Modifiers) -> Task<Message> {
        match &key {
            Key::Named(key::Named::ArrowRight) if modifiers.alt() => {
                return Task::done(Message::NextPage);
            }
            Key::Named(key::Named::ArrowLeft) if modifiers.alt() => {
                return Task::done(Message::PrevPage);
            }
            _ => {}
        }

        if key == Key::Named(key::Named::Escape) {
            return Task::done(Message::ClearSearch);
        }

        if let Key::Character(c) = &key {
            match (c.as_str(), modifiers) {
                ("o" | "O", m) if m.control() => {
                    return Task::done(Message::OpenFileDialog);
                }
                ("0", m) if m.control() => {
                    return Task::done(Message::FitWidth);
                }
                ("=" | "+", m) if m.control() => {
                    return Task::done(Message::ZoomIn);
                }
                ("-", m) if m.control() => {
                    return Task::done(Message::ZoomOut);
                }
                _ => {}
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<Message> {
        let content: Element<_> = if self.document.is_some() {
            self.pdf_view()
        } else {
            self.welcome_view()
        };

        let status = self.status_bar();
        let toolbar = self.toolbar();

        let main_row: Element<_> = if self.show_sidebar {
            let sidebar = self.sidebar_view();
            row![
                container(sidebar).width(280),
                container(content).width(Fill),
            ]
            .into()
        } else {
            container(content).width(Fill).into()
        };

        column![
            toolbar,
            horizontal_rule(1),
            container(main_row).height(Fill),
            horizontal_rule(1),
            status,
        ]
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let events = iced::event::listen().map(Message::Event);
        let tick = iced::time::every(Duration::from_millis(200)).map(|_| Message::Tick);
        Subscription::batch(vec![events, tick])
    }

    // ── UI ──

    fn toolbar(&self) -> Element<Message> {
        let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
        let current = if page_count > 0 {
            format!("{}", self.current_page + 1)
        } else {
            "0".into()
        };
        let zoom_label = self.zoom_label();
        let search_info = if !self.search_results.is_empty() {
            format!("{}/{}", self.search_index + 1, self.search_results.len())
        } else {
            String::new()
        };

        let mut children: Vec<Element<Message>> = vec![
            button(text("Open").size(13))
                .on_press(Message::OpenFileDialog)
                .padding(4)
                .into(),
            horizontal_space().width(4).into(),
            button(text("\u{25C0}").size(13))
                .on_press(Message::PrevPage)
                .padding(4)
                .into(),
            text(current)
                .size(13)
                .width(Length::Fixed(40.0))
                .center()
                .into(),
            text(format!("/ {page_count}")).size(13).into(),
            button(text("\u{25B6}").size(13))
                .on_press(Message::NextPage)
                .padding(4)
                .into(),
            horizontal_space().width(4).into(),
            text("|").size(13).into(),
            button(text("\u{2212}").size(13))
                .on_press(Message::ZoomOut)
                .padding(4)
                .into(),
            text(zoom_label)
                .size(12)
                .width(Length::Fixed(60.0))
                .center()
                .into(),
            button(text("+").size(13))
                .on_press(Message::ZoomIn)
                .padding(4)
                .into(),
            button(text("\u{229E}").size(13))
                .on_press(Message::FitWidth)
                .padding(4)
                .into(),
            horizontal_space().width(8).into(),
            text("|").size(13).into(),
            horizontal_space().width(4).into(),
            text_input("Search...", &self.search_text)
                .on_input(Message::SearchChanged)
                .on_submit(Message::Search)
                .size(13)
                .width(200)
                .into(),
        ];

        if !search_info.is_empty() {
            children.push(text(search_info).size(11).into());
        }

        if !self.search_text.is_empty() {
            children.push(
                row![
                    button(text("\u{2193}").size(13))
                        .on_press(Message::SearchNext)
                        .padding(2),
                    button(text("\u{2191}").size(13))
                        .on_press(Message::SearchPrev)
                        .padding(2),
                    button(text("\u{2715}").size(13))
                        .on_press(Message::ClearSearch)
                        .padding(2),
                ]
                .into(),
            );
        }

        children.push(horizontal_space().into());
        children.push(
            button(text("\u{2630}").size(13))
                .on_press(Message::ToggleSidebar)
                .padding(4)
                .into(),
        );

        row(children)
            .align_y(Alignment::Center)
            .padding([2, 8])
            .spacing(4)
            .into()
    }

    fn sidebar_view(&self) -> Element<Message> {
        let bookmarks: Element<_> = if self.bookmarks.is_empty() {
            text("No bookmarks").size(12).style(text::secondary).into()
        } else {
            let items: Vec<Element<_>> = self
                .bookmarks
                .iter()
                .map(|bm| {
                    let indent = 4 + bm.level as u16 * 12;
                    button(text(&bm.title).size(12))
                        .on_press(Message::BookmarkClicked(bm.page))
                        .padding([1, indent])
                        .into()
                })
                .collect();
            column(items).spacing(2).into()
        };

        let info = column![
            text("Document Info").size(14),
            horizontal_rule(1),
            text(format!("File: {}", self.file_name)).size(12),
            text(format!("Size: {}", self.file_size)).size(12),
            if let Some(doc) = &self.document {
                text(format!("Pages: {}", doc.page_count())).size(12)
            } else {
                text("").size(12)
            },
            if let Some(ver) = &self.pdf_version {
                text(format!("PDF Version: {ver}")).size(12)
            } else {
                text("").size(12)
            },
        ]
        .spacing(4);

        scrollable(
            column![
                text("Bookmarks").size(14),
                horizontal_rule(1),
                bookmarks,
                vertical_space().height(16),
                info,
            ]
            .spacing(4)
            .padding(8),
        )
        .into()
    }

    fn pdf_view(&self) -> Element<Message> {
        if self.page_heights.is_empty() || self.document.is_none() {
            return container(text("Loading...").size(16).center().width(Fill).height(Fill))
                .center_x(Fill)
                .center_y(Fill)
                .into();
        }

        let page_count = self.document.as_ref().unwrap().page_count();
        let spacing = 8.0;
        let overscan = 600.0;

        let total_height: f32 = if page_count > 0 {
            self.page_heights.iter().sum::<f32>()
                + (page_count.saturating_sub(1)) as f32 * spacing
        } else {
            0.0
        };

        let visible_range = self.visible_page_range(overscan);
        let range_start = visible_range.start;
        let range_end = visible_range.end;
        let mut children: Vec<Element<Message>> = Vec::new();

        let top_offset = self.page_offset_up_to(range_start);
        if top_offset > 0.0 {
            children.push(vertical_space().height(top_offset).into());
        }

        for page_idx in range_start..range_end {
            let page_height = self.page_heights[page_idx];

            if let Some(handle) = &self.rendered_textures[page_idx] {
                let img = iced_image(handle.clone())
                    .width(Fill)
                    .height(Length::Fixed(page_height));
                children.push(img.into());
            } else {
                let placeholder = container(
                    text(format!("Page {} — Rendering...", page_idx + 1))
                        .size(14)
                        .center(),
                )
                .width(Fill)
                .height(Length::Fixed(page_height))
                .style(container::dark);
                children.push(placeholder.into());
            }

            if page_idx + 1 < page_count {
                children.push(vertical_space().height(spacing).into());
            }
        }

        let bottom_offset = total_height - self.page_offset_up_to(range_end);
        if bottom_offset > 0.0 {
            children.push(vertical_space().height(bottom_offset).into());
        }

        scrollable(column(children))
            .id(iced::widget::scrollable::Id::new("pdf_scroll"))
            .on_scroll(move |viewport| Message::ScrollOffset(viewport.absolute_offset().y))
            .into()
    }

    fn welcome_view(&self) -> Element<Message> {
        container(
            column![
                text("AI-PDF Reader").size(32).style(text::secondary),
                vertical_space().height(20),
                if self.pdfium_ready {
                    text("Open a PDF file to get started").size(16)
                } else {
                    text("Initializing PDF engine...").size(16)
                },
                vertical_space().height(10),
                if self.pdfium_ready {
                    text("Ctrl+O or drag & drop a PDF file")
                        .size(13)
                        .style(text::secondary)
                } else {
                    text("Downloading PDF rendering engine (first run only)")
                        .size(13)
                        .style(text::secondary)
                },
            ]
            .align_x(Alignment::Center),
        )
        .center_x(Fill)
        .center_y(Fill)
        .into()
    }

    fn status_bar(&self) -> Element<Message> {
        row![
            text(&self.status_message).size(11).style(text::secondary),
            horizontal_space(),
            if let Some(doc) = &self.document {
                text(format!(
                    "Page {} / {}  |  {}",
                    self.current_page + 1,
                    doc.page_count(),
                    self.zoom_label()
                ))
                .size(11)
                .style(text::secondary)
            } else {
                text("").size(11)
            },
        ]
        .padding([2, 8])
        .into()
    }

    // ── Helpers ──

    fn effective_zoom(&self) -> f32 {
        self.zoom_level
    }

    fn zoom_label(&self) -> String {
        if self.fit_to_width {
            "Fit Width".into()
        } else {
            format!("{:.0}%", self.zoom_level * 100.0)
        }
    }

    fn on_zoom_changed(&mut self) -> Task<Message> {
        self.rendered_textures.clear();
        if let Some(renderer) = &self.renderer {
            renderer.invalidate_zoom();
        }
        if let Some(doc) = &self.document {
            self.rendered_textures.resize(doc.page_count(), None);
        }
        self.recalculate_page_heights();
        self.scroll_offset = self.calculate_page_offset(self.current_page);
        self.render_visible_task()
    }

    fn recalculate_page_heights(&mut self) {
        if let Some(renderer) = &self.renderer {
            let page_count = renderer.page_count();
            self.page_heights = Vec::with_capacity(page_count);
            let zoom = self.effective_zoom();
            for i in 0..page_count {
                let (_, h) = renderer.page_render_size(i, zoom).unwrap_or((1, 100));
                self.page_heights.push(h as f32);
            }
        }
    }

    fn page_offset_up_to(&self, page: usize) -> f32 {
        let spacing = 8.0;
        let end = page.min(self.page_heights.len());
        if end == 0 {
            return 0.0;
        }
        self.page_heights[..end].iter().sum::<f32>()
            + end.saturating_sub(1) as f32 * spacing
    }

    fn calculate_page_offset(&self, page: usize) -> f32 {
        self.page_offset_up_to(page)
    }

    fn visible_page_range(&self, overscan: f32) -> std::ops::Range<usize> {
        if self.page_heights.is_empty() {
            return 0..0;
        }

        let viewport_height = 800.0;
        let scroll = self.scroll_offset;
        let view_start = (scroll - overscan).max(0.0);
        let view_end = scroll + viewport_height + overscan;
        let spacing = 8.0;

        let mut start = 0;
        let mut cumulative = 0.0f32;
        for (i, h) in self.page_heights.iter().enumerate() {
            let page_end = cumulative + h;
            if page_end >= view_start {
                start = i;
                break;
            }
            cumulative = page_end + spacing;
            start = i + 1;
        }

        let mut end = start;
        cumulative = self.page_offset_up_to(start);
        for i in start..self.page_heights.len() {
            if cumulative >= view_end {
                break;
            }
            cumulative += self.page_heights[i] + spacing;
            end = i + 1;
        }

        start..end
    }

    fn visible_pages(&self) -> Vec<usize> {
        self.visible_page_range(600.0).collect()
    }

    fn render_visible_task(&mut self) -> Task<Message> {
        if self.renderer.is_none() || self.document.is_none() {
            return Task::none();
        }

        let zoom = self.effective_zoom();
        let visible = self.visible_page_range(800.0);
        let mut tasks = Vec::new();

        for page_idx in visible {
            if page_idx < self.rendered_textures.len()
                && self.rendered_textures[page_idx].is_none()
            {
                let renderer = self.renderer.as_ref().unwrap();
                match renderer.get_or_render_sync(page_idx, zoom) {
                    Ok(bitmap) => {
                        tasks.push(Task::done(Message::PageRendered(page_idx, bitmap)));
                    }
                    Err(e) => {
                        tracing::error!("Render page {page_idx} failed: {e}");
                    }
                }
            }
        }

        Task::batch(tasks)
    }

    fn perform_search(&mut self) {
        self.search_results.clear();
        self.search_index = 0;

        if self.search_text.is_empty() || self.document.is_none() {
            self.status_message = String::new();
            return;
        }

        let regex = match regex::Regex::new(&regex::escape(&self.search_text)) {
            Ok(r) => r,
            Err(e) => {
                self.status_message = format!("Invalid search: {e}");
                return;
            }
        };

        let doc = self.document.as_ref().unwrap();
        for i in 0..doc.page_count() {
            if let Ok(text) =
                doc.with_document(|d| PdfDoc::extract_page_text(doc.pdfium, d, i))
            {
                if let Ok(text) = text {
                    if regex.is_match(&text) {
                        self.search_results.push(i);
                    }
                }
            }
        }

        self.status_message = if self.search_results.is_empty() {
            format!("No results for \"{}\"", self.search_text)
        } else {
            format!(
                "{} results for \"{}\"",
                self.search_results.len(),
                self.search_text
            )
        };
    }

    fn load_bookmarks(&mut self) {
        self.bookmarks.clear();
        let path = match &self.file_path {
            Some(p) => p,
            None => return,
        };

        match lopdf::Document::load(path) {
            Ok(doc) => match doc.get_toc() {
                Ok(toc) => {
                    for entry in toc.toc {
                        self.bookmarks.push(BookmarkEntry {
                            title: entry.title,
                            page: entry.page.saturating_sub(1),
                            level: entry.level,
                        });
                    }
                }
                Err(e) => {
                    tracing::debug!("No bookmarks: {e}");
                }
            },
            Err(e) => {
                tracing::warn!("Could not read bookmarks: {e}");
            }
        }
    }

    fn load_metadata(&mut self) {
        let path = match &self.file_path {
            Some(p) => p,
            None => return,
        };
        if let Ok(lopdf_doc) = lopdf::Document::load(path) {
            self.pdf_version = Some(lopdf_doc.version.clone());
        }
    }
}

async fn load_pdf(path: PathBuf) -> Result<Arc<PdfDoc>, String> {
    tokio::task::spawn_blocking(move || PdfDoc::open(&path).map(Arc::new).map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("Task error: {e}"))?
}

async fn open_file_dialog() -> Option<PathBuf> {
    tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .add_filter("PDF Files", &["pdf"])
            .pick_file()
    })
    .await
    .ok()
    .flatten()
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx + 1 < UNITS.len() {
        size /= 1024.0;
        unit_idx += 1;
    }
    format!("{size:.1} {}", UNITS[unit_idx])
}
