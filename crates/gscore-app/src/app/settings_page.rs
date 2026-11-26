//! The settings window's Page layout tab: paper size and orientation, margins, the score's
//! size, and what the header and footer show, with a preview of the first page as the
//! draft would print it. Applied as one command (one undo step).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use egui::{RichText, Sense, Ui};
use gscore_edit::Command;
use gscore_layout::{Fonts, Layout, LayoutCache, LayoutOptions, MusicFontChoice, Theme};
use gscore_model::{
    MAX_MARGIN_MM, PAGE_SIZE_MM, PageSetup, PaperSize, SCALE_RANGE, Song, TrackIndex,
};

use crate::canvas::{self, Arrangement, POINTS_PER_SPACE};

/// The page setup being edited, and the song's when it was last synced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct PageDraft {
    pub(super) draft: PageSetup,
    pub(super) base: PageSetup,
}

impl PageDraft {
    pub(super) fn new(setup: PageSetup) -> Self {
        PageDraft {
            draft: setup,
            base: setup,
        }
    }

    /// Takes the song's page setup: an edit from elsewhere (undo) replaces an untouched
    /// draft.
    pub(super) fn sync(&mut self, current: PageSetup) {
        if current != self.base {
            if self.draft == self.base {
                self.draft = current;
            }
            self.base = current;
        }
    }

    /// The command that applies the draft, if it changed and is valid.
    pub(super) fn command(&self, current: PageSetup) -> Option<Command> {
        (self.draft != current && self.draft.check().is_ok())
            .then_some(Command::SetPageSetup { setup: self.draft })
    }
}

/// A labelled millimetre field.
fn millimetres(ui: &mut Ui, label: &str, value: &mut u16, range: (u16, u16)) {
    ui.label(label);
    let response = ui.add(
        egui::DragValue::new(value)
            .range(range.0..=range.1)
            .suffix(" mm"),
    );
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_label(label.to_owned()));
    ui.end_row();
}

/// The fields: paper, orientation, size, margins, score size, header and footer.
fn fields(ui: &mut Ui, setup: &mut PageSetup) {
    egui::Grid::new("page_fields")
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.label("Paper");
            let named = PaperSize::of(setup.width_mm, setup.height_mm);
            let landscape = setup.is_landscape();
            let paper = egui::ComboBox::from_id_salt("paper_size")
                .selected_text(named.map_or("Custom", PaperSize::name))
                .show_ui(ui, |ui| {
                    for size in PaperSize::ALL {
                        if ui
                            .selectable_label(named == Some(size), size.name())
                            .clicked()
                        {
                            let (width, height) = size.portrait_mm();
                            (setup.width_mm, setup.height_mm) = if landscape {
                                (height, width)
                            } else {
                                (width, height)
                            };
                        }
                    }
                });
            ui.ctx()
                .accesskit_node_builder(paper.response.id, |node| node.set_label("Paper size"));
            ui.end_row();
            ui.label("Orientation");
            ui.horizontal(|ui| {
                if ui.selectable_label(!landscape, "Portrait").clicked() && landscape {
                    *setup = setup.turned();
                }
                if ui.selectable_label(landscape, "Landscape").clicked() && !landscape {
                    *setup = setup.turned();
                }
            });
            ui.end_row();
            millimetres(ui, "Width", &mut setup.width_mm, PAGE_SIZE_MM);
            millimetres(ui, "Height", &mut setup.height_mm, PAGE_SIZE_MM);
            let margin = (0, MAX_MARGIN_MM);
            millimetres(ui, "Top margin", &mut setup.margins.top, margin);
            millimetres(ui, "Bottom margin", &mut setup.margins.bottom, margin);
            millimetres(ui, "Left margin", &mut setup.margins.left, margin);
            millimetres(ui, "Right margin", &mut setup.margins.right, margin);
            ui.label("Score size");
            let response = ui.add(
                egui::DragValue::new(&mut setup.scale_percent)
                    .range(SCALE_RANGE.0..=SCALE_RANGE.1)
                    .suffix(" %"),
            );
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_label("Score size"));
            ui.end_row();
        });
    ui.add_space(6.0);
    ui.add(super::style::switch(
        &mut setup.page_numbers,
        "Page numbers",
    ));
    ui.add(super::style::switch(
        &mut setup.copyright,
        "Copyright on the first page",
    ));
    ui.add(super::style::switch(
        &mut setup.running_header,
        "Title and artist atop later pages",
    ));
}

