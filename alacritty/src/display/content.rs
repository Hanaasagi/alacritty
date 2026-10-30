use std::borrow::Cow;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::{cmp, mem};

use alacritty_terminal::event::EventListener;
use alacritty_terminal::graphics::Image;
use alacritty_terminal::graphics::placement::{Placement, PlacementLocation};
use alacritty_terminal::graphics::unicode_placeholder::{
    PlaceholderCell, VirtualPlacementInfo, fragment_index_from_diacritic, image_id_from_fg_color,
    is_placeholder, placement_id_from_underline_color,
};
use alacritty_terminal::grid::{Dimensions, Indexed};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb as VteRgb};
use alacritty_terminal::term::search::{Match, RegexSearch};
use alacritty_terminal::term::{self, RenderableContent as TerminalContent, Term, TermMode};

use crate::config::UiConfig;
use crate::display::color::{CellRgb, DIM_FACTOR, List, Rgb};
use crate::display::hint::{self, HintState};
use crate::display::{Display, SizeInfo};
use crate::event::SearchState;

/// Minimum contrast between a fixed cursor color and the cell's background.
pub const MIN_CURSOR_CONTRAST: f64 = 1.5;

/// GPU-ready Kitty graphics placement.
#[derive(Debug, Clone)]
pub struct RenderableImagePlacement {
    pub image_id: u32,
    pub viewport_row: usize,
    pub viewport_col: usize,
    pub display_cols: u32,
    pub display_rows: u32,
    pub z_index: i32,
    pub source_x: u32,
    pub source_y: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub x_offset: u32,
    pub y_offset: u32,
}

