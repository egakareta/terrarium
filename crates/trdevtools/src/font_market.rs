use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    io::Write,
    sync::{Arc, mpsc, mpsc::Receiver},
    time::Duration,
};

use egui::{FontFamily, FontId, RichText};
use serde::Deserialize;

const FONTS_API: &str = "https://api.fontsource.org/v1/fonts";
const PREVIEW_SENTENCE: &str = "Sphinx of black quartz, judge my vow.";

type HttpResult = Result<ehttp::Response, String>;
type SaveResult = Result<Option<String>, String>;

#[derive(Clone, Debug, Deserialize)]
struct FontRecord {
    id: String,
    family: String,
    #[serde(default)]
    subsets: Vec<String>,
    #[serde(default)]
    weights: Vec<u16>,
    #[serde(default)]
    styles: Vec<String>,
    #[serde(default, rename = "defSubset")]
    default_subset: String,
    #[serde(default)]
    variable: bool,
    #[serde(default)]
    category: String,
    #[serde(default)]
    version: String,
    #[serde(default, rename = "type")]
    source_type: String,
}

#[derive(Debug, Deserialize)]
struct FontDetails {
    #[serde(flatten)]
    record: FontRecord,
    #[serde(default)]
    variants: BTreeMap<String, BTreeMap<String, BTreeMap<String, FontFiles>>>,
}

#[derive(Debug, Deserialize)]
struct FontFiles {
    #[serde(default)]
    url: BTreeMap<String, String>,
}

impl FontDetails {
    fn ttf_files(&self, include_all_subsets: bool) -> Vec<(String, String)> {
        let mut files = Vec::new();
        let mut seen_urls = HashSet::new();
        let mut seen_names = HashSet::new();
        let has_default_subset = self.variants.values().any(|styles| {
            styles
                .values()
                .any(|subsets| subsets.contains_key(&self.record.default_subset))
        });

        for (weight, styles) in &self.variants {
            for (style, subsets) in styles {
                for (subset, font_files) in subsets {
                    if !include_all_subsets
                        && has_default_subset
                        && subset != &self.record.default_subset
                    {
                        continue;
                    }
                    let Some(url) = font_files.url.get("ttf") else {
                        continue;
                    };
                    if !seen_urls.insert(url.clone()) {
                        continue;
                    }

                    let stem = format!(
                        "{}-{}-{}-{}",
                        font_file_component(&self.record.id),
                        font_file_component(weight),
                        font_file_component(style),
                        font_file_component(subset)
                    );
                    let mut name = format!("{stem}.ttf");
                    let mut suffix = 2;
                    while !seen_names.insert(name.clone()) {
                        name = format!("{stem}-{suffix}.ttf");
                        suffix += 1;
                    }
                    files.push((name, url.clone()));
                }
            }
        }

        files
    }

    fn ttf_url(&self) -> Option<&str> {
        self.ttf_url_for_weight(400)
    }

    fn ttf_url_for_weight(&self, weight: i32) -> Option<&str> {
        let variable_variant = self
            .variants
            .iter()
            .find(|(key, _)| key.as_str() == "variable" || key.contains(".."))
            .map(|(_, value)| value);

        if self.record.variable
            && let Some(url) =
                variable_variant.and_then(|variant| self.ttf_url_for_variant(variant))
        {
            return Some(url);
        }

        let requested_weight = u16::try_from(weight).unwrap_or(400);
        let variant = self
            .variants
            .get(&requested_weight.to_string())
            .or_else(|| {
                self.variants
                    .iter()
                    .filter_map(|(key, variant)| {
                        key.parse::<u16>()
                            .ok()
                            .map(|available| (available, variant))
                    })
                    .min_by_key(|(available, _)| available.abs_diff(requested_weight))
                    .map(|(_, variant)| variant)
            })
            .or_else(|| self.variants.get("400"))
            .or_else(|| self.variants.values().next())?;
        self.ttf_url_for_variant(variant)
    }