/// Width of the preview of the first page, in points.
const PREVIEW_WIDTH: f32 = 230.0;

/// The first page as the draft would print it, scaled to fit the preview's width.
fn preview_panel(ui: &mut Ui, layout: Option<&Layout>, valid: bool, music: MusicFontChoice) {
    ui.label(RichText::new("Preview").weak());
    let page = layout.and_then(|layout| layout.pages.first().map(|page| (layout, *page)));
    let Some((layout, page)) = page.filter(|_| valid) else {
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(PREVIEW_WIDTH, PREVIEW_WIDTH * 297.0 / 210.0),
            Sense::hover(),
        );
        ui.painter()
            .rect_filled(rect, 2.0, ui.visuals().faint_bg_color);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            if valid {
                "Open a song to see its pages"
            } else {
                "Fix the values to see the page"
            },
            egui::FontId::proportional(12.0),
            ui.visuals().weak_text_color(),
        );
        return;
    };
    let zoom = PREVIEW_WIDTH / (page.width * POINTS_PER_SPACE);
    let height = page.height * POINTS_PER_SPACE * zoom;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(PREVIEW_WIDTH, height), Sense::hover());
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label("Page preview");
    });
    canvas::paint(
        &ui.painter_at(rect),
        rect.min,
        layout,
        &Arrangement::new(layout, 1),
        &Theme::LIGHT,
        zoom,
        rect,
        canvas::Surface::Pages,
        music,
    );
    ui.label(
        RichText::new(format!("Page 1 of {}", layout.pages.len()))
            .small()
            .weak(),
    );
}

/// The draft's pages as they would print: the same tracks and options as the PDF export
/// (`print_options`), laid out again only when those, the song or the music font change.
/// Its own cache: the score's view keeps its layout.
#[derive(Debug, Default)]
pub(super) struct PagePreview {
    cache: LayoutCache,
    shown: Option<Shown>,
}

/// What the preview shows and what it was laid out from.
#[derive(Debug)]
struct Shown {
    tracks: Vec<usize>,
    options: LayoutOptions,
    revision: u64,
    music: MusicFontChoice,
    layout: Arc<Layout>,
}

impl PagePreview {
    /// The pages of `tracks` (editor `revision`, the first track leading) laid out with
    /// `options` (the print options with the draft's page setup).
    pub(super) fn layout(
        &mut self,
        song: &Song,
        revision: u64,
        tracks: Vec<usize>,
        options: LayoutOptions,
        fonts: &Fonts,
        music: MusicFontChoice,
    ) -> Option<Arc<Layout>> {
        let fresh = self.shown.as_ref().is_some_and(|shown| {
            shown.tracks == tracks
                && shown.options == options
                && shown.revision == revision
                && shown.music == music
        });
        if !fresh {
            let indices: Vec<TrackIndex> = tracks.iter().copied().map(TrackIndex::new).collect();
            let first = *indices.first()?;
            let cache = &mut self.cache;
            // A bug in layout must not take the settings window down.
            let laid_out = catch_unwind(AssertUnwindSafe(|| {
                cache.layout_tracks(song, &indices, first, &options, fonts)
            }));
            let Ok(Ok(layout)) = laid_out else {
                self.cache = LayoutCache::default();
                self.shown = None;
                return None;
            };
            self.shown = Some(Shown {
                tracks,
                options,
                revision,
                music,
                layout: Arc::new(layout),
            });
        }
        self.shown.as_ref().map(|shown| Arc::clone(&shown.layout))
    }
}

/// What the tab asked for.
#[derive(Debug, Default)]
pub(super) struct PageTabActions {
    /// Apply was clicked.
    pub(super) apply: bool,
    /// Done was clicked.
    pub(super) close: bool,
    /// Another music font was chosen (it applies at once, to every song).
    pub(super) music: Option<MusicFontChoice>,
}

