//! Unicode placeholder (U+10EEEE) support for Kitty virtual placements.

/// The Unicode placeholder character used by Kitty Graphics Protocol.
pub const PLACEHOLDER_CHAR: char = '\u{10EEEE}';

/// Diacritics base for row/column encoding.
/// Row and column indices are encoded as combining diacritics starting from U+0305.
const DIACRITICS_BASE: u32 = 0x0305;

/// Extract image_id from the foreground color of a placeholder cell.
/// The image_id is encoded in the lower 24 bits of the foreground color.
pub fn image_id_from_fg_color(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Extract placement_id from the underline color of a placeholder cell.
/// Returns 0 if no underline color is set.
pub fn placement_id_from_underline_color(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Extract row index from a combining diacritic character.
/// Returns None if the character is not a valid row/column diacritic.
pub fn fragment_index_from_diacritic(c: char) -> Option<u32> {
    let cp = c as u32;
    if cp >= DIACRITICS_BASE {
        Some(cp - DIACRITICS_BASE)
    } else {
        None
    }
}

/// Check if a character is the Kitty graphics placeholder.
pub fn is_placeholder(c: char) -> bool {
    c == PLACEHOLDER_CHAR
}

/// Represents a single placeholder cell found during grid scanning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceholderCell {
    /// The image ID extracted from foreground color.
    pub image_id: u32,
    /// The placement ID extracted from underline color (0 if not set).
    pub placement_id: u32,
    /// Row fragment index within the image.
    pub row: u32,
    /// Column fragment index within the image.
    pub col: u32,
    /// Grid row where this placeholder was found (negative = scrollback).
    pub grid_row: i32,
    /// Grid column where this placeholder was found.
    pub grid_col: usize,
}

/// Information about a virtual placement gathered from scanning placeholder cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualPlacementInfo {
    pub image_id: u32,
    pub placement_id: u32,
    /// Top-left grid row of the placement (negative = scrollback).
    pub start_row: i32,
    pub start_col: usize,
    /// Number of rows and columns occupied by placeholder cells.
    pub rows: usize,
    pub cols: usize,
}

impl VirtualPlacementInfo {
    /// Build virtual placement info from a collection of placeholder cells
    /// that share the same image_id and placement_id.
    pub fn from_cells(cells: &[PlaceholderCell]) -> Option<Self> {
        if cells.is_empty() {
            return None;
        }
        let image_id = cells[0].image_id;
        let placement_id = cells[0].placement_id;
        let start_row = cells.iter().map(|c| c.grid_row).min().unwrap();
        let start_col = cells.iter().map(|c| c.grid_col).min().unwrap();
        let end_row = cells.iter().map(|c| c.grid_row).max().unwrap();
        let end_col = cells.iter().map(|c| c.grid_col).max().unwrap();
        Some(Self {
            image_id,
            placement_id,
            start_row,
            start_col,
            rows: (end_row - start_row + 1) as usize,
            cols: end_col - start_col + 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_placeholder() {
        assert!(is_placeholder(PLACEHOLDER_CHAR));
        assert!(!is_placeholder('a'));
        assert!(!is_placeholder('\u{0041}'));
        assert!(!is_placeholder('\u{10EEED}'));
        assert!(!is_placeholder('\u{10EEEF}'));
    }

    #[test]
    fn test_image_id_from_fg_color() {
        assert_eq!(image_id_from_fg_color(0, 0, 0), 0);
        assert_eq!(image_id_from_fg_color(1, 2, 3), 0x010203);
        assert_eq!(image_id_from_fg_color(255, 0, 128), 0xFF0080);
    }

    #[test]
    fn test_placement_id_from_underline_color() {
        assert_eq!(placement_id_from_underline_color(0, 0, 0), 0);
        assert_eq!(placement_id_from_underline_color(0, 0, 42), 42);
        assert_eq!(placement_id_from_underline_color(10, 20, 30), 0x0A141E);
    }

    #[test]
    fn test_fragment_index_from_diacritic() {
        assert_eq!(fragment_index_from_diacritic('\u{0305}'), Some(0));
        assert_eq!(fragment_index_from_diacritic('\u{0306}'), Some(1));
        assert_eq!(fragment_index_from_diacritic('\u{0304}'), None);
        assert_eq!(fragment_index_from_diacritic('a'), None);
    }

    #[test]
    fn test_virtual_placement_from_cells() {
        let cells = vec![
            PlaceholderCell {
                image_id: 1,
                placement_id: 2,
                row: 0,
                col: 0,
                grid_row: 5,
                grid_col: 10,
            },
            PlaceholderCell {
                image_id: 1,
                placement_id: 2,
                row: 0,
                col: 1,
                grid_row: 5,
                grid_col: 11,
            },
            PlaceholderCell {
                image_id: 1,
                placement_id: 2,
                row: 1,
                col: 0,
                grid_row: 6,
                grid_col: 10,
            },
        ];
        let info = VirtualPlacementInfo::from_cells(&cells).unwrap();
        assert_eq!(info.image_id, 1);
        assert_eq!(info.placement_id, 2);
        assert_eq!(info.start_row, 5);
        assert_eq!(info.start_col, 10);
        assert_eq!(info.rows, 2);
        assert_eq!(info.cols, 2);
    }

    #[test]
    fn test_virtual_placement_from_empty_cells() {
        assert!(VirtualPlacementInfo::from_cells(&[]).is_none());
    }

    #[test]
    fn test_virtual_placement_in_scrollback() {
        // Placeholders found while the view is scrolled into history have
        // negative grid rows; the extent math must not wrap.
        let cells = vec![
            PlaceholderCell {
                image_id: 1,
                placement_id: 0,
                row: 0,
                col: 0,
                grid_row: -3,
                grid_col: 2,
            },
            PlaceholderCell {
                image_id: 1,
                placement_id: 0,
                row: 1,
                col: 0,
                grid_row: -2,
                grid_col: 2,
            },
        ];
        let info = VirtualPlacementInfo::from_cells(&cells).unwrap();
        assert_eq!(info.start_row, -3);
        assert_eq!(info.rows, 2);
        assert_eq!(info.cols, 1);
    }

    #[test]
    fn test_image_id_specific_values() {
        assert_eq!(image_id_from_fg_color(0, 0, 1), 1);
        assert_eq!(image_id_from_fg_color(255, 255, 255), 16_777_215);
    }

    #[test]
    fn test_diacritics_range() {
        for i in 0..100u32 {
            let c = char::from_u32(DIACRITICS_BASE + i).unwrap();
            assert_eq!(fragment_index_from_diacritic(c), Some(i));
        }
    }
}