/// Pixel data and placements for one frame of Kitty graphics rendering.
#[derive(Debug, Clone, Default)]
pub struct RenderableGraphics {
    pub placements: Vec<RenderableImagePlacement>,
    pub images: HashMap<u32, RenderableImage>,
    pub sync_to_gpu: bool,
    /// All image IDs referenced by any placement (including off-viewport).
    pub all_image_ids: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct RenderableImage {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl RenderableGraphics {
    pub fn collect<T: EventListener>(
        term: &Term<T>,
        display_offset: usize,
        size_info: &SizeInfo,
    ) -> Self {
        let sync_to_gpu = term.graphics_state.dirty;
        let virtual_infos = collect_virtual_placement_infos(term);

        let mut all_image_ids = Vec::with_capacity(term.graphics_state.placements.len());
        all_image_ids.extend(term.graphics_state.placements.iter().map(|p| p.image_id));

        let mut placements = Vec::new();

        for placement in &term.graphics_state.placements {
            let Some(renderable) =
                resolve_and_clip(placement, &virtual_infos, display_offset, size_info, term)
            else {
                continue;
            };

            placements.push(renderable);
        }

        // Sync pixels for every referenced image, visible or not: an image
        // replaced while off-screen (e.g. in scrollback) must not keep its
        // stale GPU texture when it scrolls back into view.
        let mut images = HashMap::new();
        if sync_to_gpu {
            for &image_id in &all_image_ids {
                if let Some(image) = term.graphics_state.storage.get_image(image_id) {
                    images.insert(
                        image_id,
                        RenderableImage {
                            width: image.width,
                            height: image.height,
                            data: image.data.clone(),
                        },
                    );
                }
            }
        }

        Self { placements, images, sync_to_gpu, all_image_ids }
    }
}

fn resolve_source_dimension(
    value: u32,
    placement: &Placement,
    term: &Term<impl EventListener>,
    width: bool,
) -> u32 {
    if value != 0 {
        return value;
    }
    term.graphics_state
        .storage
        .get_image(placement.image_id)
        .map(|image| if width { image.width } else { image.height })
        .unwrap_or(0)
}

/// Resolve a placement's position, clip it against the viewport, and produce
/// a GPU-ready `RenderableImagePlacement`. Returns `None` if the image has no
/// intersection with the current viewport.
fn resolve_and_clip<T: EventListener>(
    placement: &Placement,
    virtual_infos: &HashMap<(u32, u32), VirtualPlacementInfo>,
    display_offset: usize,
    size_info: &SizeInfo,
    term: &Term<T>,
) -> Option<RenderableImagePlacement> {
    let image = term.graphics_state.storage.get_image(placement.image_id)?;
    let cell_w = size_info.cell_width();
    let cell_h = size_info.cell_height();

    let (grid_row, grid_col, display_cols, display_rows): (i32, usize, u32, u32) =
        if placement.virtual_placement {
            let info = virtual_infos.get(&(placement.image_id, placement.placement_id))?;
            let cols = if placement.columns != 0 {
                placement.columns
            } else {
                info.cols as u32
            };
            let rows = if placement.rows != 0 {
                placement.rows
            } else {
                info.rows as u32
            };
            (info.start_row, info.start_col, cols, rows)
        } else {
            let PlacementLocation::Pin { row, col } = placement.location else {
                return None;
            };
            let cols = if placement.columns != 0 {
                placement.columns
            } else {
                auto_display_cols(placement, image, cell_w)
            };
            let rows = if placement.rows != 0 {
                placement.rows
            } else {
                auto_display_rows(placement, image, cell_h)
            };
            (row, col, cols, rows)
        };

    if display_cols == 0 || display_rows == 0 {
        return None;
    }

    // Signed viewport coordinates (can be negative = above viewport).
    let vp_row = grid_row + display_offset as i32;
    let vp_col = grid_col as i32;

    let screen_lines = size_info.screen_lines() as i32;
    let columns = size_info.columns() as i32;

    // Rectangle intersection with viewport [0, screen_lines) × [0, columns).
    let img_top = vp_row;
    let img_bot = vp_row + display_rows as i32;
    let img_left = vp_col;
    let img_right = vp_col + display_cols as i32;

    let vis_top = img_top.max(0);
    let vis_bot = img_bot.min(screen_lines);
    let vis_left = img_left.max(0);
    let vis_right = img_right.min(columns);

    if vis_top >= vis_bot || vis_left >= vis_right {
        return None;
    }

    // How many grid cells are clipped from each edge.
    let skip_top = (vis_top - img_top) as u32;
    let skip_left = (vis_left - img_left) as u32;
    let vis_rows = (vis_bot - vis_top) as u32;
    let vis_cols = (vis_right - vis_left) as u32;

    // Resolve full source dimensions.
    let full_src_w = resolve_source_dimension(placement.source_width, placement, term, true);
    let full_src_h = resolve_source_dimension(placement.source_height, placement, term, false);

    // Adjust source rectangle for clipping (linear mapping).
    let src_x = placement.source_x + skip_left * full_src_w / display_cols;
    let src_y = placement.source_y + skip_top * full_src_h / display_rows;
    let src_w = vis_cols * full_src_w / display_cols;
    let src_h = vis_rows * full_src_h / display_rows;

    // Pixel offsets: only apply if the corresponding edge is not clipped.
    let x_offset = if skip_left == 0 { placement.x_offset } else { 0 };
    let y_offset = if skip_top == 0 { placement.y_offset } else { 0 };

    Some(RenderableImagePlacement {
        image_id: placement.image_id,
        viewport_row: vis_top as usize,
        viewport_col: vis_left as usize,
        display_cols: vis_cols,
        display_rows: vis_rows,
        z_index: placement.z_index,
        source_x: src_x,
        source_y: src_y,
        source_width: src_w,
        source_height: src_h,
        x_offset,
        y_offset,
    })
}

fn auto_display_cols(placement: &Placement, image: &Image, cell_w: f32) -> u32 {
    let src_w = if placement.source_width != 0 { placement.source_width } else { image.width };
    ((src_w as f32 / cell_w).ceil() as u32).max(1)
}

fn auto_display_rows(placement: &Placement, image: &Image, cell_h: f32) -> u32 {
    let src_h = if placement.source_height != 0 { placement.source_height } else { image.height };
    ((src_h as f32 / cell_h).ceil() as u32).max(1)
}

fn collect_virtual_placement_infos<T: EventListener>(
    term: &Term<T>,
) -> HashMap<(u32, u32), VirtualPlacementInfo> {
    let mut groups: HashMap<(u32, u32), Vec<PlaceholderCell>> = HashMap::new();

    for indexed in term.grid().display_iter() {
        let cell: &Cell = indexed.cell;
        if !is_placeholder(cell.c) {
            continue;
        }

        let Some((r, g, b)) = truecolor_rgb(cell.fg) else {
            continue;
        };
        let image_id = image_id_from_fg_color(r, g, b);
        let placement_id = cell
            .underline_color()
            .and_then(truecolor_rgb)
            .map(|(r, g, b)| placement_id_from_underline_color(r, g, b))
            .unwrap_or(0);

        let (row, col) = placeholder_fragments(cell);
        let grid_row = indexed.point.line.0;
        let grid_col = indexed.point.column.0;

        groups.entry((image_id, placement_id)).or_default().push(PlaceholderCell {
            image_id,
            placement_id,
            row,
            col,
            grid_row,
            grid_col,
        });
    }

    groups
        .into_iter()
        .filter_map(|(key, cells)| VirtualPlacementInfo::from_cells(&cells).map(|info| (key, info)))
        .collect()
}

fn truecolor_rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Spec(VteRgb { r, g, b, .. }) => Some((r, g, b)),
        _ => None,
    }
}