/// The tab: the page fields beside the preview, the music font under them, then the
/// buttons.
pub(super) fn page_tab(
    ui: &mut Ui,
    page: &mut PageDraft,
    preview: Option<&Layout>,
    has_song: bool,
    music: MusicFontChoice,
    actions: &mut PageTabActions,
) {
    if !has_song {
        ui.label(RichText::new("Open a song to set up its pages.").weak());
    }
    let setup = &mut page.draft;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| fields(ui, setup));
        ui.add_space(16.0);
        ui.vertical(|ui| preview_panel(ui, preview, setup.check().is_ok(), music));
    });
    let problem = setup.check().err();
    if let Some(problem) = &problem {
        ui.colored_label(ui.visuals().error_fg_color, problem.to_string());
    }
    // The music font, where its effect shows in the preview above. It is the app's, not
    // the song's: it applies at once, to every song (Apply and Revert leave it alone).
    if MusicFontChoice::available().len() > 1 {
        ui.add_space(6.0);
        ui.label(RichText::new("Music font").strong());
        ui.label(
            RichText::new(
                "How notation and tablature look, on screen and in PDFs. Changes at once and \
                 for every song.",
            )
            .weak(),
        );
        let mut chosen = music;
        if super::music_font_picker::picker(ui, &mut chosen) {
            actions.music = Some(chosen);
        }
    }
    ui.separator();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(has_song && problem.is_none(), egui::Button::new("Apply"))
            .clicked()
        {
            actions.apply = true;
        }
        if ui.button("Revert").clicked() {
            page.draft = page.base;
        }
        if ui.button("Guitar Pro defaults").clicked() {
            page.draft = PageSetup::default();
        }
        if ui.button("Done").clicked() {
            actions.close = true;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preview_is_laid_out_again_only_when_something_changed() {
        let song = gscore_model::testing::song(&[gscore_model::testing::GUITAR_6], 60).unwrap();
        let fonts = Fonts::bundled().unwrap();
        let on = |setup: PageSetup| LayoutOptions::default().with_page_setup(&setup).on_pages();
        let mut preview = PagePreview::default();
        let a4 = PageSetup::default();
        let bravura = MusicFontChoice::Bravura;
        let first = preview
            .layout(&song, 1, vec![0], on(a4), &fonts, bravura)
            .unwrap();
        let again = preview
            .layout(&song, 1, vec![0], on(a4), &fonts, bravura)
            .unwrap();
        assert!(
            Arc::ptr_eq(&first, &again),
            "nothing changed: the same layout"
        );
        let landscape = preview
            .layout(&song, 1, vec![0], on(a4.turned()), &fonts, bravura)
            .unwrap();
        assert!(landscape.pages[0].width > landscape.pages[0].height);
        assert!(landscape.pages.len() >= first.pages.len());
        let edited = preview
            .layout(&song, 2, vec![0], on(a4.turned()), &fonts, bravura)
            .unwrap();
        assert!(
            !Arc::ptr_eq(&landscape, &edited),
            "an edit lays it out again"
        );
        let rests = LayoutOptions {
            multi_bar_rests: true,
            ..on(a4.turned())
        };
        let with_rests = preview
            .layout(&song, 2, vec![0], rests, &fonts, bravura)
            .unwrap();
        assert!(
            !Arc::ptr_eq(&edited, &with_rests),
            "another view option lays it out again"
        );
        assert!(
            with_rests.pages.len() < edited.pages.len(),
            "60 empty bars as one multi-bar rest take fewer pages ({} vs {})",
            with_rests.pages.len(),
            edited.pages.len()
        );
        assert!(
            preview
                .layout(&song, 2, vec![7], on(a4), &fonts, bravura)
                .is_none(),
            "no track 8"
        );
        assert!(
            preview
                .layout(&song, 2, Vec::new(), on(a4), &fonts, bravura)
                .is_none(),
            "no track at all"
        );
    }

    #[test]
    fn a_draft_follows_outside_edits_until_it_is_touched() {
        let mut page = PageDraft::new(PageSetup::default());
        let letter = PageSetup {
            width_mm: 216,
            height_mm: 279,
            ..PageSetup::default()
        };
        page.sync(letter);
        assert_eq!(page.draft, letter, "an untouched draft follows");
        page.draft.margins.left = 20;
        page.sync(PageSetup::default());
        assert_eq!(page.draft.margins.left, 20, "a touched draft stays");
        assert!(page.command(PageSetup::default()).is_some());
        page.draft.width_mm = 10;
        assert!(
            page.command(PageSetup::default()).is_none(),
            "invalid drafts do not apply"
        );
    }
}