    fn ttf_url_for_variant<'a>(
        &self,
        variant: &'a BTreeMap<String, BTreeMap<String, FontFiles>>,
    ) -> Option<&'a str> {
        let style = variant.get("normal").or_else(|| variant.values().next())?;
        let subset = style
            .get(&self.record.default_subset)
            .or_else(|| style.get("latin"))
            .or_else(|| style.values().next())?;
        subset.url.get("ttf").map(String::as_str)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum DetailTab {
    #[default]
    Preview,
    Glyphs,
    About,
    GetFont,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum SortOrder {
    #[default]
    Alphabetical,
    VariableFirst,
}

#[derive(Clone, Debug)]
struct FontAsset {
    bytes: Vec<u8>,
    weight_axis: bool,
}

struct FontFileRequest {
    name: String,
    receiver: Receiver<HttpResult>,
}

struct FontBundleDownload {
    font_id: String,
    total: usize,
    queued: VecDeque<(String, String)>,
    pending: Vec<FontFileRequest>,
    files: Vec<(String, Vec<u8>)>,
}

/// Fontsource-backed font browser and runtime font installer.
pub(crate) struct FontMarket {
    fonts: Vec<FontRecord>,
    catalog_requested: bool,
    catalog_request: Option<Receiver<HttpResult>>,
    selected: Option<FontRecord>,
    details: Option<FontDetails>,
    detail_request: Option<Receiver<HttpResult>>,
    detail_error: Option<String>,
    download_request: Option<Receiver<HttpResult>>,
    download_for: Option<String>,
    download_error: Option<String>,
    weight_preview_request: Option<Receiver<HttpResult>>,
    weight_preview_for: Option<(String, i32)>,
    bundle_download: Option<FontBundleDownload>,
    save_request: Option<Receiver<SaveResult>>,
    include_all_font_subsets: bool,
    downloaded_fonts: HashMap<String, FontAsset>,
    preview_fonts: HashMap<String, FontAsset>,
    preview_requested: HashSet<String>,
    preview_queue: VecDeque<String>,
    preview_detail_requests: HashMap<String, Receiver<HttpResult>>,
    preview_font_requests: HashMap<String, Receiver<HttpResult>>,
    preview_fonts_dirty: bool,
    active_font: Option<String>,
    active_font_weight: i32,
    search: String,
    category: Option<String>,
    language: Option<String>,
    variable_only: bool,
    sort_order: SortOrder,
    preview_text: String,
    preview_size: f32,
    preview_weight: i32,
    detail_tab: DetailTab,
    message: Option<String>,
}

impl Default for FontMarket {
    fn default() -> Self {
        Self {
            fonts: Vec::new(),
            catalog_requested: false,
            catalog_request: None,
            selected: None,
            details: None,
            detail_request: None,
            detail_error: None,
            download_request: None,
            download_for: None,
            download_error: None,
            weight_preview_request: None,
            weight_preview_for: None,
            bundle_download: None,
            save_request: None,
            include_all_font_subsets: false,
            downloaded_fonts: HashMap::new(),
            preview_fonts: HashMap::new(),
            preview_requested: HashSet::new(),
            preview_queue: VecDeque::new(),
            preview_detail_requests: HashMap::new(),
            preview_font_requests: HashMap::new(),
            preview_fonts_dirty: false,
            active_font: None,
            active_font_weight: 400,
            search: String::new(),
            category: None,
            language: None,
            variable_only: false,
            sort_order: SortOrder::default(),
            preview_text: PREVIEW_SENTENCE.to_owned(),
            preview_size: 32.0,
            preview_weight: 400,
            detail_tab: DetailTab::default(),
            message: None,
        }
    }
}

impl FontMarket {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        context: &egui::Context,
        base_font_definitions: Option<&egui::FontDefinitions>,
    ) {
        if !self.catalog_requested {
            self.request_catalog();
        }
        self.poll_requests(context, base_font_definitions);

        if self.selected.is_some() {
            self.show_details(ui, context, base_font_definitions);
        } else {
            self.show_browser(ui, context);
        }
        self.pump_preview_requests();
        if self.has_pending_requests() {
            context.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn request_catalog(&mut self) {
        self.catalog_requested = true;
        self.catalog_request = Some(fetch(FONTS_API));
    }

    fn open_font(&mut self, font: FontRecord) {
        self.details = None;
        self.detail_error = None;
        self.download_error = None;
        self.message = None;
        self.detail_tab = DetailTab::Preview;
        self.preview_weight = 400;
        self.include_all_font_subsets = false;
        self.detail_request = Some(fetch(&format!("{FONTS_API}/{}", font.id)));
        self.selected = Some(font);
    }

    fn poll_requests(
        &mut self,
        context: &egui::Context,
        base_font_definitions: Option<&egui::FontDefinitions>,
    ) {
        if let Some(result) = poll(&mut self.catalog_request) {
            match result.and_then(parse_json::<Vec<FontRecord>>) {
                Ok(mut fonts) => {
                    fonts.sort_by_cached_key(|font| font.family.to_lowercase());
                    self.fonts = fonts;
                    self.message = None;
                }
                Err(error) => self.message = Some(error),
            }
        }

        if let Some(result) = poll(&mut self.detail_request) {
            match result.and_then(parse_json::<FontDetails>) {
                Ok(details) => {
                    if self
                        .selected
                        .as_ref()
                        .is_some_and(|font| font.id == details.record.id)
                    {
                        self.details = Some(details);
                    }
                }
                Err(error) => self.detail_error = Some(error),
            }
        }

        if let Some(result) = poll(&mut self.download_request) {
            let download_for = self.download_for.take();
            match result.and_then(response_bytes) {
                Ok(bytes) if is_supported_font(&bytes) => {
                    if let Some(font) = download_for
                        .as_deref()
                        .and_then(|id| self.fonts.iter().find(|font| font.id == id).cloned())
                    {
                        let weight_axis = has_weight_axis(&bytes);
                        let asset = FontAsset { bytes, weight_axis };
                        self.downloaded_fonts.insert(font.id.clone(), asset);
                        self.download_error = None;
                        self.apply_font(&font.id, context, base_font_definitions);
                    }
                }
                Ok(_) if download_for.is_some() => {
                    self.download_error = Some(
                        "Fontsource returned an unsupported font file; a TrueType font is required."
                            .to_owned(),
                    );
                }
                Err(error) if download_for.is_some() => self.download_error = Some(error),
                Ok(_) | Err(_) => {}
            }
        }

        self.poll_bundle_download(context);

        if let Some(result) = poll(&mut self.weight_preview_request) {
            let weight_preview_for = self.weight_preview_for.take();
            if let Some((id, weight)) = weight_preview_for
                && let Ok(bytes) = result.and_then(response_bytes)
                && is_supported_font(&bytes)
                && self.preview_weight == weight
                && self.selected.as_ref().is_some_and(|font| font.id == id)
            {
                let asset = FontAsset {
                    weight_axis: has_weight_axis(&bytes),
                    bytes,
                };
                if self.active_font.as_deref() == Some(id.as_str())
                    && self.downloaded_fonts.contains_key(&id)
                {
                    self.downloaded_fonts.insert(id.clone(), asset);
                    self.apply_font(&id, context, base_font_definitions);
                } else {
                    self.preview_fonts.insert(id, asset);
                    self.refresh_preview_fonts(context, base_font_definitions);
                }
            }
        }

        if let Some(result) = poll_save_request(&mut self.save_request) {
            match result {
                Ok(Some(message)) => self.message = Some(message),
                Ok(None) => self.message = None,
                Err(error) => self.message = Some(error),
            }
        }

        let detail_ids: Vec<_> = self.preview_detail_requests.keys().cloned().collect();
        for id in detail_ids {
            let response = self
                .preview_detail_requests
                .get(&id)
                .and_then(try_poll_receiver);
            if let Some(response) = response {
                self.preview_detail_requests.remove(&id);
                if let Ok(details) = response.and_then(parse_json::<FontDetails>)
                    && let Some(url) = details.ttf_url()
                {
                    self.preview_font_requests.insert(id, fetch(url));
                }
            }
        }

        let font_ids: Vec<_> = self.preview_font_requests.keys().cloned().collect();
        for id in font_ids {
            let response = self
                .preview_font_requests
                .get(&id)
                .and_then(try_poll_receiver);
            if let Some(response) = response {
                self.preview_font_requests.remove(&id);
                if let Ok(bytes) = response.and_then(response_bytes)
                    && is_supported_font(&bytes)
                    && self.fonts.iter().any(|font| font.id == id)
                    && !(self
                        .selected
                        .as_ref()
                        .is_some_and(|selected| selected.id == id)
                        && self.preview_weight != 400)
                {
                    let weight_axis = has_weight_axis(&bytes);
                    self.preview_fonts
                        .insert(id, FontAsset { bytes, weight_axis });
                    self.preview_fonts_dirty = true;
                }
            }
        }

        if self.preview_fonts_dirty
            && self.preview_queue.is_empty()
            && self.preview_detail_requests.is_empty()
            && self.preview_font_requests.is_empty()
        {
            self.refresh_preview_fonts(context, base_font_definitions);
            self.preview_fonts_dirty = false;
        }
    }

    fn show_browser(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        ui.horizontal(|ui| {
            ui.add_sized(
                [ui.available_width().min(260.0), ui.available_height()],
                egui::TextEdit::singleline(&mut self.search).hint_text("Search fonts"),
            );
            ui.separator();
            ui.add_sized(
                [ui.available_width().min(160.0), ui.available_height()],
                egui::TextEdit::singleline(&mut self.preview_text).hint_text("Enter preview text"),
            );
            ui.add(
                egui::DragValue::new(&mut self.preview_size)
                    .range(16.0..=48.0)
                    .fixed_decimals(0)
                    .suffix(" px"),
            );
        });

        ui.separator();
        let categories = unique_values(self.fonts.iter().map(|font| font.category.as_str()));
        let languages = unique_values(
            self.fonts
                .iter()
                .flat_map(|font| font.subsets.iter().map(String::as_str)),
        );
        ui.horizontal_wrapped(|ui| {
            option_filter(
                ui,
                "All categories",
                "font_market_category",
                &categories,
                &mut self.category,
            );
            option_filter(
                ui,
                "All languages",
                "font_market_language",
                &languages,
                &mut self.language,
            );
            ui.checkbox(&mut self.variable_only, "Variable fonts");
            if ui
                .button(format!(
                    "{} Clear filters",
                    tregui::egui_phosphor::regular::TRASH
                ))
                .clicked()
            {
                self.search.clear();
                self.category = None;
                self.language = None;
                self.variable_only = false;
            }
        });

        if let Some(message) = self.message.as_deref() {
            ui.colored_label(ui.visuals().error_fg_color, message);
            if self.fonts.is_empty() && ui.button("Retry").clicked() {
                self.message = None;
                self.catalog_requested = false;
                self.request_catalog();
            }
        }
        if self.catalog_request.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading fonts from Fontsource…");
            });
        }
        if self.fonts.is_empty() {
            return;
        }

        let needle = self.search.trim().to_lowercase();
        let mut fonts: Vec<_> = self
            .fonts
            .iter()
            .filter(|font| {
                (needle.is_empty() || font.family.to_lowercase().contains(&needle))
                    && self
                        .category
                        .as_ref()
                        .is_none_or(|value| &font.category == value)
                    && self
                        .language
                        .as_ref()
                        .is_none_or(|value| font.subsets.contains(value))
                    && (!self.variable_only || font.variable)
            })
            .collect();
        match self.sort_order {
            SortOrder::Alphabetical => fonts.sort_by_cached_key(|font| font.family.to_lowercase()),
            SortOrder::VariableFirst => {
                fonts.sort_by_cached_key(|font| (!font.variable, font.family.to_lowercase()));
            }
        }

        ui.horizontal(|ui| {
            ui.label(format!("{} font families", fonts.len()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("font_market_sort")
                    .selected_text(match self.sort_order {
                        SortOrder::Alphabetical => "Name A–Z",
                        SortOrder::VariableFirst => "Variable first",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.sort_order,
                            SortOrder::Alphabetical,
                            "Name A–Z",
                        );
                        ui.selectable_value(
                            &mut self.sort_order,
                            SortOrder::VariableFirst,
                            "Variable first",
                        );
                    });
            });
        });

        let mut chosen = None;
        let mut visible_ids = Vec::new();
        let available_width = ui.available_width();
        let column_spacing = ui.spacing().item_spacing.x;
        let columns = (((available_width + column_spacing) / (225.0 + column_spacing)).floor()
            as usize)
            .clamp(1, 5);
        let card_height = 174.0;
        let row_count = fonts.len().div_ceil(columns);
        egui::ScrollArea::vertical()
            .id_salt("font_market_results")
            .auto_shrink([false, false])
            .show_rows(ui, card_height, row_count, |ui, row_range| {
                let column_spacing = ui.spacing().item_spacing.x;
                let card_width = (ui.available_width()
                    - column_spacing * (columns.saturating_sub(1) as f32))
                    / columns as f32;
                for row_index in row_range {
                    ui.horizontal(|ui| {
                        let start = row_index * columns;
                        let end = (start + columns).min(fonts.len());
                        for font in &fonts[start..end] {
                            let id = font.id.clone();
                            let displayed_name = font.family.clone();
                            let variable = font.variable;
                            let family = if font_family_is_available(context, &font.id) {
                                font_family(&font.id)
                            } else {
                                FontFamily::Proportional
                            };
                            let (cell_rect, _) = ui.allocate_exact_size(
                                egui::vec2(card_width, card_height),
                                egui::Sense::hover(),
                            );
                            let frame = egui::Frame::group(ui.style());
                            let frame = if ui.rect_contains_pointer(cell_rect) {
                                let hovered = &ui.visuals().widgets.hovered;
                                frame.fill(hovered.bg_fill).stroke(hovered.bg_stroke)
                            } else {
                                frame
                            };
                            let content_rect = cell_rect.shrink2(frame.total_margin().sum() * 0.5);
                            ui.painter().add(frame.paint(content_rect));
                            let mut card_ui = ui.new_child(
                                egui::UiBuilder::new()
                                    .id_salt(("font_market_card_content", &id))
                                    .max_rect(content_rect)
                                    .layout(egui::Layout::top_down(egui::Align::Min)),
                            );
                            card_ui.shrink_clip_rect(content_rect);
                            card_ui.allocate_ui_with_layout(
                                egui::vec2(content_rect.width(), 128.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(if self.preview_text.is_empty() {
                                                PREVIEW_SENTENCE
                                            } else {
                                                &self.preview_text
                                            })
                                            .font(FontId::new(self.preview_size.min(34.0), family)),
                                        )
                                        .wrap(),
                                    );
                                },
                            );
                            card_ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                                ui.horizontal(|ui| {
                                    if variable {
                                        ui.small("Variable");
                                    }
                                    ui.add(
                                        egui::Label::new(RichText::new(displayed_name).strong())
                                            .wrap(),
                                    );
                                });
                            });
                            let response = ui.interact(
                                cell_rect,
                                ui.id().with(("font_market_card", &id)),
                                egui::Sense::click(),
                            );
                            if response.clicked() {
                                chosen = Some((*font).clone());
                            }
                            if ui.is_rect_visible(cell_rect) {
                                visible_ids.push(id);
                            }
                            response.on_hover_cursor(egui::CursorIcon::PointingHand);
                        }
                    });
                }
            });
        for id in visible_ids {
            if self.preview_requested.insert(id.clone()) {
                self.preview_queue.push_back(id);
            }
        }
        if let Some(font) = chosen {
            self.open_font(font);
        }
    }

    fn show_details(
        &mut self,
        ui: &mut egui::Ui,
        context: &egui::Context,
        base_font_definitions: Option<&egui::FontDefinitions>,
    ) {
        let Some(font) = self.selected.clone() else {
            return;
        };

        ui.horizontal(|ui| {
            if ui.button("← Browse fonts").clicked() {
                self.selected = None;
                return;
            }
            ui.separator();
            ui.vertical(|ui| {
                ui.heading(&font.family);
                ui.small(format!(
                    "{}  ·  {}  ·  {} weight{}",
                    display_category(&font.category),
                    if font.variable { "Variable" } else { "Static" },
                    font.weights.len(),
                    if font.weights.len() == 1 { "" } else { "s" }
                ));
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            for (tab, label) in [
                (DetailTab::Preview, "Preview"),
                (DetailTab::Glyphs, "Glyphs"),
                (DetailTab::About, "About"),
                (DetailTab::GetFont, "Get font"),
            ] {
                if ui.selectable_label(self.detail_tab == tab, label).clicked() {
                    self.detail_tab = tab;
                }
            }
        });
        ui.separator();

        if let Some(error) = self.detail_error.as_deref() {
            ui.colored_label(ui.visuals().error_fg_color, error);
            return;
        }
        if self.detail_request.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading font details…");
            });
            return;
        }

        match self.detail_tab {
            DetailTab::Preview => self.show_preview(ui, &font, context, base_font_definitions),
            DetailTab::Glyphs => self.show_glyphs(ui, &font, context),
            DetailTab::About => self.show_about(ui, &font),
            DetailTab::GetFont => self.show_get_font(ui, &font, context, base_font_definitions),
        }
    }

    fn show_preview(
        &mut self,
        ui: &mut egui::Ui,
        font: &FontRecord,
        context: &egui::Context,
        base_font_definitions: Option<&egui::FontDefinitions>,
    ) {
        ui.horizontal(|ui| {
            ui.label("Preview text");
            ui.add_sized(
                [ui.available_width().min(360.0), ui.available_height()],
                egui::TextEdit::singleline(&mut self.preview_text),
            );
        });

        ui.horizontal(|ui| {
            ui.add(
                egui::Slider::new(&mut self.preview_size, 12.0..=144.0)
                    .text("Size")
                    .suffix(" px"),
            );
            if font.variable {
                let min = font.weights.iter().copied().min().unwrap_or(100) as i32;
                let max = font.weights.iter().copied().max().unwrap_or(900) as i32;
                let response =
                    ui.add(egui::Slider::new(&mut self.preview_weight, min..=max).text("Weight"));
                let weight_axis = if self.active_font.as_deref() == Some(font.id.as_str()) {
                    self.downloaded_fonts
                        .get(&font.id)
                        .or_else(|| self.preview_fonts.get(&font.id))
                } else {
                    self.preview_fonts
                        .get(&font.id)
                        .or_else(|| self.downloaded_fonts.get(&font.id))
                }
                .is_some_and(|asset| asset.weight_axis);
                if weight_axis && response.changed() {
                    if self.downloaded_fonts.contains_key(&font.id)
                        && self.active_font.as_deref() == Some(font.id.as_str())
                    {
                        self.apply_font(&font.id, context, base_font_definitions);
                    } else {
                        self.refresh_preview_fonts(context, base_font_definitions);
                    }
                } else if !weight_axis
                    && ((response.changed() && !response.dragged()) || response.drag_stopped())
                {
                    self.request_preview_weight(font);
                }
                if self
                    .weight_preview_for
                    .as_ref()
                    .is_some_and(|(id, _)| id == &font.id)
                {
                    ui.spinner();
                }
            } else {
                ui.label(format!("Weights: {}", format_weights(&font.weights)));
            }
            ui.separator();
            if ui.button("Get this font").clicked() {
                self.detail_tab = DetailTab::GetFont;
            }
            if self.active_font.as_deref() == Some(font.id.as_str()) {
                ui.weak("Applied to app text");
            }
        });
        ui.separator();
        let preview = if self.preview_text.is_empty() {
            PREVIEW_SENTENCE
        } else {
            &self.preview_text
        };
        ui.add(
            egui::Label::new(RichText::new(preview).font(self.preview_font_id(
                font,
                self.preview_size,
                context,
            )))
            .wrap(),
        );
        ui.add_space(18.0);
        ui.label(
            RichText::new("The quick brown fox jumps over the lazy dog.")
                .font(self.preview_font_id(font, 22.0, context)),
        );
    }

    fn show_glyphs(&self, ui: &mut egui::Ui, font: &FontRecord, context: &egui::Context) {
        ui.label("Latin character preview");
        ui.add_space(8.0);
        const CHARACTERS: &str =
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!?@&%";
        let cell_size = egui::vec2(42.0, 42.0);
        let grid_spacing = egui::vec2(8.0, 8.0);
        let columns = (((ui.available_width() + grid_spacing.x) / (cell_size.x + grid_spacing.x))
            .floor() as usize)
            .clamp(1, CHARACTERS.chars().count());
        egui::Grid::new("font_market_glyph_grid")
            .num_columns(columns)
            .spacing(grid_spacing)
            .show(ui, |ui| {
                for (index, character) in CHARACTERS.chars().enumerate() {
                    ui.add_sized(
                        cell_size,
                        egui::Button::new(
                            RichText::new(character.to_string())
                                .font(self.preview_font_id(font, 24.0, context)),
                        ),
                    );
                    if index % columns == columns - 1 {
                        ui.end_row();
                    }
                }
            });
    }

    fn show_about(&self, ui: &mut egui::Ui, font: &FontRecord) {
        ui.heading(&font.family);
        egui::Grid::new("font_market_about")
            .num_columns(2)
            .spacing([18.0, 10.0])
            .show(ui, |ui| {
                about_row(ui, "Category", display_category(&font.category));
                about_row(ui, "Source", &font.source_type);
                about_row(ui, "Version", &font.version);
                about_row(ui, "Style", &font.styles.join(", "));
                about_row(ui, "Weights", &format_weights(&font.weights));
                about_row(ui, "Subsets", &font.subsets.join(", "));
                about_row(ui, "Variable", if font.variable { "Yes" } else { "No" });
            });
    }

    fn show_get_font(
        &mut self,
        ui: &mut egui::Ui,
        font: &FontRecord,
        context: &egui::Context,
        base_font_definitions: Option<&egui::FontDefinitions>,
    ) {
        ui.label("Download all weights and styles for the selected language coverage.");
        ui.add_space(8.0);
        if let Some(default_subset) = self
            .details
            .as_ref()
            .map(|details| details.record.default_subset.clone())
        {
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut self.include_all_font_subsets,
                    "Include all language subsets",
                );
                if self.include_all_font_subsets {
                    ui.weak("May include hundreds of split font files");
                } else {
                    ui.weak(format!("Default subset: {default_subset}"));
                }
            });
        }
        ui.horizontal(|ui| {
            let bundle_busy = self.bundle_download.is_some() || self.save_request.is_some();
            let file_count = self
                .details
                .as_ref()
                .map(|details| details.ttf_files(self.include_all_font_subsets).len())
                .unwrap_or_default();
            let download_label = match file_count {
                0 => "↓  Download".to_owned(),
                1 => "↓  Download 1 file".to_owned(),
                count => format!("↓  Download {count} files"),
            };
            if ui
                .add_enabled(
                    file_count > 0 && !bundle_busy,
                    egui::Button::new(download_label),
                )
                .clicked()
            {
                self.download_font_files(font, self.include_all_font_subsets);
            }

            let activation_pending = self.download_for.as_deref() == Some(font.id.as_str());
            let mut enabled =
                self.active_font.as_deref() == Some(font.id.as_str()) || activation_pending;
            let can_change_font = self.download_request.is_none() || activation_pending;
            if ui
                .add_enabled(
                    can_change_font,
                    egui::Checkbox::new(&mut enabled, "Use in app"),
                )
                .changed()
            {
                if enabled {
                    self.enable_font(font, context, base_font_definitions);
                } else {
                    if activation_pending {
                        self.download_for = None;
                    }
                    if self.active_font.as_deref() == Some(font.id.as_str()) {
                        self.active_font = None;
                        self.active_font_weight = 400;
                        self.refresh_preview_fonts(context, base_font_definitions);
                    }
                }
            }

            if activation_pending {
                ui.spinner();
                ui.label("Loading font…");
            }
            if self
                .bundle_download
                .as_ref()
                .is_some_and(|bundle| bundle.font_id == font.id)
            {
                ui.spinner();
                ui.label("Downloading font files…");
            } else if self.save_request.is_some() {
                ui.spinner();
                ui.label("Saving font files…");
            }
        });

        if let Some(bundle) = self
            .bundle_download
            .as_ref()
            .filter(|bundle| bundle.font_id == font.id)
        {
            ui.add(
                egui::ProgressBar::new(bundle.files.len() as f32 / bundle.total as f32)
                    .text(format!("{} / {} files", bundle.files.len(), bundle.total)),
            );
        }
        if let Some(error) = self.download_error.as_deref() {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(message) = self.message.as_deref() {
            ui.weak(message);
        }
    }

    fn preview_font_id(&self, font: &FontRecord, size: f32, context: &egui::Context) -> FontId {
        let family = if font_family_is_available(context, &font.id) {
            font_family(&font.id)
        } else {
            FontFamily::Proportional
        };
        FontId::new(size, family)
    }

    fn enable_font(
        &mut self,
        font: &FontRecord,
        context: &egui::Context,
        base_font_definitions: Option<&egui::FontDefinitions>,
    ) {
        self.download_error = None;
        self.message = None;
        if let Some(asset) = self
            .preview_fonts
            .get(&font.id)
            .or_else(|| self.downloaded_fonts.get(&font.id))
            .cloned()
        {
            self.downloaded_fonts.insert(font.id.clone(), asset);
            self.apply_font(&font.id, context, base_font_definitions);
            return;
        }
        let Some(details) = self.details.as_ref() else {
            return;
        };
        let Some(url) = details.ttf_url_for_weight(self.preview_weight) else {
            self.download_error =
                Some("No TrueType font file is available for this family.".to_owned());
            return;
        };
        self.download_error = None;
        self.message = None;
        self.download_for = Some(font.id.clone());
        self.download_request = Some(fetch(url));
    }

    fn download_font_files(&mut self, font: &FontRecord, include_all_subsets: bool) {
        let Some(details) = self
            .details
            .as_ref()
            .filter(|details| details.record.id == font.id)
        else {
            return;
        };
        let files = details.ttf_files(include_all_subsets);
        if files.is_empty() {
            self.download_error =
                Some("No TrueType font files are available for this family.".to_owned());
            return;
        }

        self.download_error = None;
        self.message = None;
        self.bundle_download = Some(FontBundleDownload {
            font_id: font.id.clone(),
            total: files.len(),
            queued: files.into(),
            pending: Vec::new(),
            files: Vec::new(),
        });
        self.pump_bundle_requests();
    }

    fn pump_bundle_requests(&mut self) {
        let Some(bundle) = &mut self.bundle_download else {
            return;
        };
        while bundle.pending.len() < 4 {
            let Some((name, url)) = bundle.queued.pop_front() else {
                break;
            };
            bundle.pending.push(FontFileRequest {
                name,
                receiver: fetch(&url),
            });
        }
    }

    fn poll_bundle_download(&mut self, context: &egui::Context) {
        let mut failure = None;
        if let Some(bundle) = &mut self.bundle_download {
            let mut pending = Vec::with_capacity(bundle.pending.len());
            for request in bundle.pending.drain(..) {
                if let Some(result) = try_poll_receiver(&request.receiver) {
                    match result.and_then(response_bytes) {
                        Ok(bytes) if is_supported_font(&bytes) => {
                            bundle.files.push((request.name, bytes));
                        }
                        Ok(_) => {
                            failure = Some(format!(
                                "Fontsource returned an unsupported file for {}.",
                                request.name
                            ));
                            break;
                        }
                        Err(error) => {
                            failure = Some(error);
                            break;
                        }
                    }
                } else {
                    pending.push(request);
                }
            }
            bundle.pending = pending;
        }

        if let Some(error) = failure {
            self.bundle_download = None;
            self.download_error = Some(error);
            return;
        }

        self.pump_bundle_requests();
        let finished = self
            .bundle_download
            .as_ref()
            .is_some_and(|bundle| bundle.queued.is_empty() && bundle.pending.is_empty());
        if finished {
            let bundle = self.bundle_download.take().expect("finished bundle exists");
            self.save_font_bundle(bundle.font_id, bundle.files, context);
        }
    }

    fn save_font_bundle(
        &mut self,
        id: String,
        files: Vec<(String, Vec<u8>)>,
        context: &egui::Context,
    ) {
        let (sender, receiver) = mpsc::channel();
        self.save_request = Some(receiver);
        let context = context.clone();

        trtask::spawn_background(async move {
            let result = save_font_files(id, files).await;
            let _ = sender.send(result);
            context.request_repaint();
        });
    }

    fn request_preview_weight(&mut self, font: &FontRecord) {
        let Some(url) = self
            .details
            .as_ref()
            .filter(|details| details.record.id == font.id)
            .and_then(|details| details.ttf_url_for_weight(self.preview_weight))
            .map(str::to_owned)
        else {
            return;
        };

        self.weight_preview_for = Some((font.id.clone(), self.preview_weight));
        self.weight_preview_request = Some(fetch(&url));
    }

    fn apply_font(
        &mut self,
        id: &str,
        context: &egui::Context,
        base_font_definitions: Option<&egui::FontDefinitions>,
    ) {
        if !self.downloaded_fonts.contains_key(id) {
            return;
        }
        self.active_font = Some(id.to_owned());
        self.active_font_weight = if self.selected.as_ref().is_some_and(|font| font.id == id) {
            self.preview_weight
        } else {
            400
        };
        let mut definitions = self.font_definitions(base_font_definitions);
        let data_name = font_data_name(id);
        let proportional = definitions
            .families
            .entry(FontFamily::Proportional)
            .or_default();
        proportional.retain(|name| name != &data_name);
        proportional.insert(0, data_name);
        context.set_fonts(definitions);
    }

    fn pump_preview_requests(&mut self) {
        while self.preview_detail_requests.len() + self.preview_font_requests.len() < 4 {
            let Some(id) = self.preview_queue.pop_front() else {
                break;
            };
            self.preview_detail_requests
                .insert(id.clone(), fetch(&format!("{FONTS_API}/{id}")));
        }
    }

    fn refresh_preview_fonts(
        &mut self,
        context: &egui::Context,
        base_font_definitions: Option<&egui::FontDefinitions>,
    ) {
        let mut definitions = self.font_definitions(base_font_definitions);
        if let Some(active_font) = &self.active_font {
            let active_data_name = font_data_name(active_font);
            let proportional = definitions
                .families
                .entry(FontFamily::Proportional)
                .or_default();
            proportional.retain(|name| name != &active_data_name);
            proportional.insert(0, active_data_name);
        }
        context.set_fonts(definitions);
    }

    fn font_definitions(
        &self,
        base_font_definitions: Option<&egui::FontDefinitions>,
    ) -> egui::FontDefinitions {
        let mut definitions = base_font_definitions.cloned().unwrap_or_else(|| {
            let mut definitions = egui::FontDefinitions::default();
            tregui::egui_phosphor::add_to_fonts(
                &mut definitions,
                tregui::egui_phosphor::Variant::Regular,
            );
            definitions
        });
        self.add_cached_fonts(&mut definitions);
        definitions
    }

    fn add_cached_fonts(&self, definitions: &mut egui::FontDefinitions) {
        let ids: HashSet<_> = self
            .preview_fonts
            .keys()
            .chain(self.downloaded_fonts.keys())
            .collect();
        for id in ids {
            let asset = if self.active_font.as_deref() == Some(id.as_str()) {
                self.downloaded_fonts
                    .get(id)
                    .or_else(|| self.preview_fonts.get(id))
            } else {
                self.preview_fonts
                    .get(id)
                    .or_else(|| self.downloaded_fonts.get(id))
            };
            let Some(asset) = asset else {
                continue;
            };
            let data_name = font_data_name(id);
            let weight = if !asset.weight_axis {
                400
            } else if self.selected.as_ref().is_some_and(|font| font.id == *id) {
                self.preview_weight
            } else if self.active_font.as_deref() == Some(id.as_str()) {
                self.active_font_weight
            } else {
                400
            };
            let mut data = egui::FontData::from_owned(asset.bytes.clone());
            if asset.weight_axis {
                let mut tweak = egui::FontTweak::default();
                tweak.coords.push(b"wght", weight as f32);
                data = data.tweak(tweak);
            }
            definitions
                .font_data
                .insert(data_name.clone(), Arc::new(data));
            definitions
                .families
                .insert(font_family(id), vec![data_name]);
        }
    }

    fn has_pending_requests(&self) -> bool {
        self.catalog_request.is_some()
            || self.detail_request.is_some()
            || self.download_request.is_some()
            || self.weight_preview_request.is_some()
            || self.bundle_download.is_some()
            || self.save_request.is_some()
            || !self.preview_detail_requests.is_empty()
            || !self.preview_font_requests.is_empty()
            || !self.preview_queue.is_empty()
    }
}