fn placeholder_fragments(cell: &Cell) -> (u32, u32) {
    let mut row = 0;
    let mut col = 0;
    let mut count = 0;
    if let Some(zerowidth) = cell.zerowidth() {
        for zw in zerowidth {
            if let Some(index) = fragment_index_from_diacritic(*zw) {
                if count == 0 {
                    row = index;
                } else if count == 1 {
                    col = index;
                }
                count += 1;
            }
        }
    }
    (row, col)
}

/// Renderable terminal content.
///
/// This provides the terminal cursor and an iterator over all non-empty cells.
pub struct RenderableContent<'a> {
    terminal_content: TerminalContent<'a>,
    cursor: RenderableCursor,
    cursor_shape: CursorShape,
    cursor_point: Point<usize>,
    search: Option<HintMatches<'a>>,
    hint: Option<Hint<'a>>,
    config: &'a UiConfig,
    colors: &'a List,
    focused_match: Option<&'a Match>,
    size: &'a SizeInfo,
}

impl<'a> RenderableContent<'a> {
    pub fn new<T: EventListener>(
        config: &'a UiConfig,
        display: &'a mut Display,
        term: &'a Term<T>,
        search_state: &'a mut SearchState,
    ) -> Self {
        let search = search_state.dfas().map(|dfas| HintMatches::visible_regex_matches(term, dfas));
        let focused_match = search_state.focused_match();
        let terminal_content = term.renderable_content();

        // Find terminal cursor shape.
        let cursor_shape = if terminal_content.cursor.shape == CursorShape::Hidden
            || display.cursor_hidden
            || search_state.regex().is_some()
            || display.ime.preedit().is_some()
        {
            CursorShape::Hidden
        } else if !term.is_focused && config.cursor.unfocused_hollow {
            CursorShape::HollowBlock
        } else {
            terminal_content.cursor.shape
        };

        // Convert terminal cursor point to viewport position.
        let cursor_point = terminal_content.cursor.point;
        let display_offset = terminal_content.display_offset;
        let cursor_point = term::point_to_viewport(display_offset, cursor_point).unwrap();

        let hint = if display.hint_state.active() {
            display.hint_state.update_matches(term);
            Some(Hint::from(&display.hint_state))
        } else {
            None
        };

        Self {
            colors: &display.colors,
            size: &display.size_info,
            cursor: RenderableCursor::new_hidden(),
            terminal_content,
            focused_match,
            cursor_shape,
            cursor_point,
            search,
            config,
            hint,
        }
    }

