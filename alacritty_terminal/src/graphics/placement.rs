use crate::graphics::command::{DisplayCmd, VirtualPlacement};

/// A placement ties an image to a position on the terminal grid.
#[derive(Debug, Clone)]
pub struct Placement {
    /// The image this placement refers to.
    pub image_id: u32,
    /// Unique placement ID (0 = auto-assigned).
    pub placement_id: u32,
    /// Where this placement is anchored.
    pub location: PlacementLocation,
    /// Source rectangle within the image (pixels).
    pub source_x: u32,
    pub source_y: u32,
    /// 0 = full image width.
    pub source_width: u32,
    /// 0 = full image height.
    pub source_height: u32,
    /// Display size in terminal cells (0 = auto-calculate).
    pub columns: u32,
    /// Display size in terminal cells (0 = auto-calculate).
    pub rows: u32,
    /// Pixel offsets within the top-left cell.
    pub x_offset: u32,
    pub y_offset: u32,
    /// Z-index for stacking order.
    pub z_index: i32,
    /// Whether this is a virtual (unicode placeholder) placement.
    pub virtual_placement: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementLocation {
    /// Pinned to a grid position. `row` is a terminal Line value (0 = top of
    /// active area, negative = scrollback). Survives scrolling into history.
    Pin { row: i32, col: usize },
    /// Virtual placement (U=1) - follows unicode placeholder characters.
    Virtual,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            image_id: 0,
            placement_id: 0,
            location: PlacementLocation::Pin { row: 0, col: 0 },
            source_x: 0,
            source_y: 0,
            source_width: 0,
            source_height: 0,
            columns: 0,
            rows: 0,
            x_offset: 0,
            y_offset: 0,
            z_index: 0,
            virtual_placement: false,
        }
    }
}

impl Placement {
    /// Build a placement from display parameters and cursor position.
    pub fn from_display(
        image_id: u32,
        display: &DisplayCmd,
        transmission_placement_id: u32,
        cursor_row: i32,
        cursor_col: usize,
        placements: &[Placement],
    ) -> Self {
        let virtual_placement = display.virtual_placement == VirtualPlacement::Yes;
        let location = if virtual_placement {
            PlacementLocation::Virtual
        } else {
            PlacementLocation::Pin { row: cursor_row, col: cursor_col }
        };

        let placement_id = resolve_placement_id(display.placement_id, transmission_placement_id, placements);

        Self {
            image_id,
            placement_id,
            location,
            source_x: 0,
            source_y: 0,
            source_width: display.width,
            source_height: display.height,
            columns: display.columns,
            rows: display.rows,
            x_offset: display.x_offset,
            y_offset: display.y_offset,
            z_index: display.z_index,
            virtual_placement,
        }
    }
}

fn resolve_placement_id(display_id: u32, transmission_id: u32, placements: &[Placement]) -> u32 {
    if display_id != 0 {
        return display_id;
    }
    if transmission_id != 0 {
        return transmission_id;
    }
    placements.iter().map(|p| p.placement_id).max().unwrap_or(0).saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::command::DisplayCmd;

    #[test]
    fn test_placement_creation() {
        let display = DisplayCmd {
            columns: 10,
            rows: 5,
            x_offset: 2,
            y_offset: 3,
            width: 100,
            height: 50,
            z_index: -1,
            placement_id: 7,
            ..Default::default()
        };

        let placement = Placement::from_display(42, &display, 0, 3, 4, &[]);
        assert_eq!(placement.image_id, 42);
        assert_eq!(placement.placement_id, 7);
        assert_eq!(placement.columns, 10);
        assert_eq!(placement.rows, 5);
        assert_eq!(placement.x_offset, 2);
        assert_eq!(placement.y_offset, 3);
        assert_eq!(placement.source_width, 100);
        assert_eq!(placement.source_height, 50);
        assert_eq!(placement.z_index, -1);
        assert_eq!(placement.location, PlacementLocation::Pin { row: 3, col: 4 });
        assert!(!placement.virtual_placement);
    }

    #[test]
    fn test_placement_default() {
        let placement = Placement::default();
        assert_eq!(placement.image_id, 0);
        assert_eq!(placement.placement_id, 0);
        assert_eq!(placement.source_width, 0);
        assert_eq!(placement.columns, 0);
        assert_eq!(placement.z_index, 0);
        assert_eq!(placement.location, PlacementLocation::Pin { row: 0, col: 0 });
        assert!(!placement.virtual_placement);
    }
}