async fn save_font_files(id: String, mut files: Vec<(String, Vec<u8>)>) -> SaveResult {
    files.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    let (file_name, bytes, archive) = if files.len() == 1 {
        let (name, bytes) = files.pop().expect("one font file exists");
        (name, bytes, false)
    } else {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in files {
            writer
                .start_file(name, options)
                .map_err(|error| format!("Could not package font files: {error}"))?;
            writer
                .write_all(&bytes)
                .map_err(|error| format!("Could not package font files: {error}"))?;
        }
        let bytes = writer
            .finish()
            .map_err(|error| format!("Could not package font files: {error}"))?
            .into_inner();
        (format!("{id}.zip"), bytes, true)
    };

    let dialog = rfd::AsyncFileDialog::new()
        .set_title("Save font files")
        .set_file_name(file_name.clone());
    let dialog = if archive {
        dialog.add_filter("ZIP archive", &["zip"])
    } else {
        dialog.add_filter("TrueType font", &["ttf"])
    };
    let Some(file) = dialog.save_file().await else {
        return Ok(None);
    };

    let saved_name = file.file_name();
    file.write(&bytes)
        .await
        .map(|()| Some(format!("Saved {saved_name}")))
        .map_err(|error| format!("Could not save font files: {error}"))
}

fn fetch(url: &str) -> Receiver<HttpResult> {
    let (sender, receiver) = mpsc::channel();
    ehttp::fetch(ehttp::Request::get(url), move |result| {
        let result = result.map_err(|error| format!("Fontsource request failed: {error}"));
        let _ = sender.send(result);
    });
    receiver
}