    /// Viewport offset.
    pub fn display_offset(&self) -> usize {
        self.terminal_content.display_offset
    }

    /// Get the terminal cursor.
    pub fn cursor(mut self) -> RenderableCursor {
        // Assure this function is only called after the iterator has been drained.
        debug_assert!(self.next().is_none());

        self.cursor
    }

    /// Get the RGB value for a color index.
    pub fn color(&self, color: usize) -> Rgb {
        self.terminal_content.colors[color].map(Rgb).unwrap_or(self.colors[color])
    }

    pub fn selection_range(&self) -> Option<SelectionRange> {
        self.terminal_content.selection
    }

    /// Assemble the information required to render the terminal cursor.
    fn renderable_cursor(&mut self, cell: &RenderableCell) -> RenderableCursor {
        // Cursor colors.
        let color = if self.terminal_content.mode.contains(TermMode::VI) {
            self.config.colors.vi_mode_cursor
        } else {
            self.config.colors.cursor
        };
        let cursor_color = self.terminal_content.colors[NamedColor::Cursor]
            .map_or(color.background, |c| CellRgb::Rgb(Rgb(c)));
        let text_color = color.foreground;

        let insufficient_contrast = (!matches!(cursor_color, CellRgb::Rgb(_))
            || !matches!(text_color, CellRgb::Rgb(_)))
            && cell.fg.contrast(*cell.bg) < MIN_CURSOR_CONTRAST;

        // Convert from cell colors to RGB.
        let mut text_color = text_color.color(cell.fg, cell.bg);
        let mut cursor_color = cursor_color.color(cell.fg, cell.bg);

        // Invert cursor color with insufficient contrast to prevent invisible cursors.
        if insufficient_contrast {
            cursor_color = self.config.colors.primary.foreground;
            text_color = self.config.colors.primary.background;
        }

        let width = if cell.flags.contains(Flags::WIDE_CHAR) {
            NonZeroU32::new(2).unwrap()
        } else {
            NonZeroU32::new(1).unwrap()
        };
        RenderableCursor {
            width,
            shape: self.cursor_shape,
            point: self.cursor_point,
            cursor_color,
            text_color,
        }
    }
}

impl Iterator for RenderableContent<'_> {
    type Item = RenderableCell;

    /// Gets the next renderable cell.
    ///
    /// Skips empty (background) cells and applies any flags to the cell state
    /// (eg. invert fg and bg colors).
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let cell = self.terminal_content.display_iter.next()?;
            let mut cell = RenderableCell::new(self, cell);

            if self.cursor_point == cell.point {
                // Store the cursor which should be rendered.
                self.cursor = self.renderable_cursor(&cell);
                if self.cursor.shape == CursorShape::Block {
                    cell.fg = self.cursor.text_color;
                    cell.bg = self.cursor.cursor_color;

                    // Since we draw Block cursor by drawing cell below it with a proper color,
                    // we must adjust alpha to make it visible.
                    cell.bg_alpha = 1.;
                }

                return Some(cell);
            } else if !cell.is_empty() && !cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                // Skip empty cells and wide char spacers.
                return Some(cell);
            }
        }
    }
}

/// Cell ready for rendering.
#[derive(Clone, Debug)]
pub struct RenderableCell {
    pub character: char,
    pub point: Point<usize>,
    pub fg: Rgb,
    pub bg: Rgb,
    pub bg_alpha: f32,
    pub underline: Rgb,
    pub flags: Flags,
    pub extra: Option<Box<RenderableCellExtra>>,
}