fn poll(receiver: &mut Option<Receiver<HttpResult>>) -> Option<HttpResult> {
    let result = receiver.as_ref()?.try_recv();
    match result {
        Ok(response) => {
            *receiver = None;
            Some(response)
        }
        Err(mpsc::TryRecvError::Disconnected) => {
            *receiver = None;
            Some(Err("Fontsource request was interrupted.".to_owned()))
        }
        Err(mpsc::TryRecvError::Empty) => None,
    }
}

fn poll_save_request(receiver: &mut Option<Receiver<SaveResult>>) -> Option<SaveResult> {
    let result = receiver.as_ref()?.try_recv();
    match result {
        Ok(result) => {
            *receiver = None;
            Some(result)
        }
        Err(mpsc::TryRecvError::Disconnected) => {
            *receiver = None;
            Some(Err("Font save was interrupted.".to_owned()))
        }
        Err(mpsc::TryRecvError::Empty) => None,
    }
}

fn try_poll_receiver(receiver: &Receiver<HttpResult>) -> Option<HttpResult> {
    match receiver.try_recv() {
        Ok(response) => Some(response),
        Err(mpsc::TryRecvError::Disconnected) => {
            Some(Err("Fontsource request was interrupted.".to_owned()))
        }
        Err(mpsc::TryRecvError::Empty) => None,
    }
}