/// Extra storage with rarely present fields for [`RenderableCell`], to reduce the cell size we
/// pass around.
#[derive(Clone, Debug)]
pub struct RenderableCellExtra {
    pub zerowidth: Option<Vec<char>>,
    pub hyperlink: Option<Hyperlink>,
}

impl RenderableCell {
    fn new(content: &mut RenderableContent<'_>, cell: Indexed<&Cell>) -> Self {
        // Lookup RGB values.
        let mut fg = Self::compute_fg_rgb(content, cell.fg, cell.flags);
        let mut bg = Self::compute_bg_rgb(content, cell.bg);

        let mut bg_alpha = if cell.flags.contains(Flags::INVERSE) {
            mem::swap(&mut fg, &mut bg);
            1.0
        } else {
            Self::compute_bg_alpha(content.config, cell.bg)
        };

        let is_selected = content.terminal_content.selection.is_some_and(|selection| {
            selection.contains_cell(
                &cell,
                content.terminal_content.cursor.point,
                content.cursor_shape,
            )
        });

        let display_offset = content.terminal_content.display_offset;
        let viewport_start = Point::new(Line(-(display_offset as i32)), Column(0));
        let colors = &content.config.colors;
        let mut character = cell.c;
        let mut flags = cell.flags;

        let num_cols = content.size.columns();
        if let Some((c, is_first)) = content
            .hint
            .as_mut()
            .and_then(|hint| hint.advance(viewport_start, num_cols, cell.point))
        {
            if is_first {
                let (config_fg, config_bg) =
                    (colors.hints.start.foreground, colors.hints.start.background);
                Self::compute_cell_rgb(&mut fg, &mut bg, &mut bg_alpha, config_fg, config_bg);
            } else if c.is_some() {
                let (config_fg, config_bg) =
                    (colors.hints.end.foreground, colors.hints.end.background);
                Self::compute_cell_rgb(&mut fg, &mut bg, &mut bg_alpha, config_fg, config_bg);
            } else {
                flags.insert(Flags::UNDERLINE);
            }

            character = c.unwrap_or(character);
        } else if is_selected {
            let config_fg = colors.selection.foreground;
            let config_bg = colors.selection.background;
            Self::compute_cell_rgb(&mut fg, &mut bg, &mut bg_alpha, config_fg, config_bg);

            if fg == bg && !cell.flags.contains(Flags::HIDDEN) {
                // Reveal inversed text when fg/bg is the same.
                fg = content.color(NamedColor::Background as usize);
                bg = content.color(NamedColor::Foreground as usize);
                bg_alpha = 1.0;
            }
        } else if content.search.as_mut().is_some_and(|search| search.advance(cell.point)) {
            let focused = content.focused_match.is_some_and(|fm| fm.contains(&cell.point));
            let (config_fg, config_bg) = if focused {
                (colors.search.focused_match.foreground, colors.search.focused_match.background)
            } else {
                (colors.search.matches.foreground, colors.search.matches.background)
            };
            Self::compute_cell_rgb(&mut fg, &mut bg, &mut bg_alpha, config_fg, config_bg);
        }

        // Apply transparency to all renderable cells if `transparent_background_colors` is set
        if bg_alpha > 0. && content.config.colors.transparent_background_colors {
            bg_alpha = content.config.window_opacity();
        }

        // Convert cell point to viewport position.
        let cell_point = cell.point;
        let point = term::point_to_viewport(display_offset, cell_point).unwrap();

        let underline = cell
            .underline_color()
            .map_or(fg, |underline| Self::compute_fg_rgb(content, underline, flags));

        let zerowidth = cell.zerowidth();
        let hyperlink = cell.hyperlink();

        let extra = (zerowidth.is_some() || hyperlink.is_some()).then(|| {
            Box::new(RenderableCellExtra {
                zerowidth: zerowidth.map(|zerowidth| zerowidth.to_vec()),
                hyperlink,
            })
        });

        RenderableCell { flags, character, bg_alpha, point, fg, bg, underline, extra }
    }

    /// Check if cell contains any renderable content.
    fn is_empty(&self) -> bool {
        self.bg_alpha == 0.
            && self.character == ' '
            && self.extra.is_none()
            && !self.flags.intersects(Flags::ALL_UNDERLINES | Flags::STRIKEOUT)
    }

    /// Apply [`CellRgb`] colors to the cell's colors.
    fn compute_cell_rgb(
        cell_fg: &mut Rgb,
        cell_bg: &mut Rgb,
        bg_alpha: &mut f32,
        fg: CellRgb,
        bg: CellRgb,
    ) {
        let old_fg = mem::replace(cell_fg, fg.color(*cell_fg, *cell_bg));
        *cell_bg = bg.color(old_fg, *cell_bg);

        if bg != CellRgb::CellBackground {
            *bg_alpha = 1.0;
        }
    }

    /// Get the RGB color from a cell's foreground color.
    fn compute_fg_rgb(content: &RenderableContent<'_>, fg: Color, flags: Flags) -> Rgb {
        let config = &content.config;
        match fg {
            Color::Spec(rgb) => match flags & Flags::DIM {
                Flags::DIM => {
                    let rgb: Rgb = rgb.into();
                    rgb * DIM_FACTOR
                },
                _ => rgb.into(),
            },
            Color::Named(ansi) => {
                match (config.colors.draw_bold_text_with_bright_colors, flags & Flags::DIM_BOLD) {
                    // If no bright foreground is set, treat it like the BOLD flag doesn't exist.
                    (_, Flags::DIM_BOLD)
                        if ansi == NamedColor::Foreground
                            && config.colors.primary.bright_foreground.is_none() =>
                    {
                        content.color(NamedColor::DimForeground as usize)
                    },
                    // Draw bold text in bright colors *and* contains bold flag.
                    (true, Flags::BOLD) => content.color(ansi.to_bright() as usize),
                    // Cell is marked as dim and not bold.
                    (_, Flags::DIM) | (false, Flags::DIM_BOLD) => {
                        content.color(ansi.to_dim() as usize)
                    },
                    // None of the above, keep original color..
                    _ => content.color(ansi as usize),
                }
            },
            Color::Indexed(idx) => {
                let idx = match (
                    config.colors.draw_bold_text_with_bright_colors,
                    flags & Flags::DIM_BOLD,
                    idx,
                ) {
                    (true, Flags::BOLD, 0..=7) => idx as usize + 8,
                    (false, Flags::DIM, 8..=15) => idx as usize - 8,
                    (false, Flags::DIM, 0..=7) => NamedColor::DimBlack as usize + idx as usize,
                    _ => idx as usize,
                };

                content.color(idx)
            },
        }
    }

    /// Get the RGB color from a cell's background color.
    #[inline]
    fn compute_bg_rgb(content: &RenderableContent<'_>, bg: Color) -> Rgb {
        match bg {
            Color::Spec(rgb) => rgb.into(),
            Color::Named(ansi) => content.color(ansi as usize),
            Color::Indexed(idx) => content.color(idx as usize),
        }
    }

    /// Compute background alpha based on cell's original color.
    ///
    /// Since an RGB color matching the background should not be transparent, this is computed
    /// using the named input color, rather than checking the RGB of the background after its color
    /// is computed.
    #[inline]
    fn compute_bg_alpha(config: &UiConfig, bg: Color) -> f32 {
        if bg == Color::Named(NamedColor::Background) {
            0.
        } else if config.colors.transparent_background_colors {
            config.window_opacity()
        } else {
            1.
        }
    }
}

/// Cursor storing all information relevant for rendering.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub struct RenderableCursor {
    shape: CursorShape,
    cursor_color: Rgb,
    text_color: Rgb,
    width: NonZeroU32,
    point: Point<usize>,
}