fn parse_json<T: serde::de::DeserializeOwned>(response: ehttp::Response) -> Result<T, String> {
    let bytes = response_bytes(response)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Could not read Fontsource response: {error}"))
}

fn response_bytes(response: ehttp::Response) -> Result<Vec<u8>, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("Fontsource returned HTTP {}.", response.status));
    }
    Ok(response.bytes)
}

fn is_supported_font(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\0\x01\0\0")
        || bytes.starts_with(b"OTTO")
        || bytes.starts_with(b"true")
        || bytes.starts_with(b"typ1")
}

fn has_weight_axis(bytes: &[u8]) -> bool {
    egui::FontData::from_owned(bytes.to_vec())
        .variation_axes()
        .iter()
        .any(|axis| axis.tag.to_be_bytes() == *b"wght")
}

fn font_data_name(id: &str) -> String {
    format!("fontsource-{id}")
}

fn font_file_component(value: &str) -> String {
    let component: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect();
    if component.is_empty() {
        "font".to_owned()
    } else {
        component
    }
}

fn font_family_is_available(context: &egui::Context, id: &str) -> bool {
    let family = font_family(id);
    context.fonts(|fonts| {
        fonts
            .definitions()
            .families
            .get(&family)
            .is_some_and(|names| {
                !names.is_empty()
                    && names
                        .iter()
                        .all(|name| fonts.definitions().font_data.contains_key(name))
            })
    })
}

fn font_family(id: &str) -> FontFamily {
    FontFamily::Name(Arc::<str>::from(font_data_name(id)))
}

fn unique_values<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut values: Vec<_> = values
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect();
    values.sort_unstable();
    values.dedup();
    values
}

fn option_filter(
    ui: &mut egui::Ui,
    all_label: &str,
    id: &'static str,
    options: &[String],
    selected: &mut Option<String>,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected.as_deref().unwrap_or(all_label))
        .show_ui(ui, |ui| {
            ui.selectable_value(selected, None, all_label);
            for option in options {
                ui.selectable_value(selected, Some(option.clone()), option);
            }
        });
}

fn display_category(category: &str) -> &str {
    if category.is_empty() {
        "Uncategorized"
    } else {
        category
    }
}

fn format_weights(weights: &[u16]) -> String {
    if weights.is_empty() {
        "Not listed".to_owned()
    } else {
        weights
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn about_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.strong(label);
    ui.label(if value.is_empty() {
        "Not listed"
    } else {
        value
    });
    ui.end_row();
}