impl RenderableCursor {
    fn new_hidden() -> Self {
        let shape = CursorShape::Hidden;
        let cursor_color = Rgb::default();
        let text_color = Rgb::default();
        let width = NonZeroU32::new(1).unwrap();
        let point = Point::default();
        Self { shape, cursor_color, text_color, width, point }
    }
}

impl RenderableCursor {
    pub fn new(
        point: Point<usize>,
        shape: CursorShape,
        cursor_color: Rgb,
        width: NonZeroU32,
    ) -> Self {
        Self { shape, cursor_color, text_color: cursor_color, width, point }
    }

    pub fn color(&self) -> Rgb {
        self.cursor_color
    }

    pub fn shape(&self) -> CursorShape {
        self.shape
    }

    pub fn width(&self) -> NonZeroU32 {
        self.width
    }

    pub fn point(&self) -> Point<usize> {
        self.point
    }
}

/// Regex hints for keyboard shortcuts.
struct Hint<'a> {
    /// Hint matches and position.
    matches: HintMatches<'a>,

    /// Last match checked against current cell position.
    labels: &'a Vec<Vec<char>>,
}

impl Hint<'_> {
    /// Advance the hint iterator.
    ///
    /// If the point is within a hint, the keyboard shortcut character that should be displayed at
    /// this position will be returned.
    ///
    /// The tuple's [`bool`] will be `true` when the character is the first for this hint.
    ///
    /// The tuple's [`Option<char>`] will be [`None`] when the point is part of the match, but not
    /// part of the hint label.
    fn advance(
        &mut self,
        viewport_start: Point,
        num_cols: usize,
        point: Point,
    ) -> Option<(Option<char>, bool)> {
        // Check if we're within a match at all.
        if !self.matches.advance(point) {
            return None;
        }

        // Match starting position on this line; linebreaks interrupt the hint labels.
        let start = self
            .matches
            .get(self.matches.index)
            .map(|bounds| cmp::max(*bounds.start(), viewport_start))?;

        // Position within the hint label.
        let line_delta = point.line.0 - start.line.0;
        let col_delta = point.column.0 as i32 - start.column.0 as i32;
        let label_position = usize::try_from(line_delta * num_cols as i32 + col_delta).unwrap_or(0);
        let is_first = label_position == 0;

        // Hint label character.
        let hint_char = self.labels[self.matches.index]
            .get(label_position)
            .copied()
            .map(|c| (Some(c), is_first))
            .unwrap_or((None, false));

        Some(hint_char)
    }
}

impl<'a> From<&'a HintState> for Hint<'a> {
    fn from(hint_state: &'a HintState) -> Self {
        let matches = HintMatches::new(hint_state.matches());
        Self { labels: hint_state.labels(), matches }
    }
}

/// Visible hint match tracking.
#[derive(Default)]
struct HintMatches<'a> {
    /// All visible matches.
    matches: Cow<'a, [Match]>,

    /// Index of the last match checked.
    index: usize,
}

impl<'a> HintMatches<'a> {
    /// Create new renderable matches iterator..
    fn new(matches: impl Into<Cow<'a, [Match]>>) -> Self {
        Self { matches: matches.into(), index: 0 }
    }

    /// Create from regex matches on term visible part.
    fn visible_regex_matches<T>(term: &Term<T>, dfas: &mut RegexSearch) -> Self {
        let matches = hint::visible_regex_match_iter(term, dfas).collect::<Vec<_>>();
        Self::new(matches)
    }

    /// Advance the regex tracker to the next point.
    ///
    /// This will return `true` if the point passed is part of a regex match.
    fn advance(&mut self, point: Point) -> bool {
        while let Some(bounds) = self.get(self.index) {
            if bounds.start() > &point {
                break;
            } else if bounds.end() < &point {
                self.index += 1;
            } else {
                return true;
            }
        }
        false
    }
}

impl Deref for HintMatches<'_> {
    type Target = [Match];

    fn deref(&self) -> &Self::Target {
        self.matches.deref()
    }
}
