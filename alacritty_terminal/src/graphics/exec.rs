use crate::graphics::command::{
    Action, Command, CursorMovement, DeleteAction, DisplayCmd, Medium, Quiet, Response,
    Transmission,
};
use crate::graphics::image::{Image, ImageError, LoadingImage, PendingDisplay};
use crate::graphics::placement::{Placement, PlacementLocation};
use crate::graphics::storage::{ImageStorage, StorageError};

/// Result of executing a graphics command.
pub struct ExecResult {
    /// Response to send back via PTY (may be None if quiet).
    pub response: Option<Vec<u8>>,
    /// Number of rows the cursor should move down after displaying an image.
    /// Non-zero only for display actions with `CursorMovement::Move` (the default).
    pub cursor_rows_to_move: usize,
}

/// Execute a Kitty Graphics command.
pub fn execute(
    cmd: &Command,
    storage: &mut ImageStorage,
    placements: &mut Vec<Placement>,
    loading: &mut Option<LoadingImage>,
    cursor_row: i32,
    cursor_col: usize,
) -> ExecResult {
    if let Some(msg) = validate_identifiers(cmd) {
        return finish(cmd.quiet, error_response(cmd, msg));
    }

    match cmd.action {
        Action::Query => execute_query(cmd),
        Action::Transmit => execute_transmit(cmd, storage, placements, loading, None),
        Action::TransmitAndDisplay => execute_transmit(
            cmd,
            storage,
            placements,
            loading,
            Some((cursor_row, cursor_col)),
        ),
        Action::Display => execute_display(cmd, storage, placements, cursor_row, cursor_col),
        Action::Delete => execute_delete(cmd, storage, placements, loading, cursor_row, cursor_col),
        Action::TransmitAnimationFrame | Action::ControlAnimation | Action::ComposeAnimation => {
            finish(cmd.quiet, error_response(cmd, "ENOTSUP:animation not yet implemented"))
        },
    }
}

fn validate_identifiers(cmd: &Command) -> Option<&'static str> {
    let (image_id, image_number) = match cmd.action {
        Action::Display | Action::Delete => {
            (cmd.transmission.image_id, cmd.transmission.image_number)
        },
        Action::Query | Action::Transmit | Action::TransmitAndDisplay => {
            (cmd.transmission.image_id, cmd.transmission.image_number)
        },
        _ => return None,
    };

    if image_id > 0 && image_number > 0 {
        Some("EINVAL: image ID and number are mutually exclusive")
    } else {
        None
    }
}

fn execute_query(cmd: &Command) -> ExecResult {
    let image_id = cmd.transmission.image_id;
    match try_complete_image(cmd) {
        Ok(()) => finish(cmd.quiet, Response::ok(image_id)),
        Err(err) => finish(cmd.quiet, error_response_id(image_id, err)),
    }
}

fn execute_transmit(
    cmd: &Command,
    storage: &mut ImageStorage,
    placements: &mut Vec<Placement>,
    loading: &mut Option<LoadingImage>,
    display_at: Option<(i32, usize)>,
) -> ExecResult {
    let quiet = cmd.quiet;
    let initial_id = cmd.transmission.image_id;
    let initial_placement = cmd.transmission.placement_id;

    // Grab the saved display intent before load_and_store consumes the
    // LoadingImage (on the final chunk, take() moves it out).
    let saved_display = loading.as_ref().and_then(|li| li.pending_display.clone());

    match load_and_store(cmd, storage, placements, loading) {
        Ok(LoadOutcome { image, more, implicit_id }) => {
            if more {
                // First chunk of a multi-part TransmitAndDisplay: save the
                // display intent so it survives across continuation chunks
                // (which default to Action::Transmit and lose display_at).
                if let (Some(li), Some((row, col))) = (loading, display_at) {
                    if li.pending_display.is_some() {
                        return ExecResult { response: None, cursor_rows_to_move: 0 };
                    }
                    li.pending_display = Some(PendingDisplay {
                        cursor_row: row,
                        cursor_col: col,
                        display: cmd.display.clone(),
                        placement_id: initial_placement,
                        quiet,
                    });
                }
                return ExecResult { response: None, cursor_rows_to_move: 0 };
            }

            // Create placement if there is a display intent — either from a
            // single-chunk TransmitAndDisplay or saved from the first chunk
            // of a multi-part upload.
            let mut cursor_rows = 0usize;
            let mut result_placement_id = 0u32;

            if let Some((row, col)) = display_at {
                let placement = add_placement(
                    storage,
                    placements,
                    &image,
                    &cmd.display,
                    initial_placement,
                    row,
                    col,
                );
                result_placement_id = placement.placement_id;
                if cmd.display.cursor_movement == CursorMovement::Move {
                    cursor_rows = cursor_advance_rows(&image, &placement);
                }
            } else if let Some(ref pd) = saved_display {
                let placement = add_placement(
                    storage,
                    placements,
                    &image,
                    &pd.display,
                    pd.placement_id,
                    pd.cursor_row,
                    pd.cursor_col,
                );
                result_placement_id = placement.placement_id;
                if pd.display.cursor_movement == CursorMovement::Move {
                    cursor_rows = cursor_advance_rows(&image, &placement);
                }
            }

            // Suppress PTY response for implicit IDs (auto-assigned when
            // the client didn't send `i=`).  The placement was already
            // created above — only the response is suppressed.
            if implicit_id {
                return ExecResult { response: None, cursor_rows_to_move: cursor_rows };
            }

            let effective_quiet = saved_display.as_ref().map(|pd| pd.quiet).unwrap_or(quiet);
            let mut response = Response::ok(image.id);
            response.image_number = image.image_number;
            response.placement_id = result_placement_id;
            finish_with_cursor(effective_quiet, response, cursor_rows)
        },
        Err(err) => {
            let id = loading.as_ref().map(|l| l.id).unwrap_or(initial_id);
            finish(quiet, error_response_id(id, err))
        },
    }
}

fn execute_display(
    cmd: &Command,
    storage: &mut ImageStorage,
    placements: &mut Vec<Placement>,
    cursor_row: i32,
    cursor_col: usize,
) -> ExecResult {
    let t = &cmd.transmission;
    if t.image_id == 0 && t.image_number == 0 {
        return finish(cmd.quiet, Response::error(0, "EINVAL: image ID or number required"));
    }

    let image = if t.image_id != 0 {
        storage.get_image(t.image_id).cloned()
    } else {
        storage.get_image_by_number(t.image_number).cloned()
    };

    let Some(image) = image else {
        let mut response = Response::error(t.image_id, "ENOENT: image not found");
        if t.image_id == 0 && t.image_number != 0 {
            response.image_number = t.image_number;
        }
        return finish(cmd.quiet, response);
    };

    let placement = add_placement(
        storage,
        placements,
        &image,
        &cmd.display,
        t.placement_id,
        cursor_row,
        cursor_col,
    );

    let mut cursor_rows = 0;
    if cmd.display.cursor_movement == CursorMovement::Move {
        cursor_rows = cursor_advance_rows(&image, &placement);
    }

    let mut response = Response::ok(image.id);
    response.image_number = image.image_number;
    response.placement_id = placement.placement_id;
    finish_with_cursor(cmd.quiet, response, cursor_rows)
}

fn execute_delete(
    cmd: &Command,
    storage: &mut ImageStorage,
    placements: &mut Vec<Placement>,
    loading: &mut Option<LoadingImage>,
    cursor_row: i32,
    cursor_col: usize,
) -> ExecResult {
    *loading = None;
    delete_placements(&cmd.delete, placements, storage, cursor_row, cursor_col);
    storage.delete(&cmd.delete);

    // Respond only when the client supplied an identifier to match the
    // response against (mirrors kitty, which never answers anonymous deletes).
    let t = &cmd.transmission;
    if t.image_id == 0 && t.image_number == 0 {
        return ExecResult { response: None, cursor_rows_to_move: 0 };
    }
    let mut response = Response::ok(t.image_id);
    response.image_number = t.image_number;
    finish(cmd.quiet, response)
}

struct LoadOutcome {
    image: Image,
    more: bool,
    implicit_id: bool,
}

fn load_and_store(
    cmd: &Command,
    storage: &mut ImageStorage,
    placements: &mut Vec<Placement>,
    loading: &mut Option<LoadingImage>,
) -> Result<LoadOutcome, ExecError> {
    let t = &cmd.transmission;

    if let Some(ref current) = *loading {
        if t.image_id != 0 && current.id != t.image_id {
            *loading = None;
        }
    }

    if let Some(existing) = loading.take() {
        let mut current = existing;
        current.add_data(&cmd.data).map_err(ExecError::Image)?;
        if t.more_chunks {
            let implicit_id = current.implicit_id;
            let id = current.id;
            *loading = Some(current);
            return Ok(LoadOutcome {
                image: Image {
                    id,
                    width: t.width,
                    height: t.height,
                    data: Vec::new(),
                    image_number: t.image_number,
                    implicit_id,
                },
                more: true,
                implicit_id,
            });
        }
        let implicit_id = current.implicit_id;
        let image = current.complete().map_err(ExecError::Image)?;
        storage.add_image(image.clone()).map_err(ExecError::Storage)?;
        return Ok(LoadOutcome { image, more: false, implicit_id });
    }

    if t.image_id > 0 {
        remove_placements_for_image(placements, storage, t.image_id);
        storage.delete(&DeleteAction::ById { image_id: t.image_id, delete_data: true });
    }

    let current = start_loading(cmd, storage)?;
    if t.more_chunks {
        let implicit_id = current.implicit_id;
        let id = current.id;
        *loading = Some(current);
        return Ok(LoadOutcome {
            image: Image {
                id,
                width: t.width,
                height: t.height,
                data: Vec::new(),
                image_number: t.image_number,
                implicit_id,
            },
            more: true,
            implicit_id,
        });
    }

    let implicit_id = current.implicit_id;
    let image = current.complete().map_err(ExecError::Image)?;
    storage.add_image(image.clone()).map_err(ExecError::Storage)?;
    Ok(LoadOutcome { image, more: false, implicit_id })
}

fn start_loading(cmd: &Command, storage: &mut ImageStorage) -> Result<LoadingImage, ExecError> {
    let t = &cmd.transmission;
    if t.medium != Medium::Direct {
        return Err(ExecError::Message("EINVAL: unsupported transmission medium"));
    }

    let (id, implicit_id) = resolve_image_id(t, storage);
    let mut loading = LoadingImage::with_number(
        id,
        t.width,
        t.height,
        t.format,
        t.compression,
        implicit_id,
        t.image_number,
    );
    loading.add_data(&cmd.data).map_err(ExecError::Image)?;
    Ok(loading)
}

fn try_complete_image(cmd: &Command) -> Result<(), ExecError> {
    if cmd.transmission.medium != Medium::Direct {
        return Err(ExecError::Message("EINVAL: unsupported transmission medium"));
    }
    let t = &cmd.transmission;
    let mut loading = LoadingImage::with_number(
        t.image_id.max(1),
        t.width,
        t.height,
        t.format,
        t.compression,
        false,
        t.image_number,
    );
    loading.add_data(&cmd.data).map_err(ExecError::Image)?;
    let image = loading.complete().map_err(ExecError::Image)?;
    drop(image);
    Ok(())
}

fn resolve_image_id(t: &Transmission, storage: &mut ImageStorage) -> (u32, bool) {
    if t.image_id > 0 {
        return (t.image_id, false);
    }
    if t.image_number > 0 {
        return (storage.next_image_id(), false);
    }
    (storage.next_image_id(), true)
}

fn add_placement(
    storage: &mut ImageStorage,
    placements: &mut Vec<Placement>,
    image: &Image,
    display: &DisplayCmd,
    transmission_placement_id: u32,
    cursor_row: i32,
    cursor_col: usize,
) -> Placement {
    let placement = Placement::from_display(
        image.id,
        display,
        transmission_placement_id,
        cursor_row,
        cursor_col,
        placements,
    );
    // The (image_id, placement_id) pair uniquely identifies a placement:
    // re-displaying it replaces the previous one (kitty uses this for
    // flicker-free moves/resizes).
    remove_placements_matching(placements, storage, |p| {
        p.image_id == placement.image_id && p.placement_id == placement.placement_id
    });
    storage.add_reference(image.id);
    placements.push(placement.clone());
    placement
}

fn delete_placements(
    action: &DeleteAction,
    placements: &mut Vec<Placement>,
    storage: &mut ImageStorage,
    cursor_row: i32,
    cursor_col: usize,
) {
    match action {
        DeleteAction::All { .. } => {
            for placement in placements.drain(..) {
                storage.remove_reference(placement.image_id);
            }
        },
        DeleteAction::ById { image_id, .. } => {
            remove_placements_for_image(placements, storage, *image_id);
        },
        DeleteAction::ByNumber { image_number, .. } => {
            if let Some(id) = storage.get_image_by_number(*image_number).map(|img| img.id) {
                remove_placements_for_image(placements, storage, id);
            }
        },
        DeleteAction::AtCursor { .. } => {
            placements.retain(|p| {
                let remove = placement_intersects_cell(p, cursor_row, cursor_col as u32);
                if remove {
                    storage.remove_reference(p.image_id);
                }
                !remove
            });
        },
        DeleteAction::AtPosition { x, y, z, delete_data } => {
            // The cell coordinate in d=p is 1-based (top-left cell is x=1, y=1).
            let removed = match (x.checked_sub(1), y.checked_sub(1)) {
                (Some(col), Some(row)) => remove_placements_matching(placements, storage, |p| {
                    (*z == 0 || p.z_index == *z)
                        && placement_intersects_cell(p, row as i32, col)
                }),
                _ => Vec::new(),
            };
            if *delete_data {
                delete_data_for_images(storage, &removed, action);
            }
        },
        DeleteAction::ByZIndex { z, delete_data } => {
            let removed = remove_placements_matching(placements, storage, |p| p.z_index == *z);
            if *delete_data {
                delete_data_for_images(storage, &removed, action);
            }
        },
        DeleteAction::InColumn { x, delete_data } => {
            let removed = remove_placements_matching(placements, storage, |p| {
                matches!(p.location, PlacementLocation::Pin { col, .. } if col == *x as usize)

            });
            if *delete_data {
                delete_data_for_images(storage, &removed, action);
            }
        },
        DeleteAction::InRow { y, delete_data } => {
            let removed = remove_placements_matching(placements, storage, |p| {
                matches!(p.location, PlacementLocation::Pin { row, .. } if row == *y as i32)
            });
            if *delete_data {
                delete_data_for_images(storage, &removed, action);
            }
        },
        DeleteAction::AnimationFrames { image_id } => {
            remove_placements_for_image(placements, storage, *image_id);
        },
        DeleteAction::ByRange { image_id, placement_id, delete_data } => {
            let removed = remove_placements_matching(placements, storage, |p| {
                p.image_id == *image_id && p.placement_id == *placement_id
            });
            if *delete_data {
                delete_data_for_images(storage, &removed, action);
            }
        },
    }
}

/// Whether a placement overlaps the given cell (0-based).
///
/// ponytail: auto-sized placements (rows/columns == 0) are treated as a
/// single cell — the terminal crate has no cell pixel metrics to resolve
/// the true extent. Pass cell dims in if this ever under-deletes.
fn placement_intersects_cell(p: &Placement, row: i32, col: u32) -> bool {
    let PlacementLocation::Pin { row: pr, col: pc } = p.location else {
        return false;
    };
    let rows = p.rows.max(1) as i32;
    let cols = p.columns.max(1);
    row >= pr && row < pr + rows && col >= pc as u32 && col < pc as u32 + cols
}

fn remove_placements_matching<F>(
    placements: &mut Vec<Placement>,
    storage: &mut ImageStorage,
    mut matches: F,
) -> Vec<u32>
where
    F: FnMut(&Placement) -> bool,
{
    let mut removed_image_ids = Vec::new();
    placements.retain(|p| {
        if matches(p) {
            storage.remove_reference(p.image_id);
            removed_image_ids.push(p.image_id);
            false
        } else {
            true
        }
    });
    removed_image_ids
}

fn delete_data_for_images(storage: &mut ImageStorage, removed_image_ids: &[u32], action: &DeleteAction) {
    let mut seen = Vec::new();
    for &image_id in removed_image_ids {
        if seen.contains(&image_id) {
            continue;
        }
        seen.push(image_id);
        storage.delete(&match action {
            DeleteAction::ByRange { delete_data, .. } => {
                DeleteAction::ById { image_id, delete_data: *delete_data }
            },
            DeleteAction::AtPosition { delete_data, .. }
            | DeleteAction::ByZIndex { delete_data, .. }
            | DeleteAction::InColumn { delete_data, .. }
            | DeleteAction::InRow { delete_data, .. } => DeleteAction::ById { image_id, delete_data: *delete_data },
            _ => DeleteAction::ById { image_id, delete_data: true },
        });
    }
}

fn remove_placements_for_image(placements: &mut Vec<Placement>, storage: &mut ImageStorage, image_id: u32) {
    placements.retain(|p| {
        if p.image_id == image_id {
            storage.remove_reference(p.image_id);
            false
        } else {
            true
        }
    });
}

#[derive(Debug)]
enum ExecError {
    Image(ImageError),
    Storage(StorageError),
    Message(&'static str),
}

impl From<ImageError> for ExecError {
    fn from(value: ImageError) -> Self {
        Self::Image(value)
    }
}

impl From<StorageError> for ExecError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

fn error_response(cmd: &Command, message: &'static str) -> Response {
    Response {
        image_id: cmd.transmission.image_id,
        image_number: cmd.transmission.image_number,
        placement_id: cmd.transmission.placement_id,
        message: message.into(),
    }
}

fn error_response_id(image_id: u32, err: ExecError) -> Response {
    Response::error(image_id, exec_error_message(&err))
}

fn exec_error_message(err: &ExecError) -> String {
    match err {
        ExecError::Message(msg) => (*msg).into(),
        ExecError::Storage(StorageError::ImageTooLarge) => "ENOMEM: image too large".into(),
        ExecError::Image(image_err) => match image_err {
            ImageError::EmptyData => "ENODATA: insufficient data".into(),
            ImageError::DecompressError(_) => "EINVAL: decompression failed".into(),
            ImageError::PngDecodeError(_) => "EINVAL: invalid data".into(),
            ImageError::DimensionMismatch { .. } => "EINVAL: invalid data".into(),
            ImageError::DataTooLarge => "EINVAL: data too large".into(),
        },
    }
}

/// Calculate how many rows the cursor should advance after displaying an image.
///
/// Uses `placement.rows` if explicitly set (from `r=`), otherwise falls back
/// to 1 (the image occupies at least the cursor row).
fn cursor_advance_rows(image: &Image, placement: &Placement) -> usize {
    if placement.rows > 0 {
        placement.rows as usize
    } else if image.height > 0 {
        // Without explicit rows, the image auto-sizes.  We don't know the
        // cell height here, so conservatively advance by 1 row.  Clients
        // like chafa always set `r=` explicitly.
        1
    } else {
        1
    }
}

fn finish(quiet: Quiet, response: Response) -> ExecResult {
    ExecResult { response: maybe_encode(quiet, &response), cursor_rows_to_move: 0 }
}

fn finish_with_cursor(quiet: Quiet, response: Response, cursor_rows: usize) -> ExecResult {
    ExecResult { response: maybe_encode(quiet, &response), cursor_rows_to_move: cursor_rows }
}

fn maybe_encode(quiet: Quiet, response: &Response) -> Option<Vec<u8>> {
    match quiet {
        Quiet::No => Some(response.encode()),
        Quiet::SuppressOk if response.is_ok() => None,
        Quiet::SuppressOk => Some(response.encode()),
        Quiet::SuppressAll => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::command::{DisplayCmd, Format, Transmission};

    fn rgba_1x1() -> Vec<u8> {
        vec![255, 0, 0, 255]
    }

    fn transmit_cmd(id: u32, data: &[u8], more: bool) -> Command {
        Command {
            action: Action::Transmit,
            transmission: Transmission {
                format: Format::Rgba,
                width: 1,
                height: 1,
                image_id: id,
                more_chunks: more,
                ..Default::default()
            },
            data: data.to_vec(),
            ..Default::default()
        }
    }

    #[test]
    fn test_execute_query_ok() {
        let mut cmd = transmit_cmd(5, &rgba_1x1(), false);
        cmd.action = Action::Query;
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        let result = execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(result.response.is_some());
        assert!(String::from_utf8_lossy(result.response.as_ref().unwrap()).contains("OK"));
        assert!(storage.get_image(5).is_none());
    }

    #[test]
    fn test_execute_query_error() {
        let mut cmd = transmit_cmd(5, &[1, 2], false);
        cmd.action = Action::Query;
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        let result = execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(result.response.is_some());
        assert!(String::from_utf8_lossy(result.response.as_ref().unwrap()).contains("EINVAL"));
    }

    #[test]
    fn test_execute_transmit() {
        let cmd = transmit_cmd(10, &rgba_1x1(), false);
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        let result = execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(result.response.is_some());
        assert!(storage.get_image(10).is_some());
    }

    #[test]
    fn test_execute_transmit_chunked() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;

        let first = transmit_cmd(11, &[255, 0, 0], true);
        let r1 = execute(&first, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(r1.response.is_none());
        assert!(loading.is_some());
        assert!(storage.get_image(11).is_none());

        let second = transmit_cmd(11, &[255], false);
        let r2 = execute(&second, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(r2.response.is_some());
        assert!(loading.is_none());
        assert!(storage.get_image(11).is_some());
    }

    #[test]
    fn test_execute_transmit_auto_id() {
        let cmd = Command {
            action: Action::Transmit,
            transmission: Transmission {
                format: Format::Rgba,
                width: 1,
                height: 1,
                ..Default::default()
            },
            data: rgba_1x1(),
            ..Default::default()
        };
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        assert_eq!(storage.total_bytes(), 4);
        assert!(loading.is_none());
    }

    #[test]
    fn test_execute_transmit_and_display() {
        let mut cmd = transmit_cmd(20, &rgba_1x1(), false);
        cmd.action = Action::TransmitAndDisplay;
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        execute(&cmd, &mut storage, &mut placements, &mut loading, 2, 3);
        assert!(storage.get_image(20).is_some());
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].image_id, 20);
        assert_eq!(placements[0].location, PlacementLocation::Pin { row: 2, col: 3 });
    }

    #[test]
    fn test_execute_display() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        execute(&transmit_cmd(30, &rgba_1x1(), false), &mut storage, &mut placements, &mut loading, 0, 0);

        let cmd = Command {
            action: Action::Display,
            transmission: Transmission { image_id: 30, ..Default::default() },
            ..Default::default()
        };
        execute(&cmd, &mut storage, &mut placements, &mut loading, 1, 2);
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].location, PlacementLocation::Pin { row: 1, col: 2 });
    }

    #[test]
    fn test_execute_delete_all() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        execute(&transmit_cmd(1, &rgba_1x1(), false), &mut storage, &mut placements, &mut loading, 0, 0);

        let mut cmd = transmit_cmd(1, &rgba_1x1(), false);
        cmd.action = Action::TransmitAndDisplay;
        execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);

        let delete = Command {
            action: Action::Delete,
            delete: DeleteAction::All { delete_data: true },
            ..Default::default()
        };
        execute(&delete, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(placements.is_empty());
        assert!(storage.get_image(1).is_none());
    }

    #[test]
    fn test_execute_delete_by_id() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        execute(&transmit_cmd(40, &rgba_1x1(), false), &mut storage, &mut placements, &mut loading, 0, 0);

        let delete = Command {
            action: Action::Delete,
            delete: DeleteAction::ById { image_id: 40, delete_data: true },
            ..Default::default()
        };
        execute(&delete, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(storage.get_image(40).is_none());
    }

    #[test]
    fn test_execute_quiet_suppress_ok() {
        let mut cmd = transmit_cmd(50, &rgba_1x1(), false);
        cmd.quiet = Quiet::SuppressOk;
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        let result = execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(result.response.is_none());
        assert!(storage.get_image(50).is_some());

        cmd.data = vec![0];
        cmd.quiet = Quiet::SuppressOk;
        cmd.action = Action::Query;
        let bad = execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(bad.response.is_some());
    }

    #[test]
    fn test_execute_quiet_suppress_all() {
        let mut cmd = transmit_cmd(60, &rgba_1x1(), false);
        cmd.quiet = Quiet::SuppressAll;
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        let ok = execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(ok.response.is_none());

        cmd.action = Action::Query;
        cmd.data = vec![1];
        let err = execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(err.response.is_none());
    }

    #[test]
    fn test_execute_display_not_found() {
        let cmd = Command {
            action: Action::Display,
            transmission: Transmission { image_id: 999, ..Default::default() },
            ..Default::default()
        };
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        let result = execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        let response = result.response.expect("expected ENOENT response");
        let text = String::from_utf8_lossy(&response);
        assert!(text.contains("ENOENT"));
        assert!(text.contains("i=999"));
    }

    fn display_image(id: u32, z_index: i32) -> Command {
        Command {
            action: Action::Display,
            transmission: Transmission { image_id: id, ..Default::default() },
            display: DisplayCmd { z_index, ..Default::default() },
            ..Default::default()
        }
    }

    #[test]
    fn test_execute_delete_by_z_index() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        execute(&transmit_cmd(1, &rgba_1x1(), false), &mut storage, &mut placements, &mut loading, 0, 0);
        execute(&transmit_cmd(2, &rgba_1x1(), false), &mut storage, &mut placements, &mut loading, 0, 0);

        execute(&display_image(1, 1), &mut storage, &mut placements, &mut loading, 0, 0);
        execute(&display_image(2, 2), &mut storage, &mut placements, &mut loading, 1, 0);
        assert_eq!(placements.len(), 2);

        let delete = Command {
            action: Action::Delete,
            delete: DeleteAction::ByZIndex { z: 1, delete_data: false },
            ..Default::default()
        };
        execute(&delete, &mut storage, &mut placements, &mut loading, 0, 0);
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].z_index, 2);
    }

    #[test]
    fn test_execute_delete_in_column() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        execute(&transmit_cmd(1, &rgba_1x1(), false), &mut storage, &mut placements, &mut loading, 0, 0);
        execute(&transmit_cmd(2, &rgba_1x1(), false), &mut storage, &mut placements, &mut loading, 0, 0);

        execute(&display_image(1, 0), &mut storage, &mut placements, &mut loading, 0, 0);
        execute(&display_image(2, 0), &mut storage, &mut placements, &mut loading, 0, 1);
        assert_eq!(placements.len(), 2);

        let delete = Command {
            action: Action::Delete,
            delete: DeleteAction::InColumn { x: 0, delete_data: false },
            ..Default::default()
        };
        execute(&delete, &mut storage, &mut placements, &mut loading, 0, 0);
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].image_id, 2);
    }

    #[test]
    fn test_execute_animation_stub_error() {
        for action in [
            Action::TransmitAnimationFrame,
            Action::ControlAnimation,
            Action::ComposeAnimation,
        ] {
            let cmd = Command { action, ..Default::default() };
            let mut storage = ImageStorage::new();
            let mut placements = Vec::new();
            let mut loading = None;
            let result = execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
            let response = result.response.expect("expected animation error response");
            let text = String::from_utf8_lossy(&response);
            assert!(text.contains("ENOTSUP"));
            assert!(text.contains("animation not yet implemented"));
        }
    }

    // --- Regression tests for chafa/chunked display bugs ---

    fn transmit_and_display_cmd(id: u32, data: &[u8], more: bool, rows: u32, cols: u32) -> Command {
        Command {
            action: Action::TransmitAndDisplay,
            transmission: Transmission {
                format: Format::Rgba,
                width: 1,
                height: 1,
                image_id: id,
                more_chunks: more,
                ..Default::default()
            },
            display: DisplayCmd { rows, columns: cols, ..Default::default() },
            data: data.to_vec(),
            ..Default::default()
        }
    }

    fn continuation_chunk(data: &[u8], more: bool) -> Command {
        Command {
            action: Action::Transmit,
            transmission: Transmission {
                format: Format::Rgba,
                more_chunks: more,
                ..Default::default()
            },
            data: data.to_vec(),
            ..Default::default()
        }
    }

    /// Bug A: Multi-chunk `a=T` loses display intent on continuation chunks.
    /// The first chunk starts with `a=T` but continuations default to `a=t`
    /// (Transmit), so `display_at` is None.  The saved `pending_display`
    /// must be used to create the placement when the final chunk completes.
    #[test]
    fn test_chunked_transmit_and_display_creates_placement() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;

        // First chunk: a=T, m=1, explicit id, r=3, c=5.
        let first = transmit_and_display_cmd(70, &[255, 0], true, 3, 5);
        let r1 = execute(&first, &mut storage, &mut placements, &mut loading, 2, 4);
        assert!(r1.response.is_none());
        assert!(loading.is_some());
        assert!(loading.as_ref().unwrap().pending_display.is_some());

        // Continuation chunk: default action (Transmit), m=1.
        let mid = continuation_chunk(&[0, 255], true);
        execute(&mid, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(loading.is_some());

        // Final chunk: default action (Transmit), m=0.
        let last = continuation_chunk(&[], false);
        execute(&last, &mut storage, &mut placements, &mut loading, 0, 0);

        assert!(loading.is_none());
        assert_eq!(placements.len(), 1, "placement must be created from saved display intent");
        assert_eq!(placements[0].image_id, 70);
        assert_eq!(placements[0].rows, 3);
        assert_eq!(placements[0].columns, 5);
        assert_eq!(placements[0].location, PlacementLocation::Pin { row: 2, col: 4 });
    }

    /// Bug B: `implicit_id` early return prevented placement creation.
    /// chafa does not send `i=`, so the image gets an auto-assigned ID
    /// with `implicit_id=true`.  Placement must still be created.
    #[test]
    fn test_chunked_implicit_id_creates_placement() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;

        // First chunk: a=T, no i= (implicit id), m=1.
        let first = Command {
            action: Action::TransmitAndDisplay,
            transmission: Transmission {
                format: Format::Rgba,
                width: 1,
                height: 1,
                more_chunks: true,
                ..Default::default()
            },
            display: DisplayCmd { rows: 2, columns: 3, ..Default::default() },
            data: vec![255, 0],
            ..Default::default()
        };
        let r1 = execute(&first, &mut storage, &mut placements, &mut loading, 5, 0);
        assert!(r1.response.is_none());
        assert!(loading.is_some());

        // Final chunk.
        let last = continuation_chunk(&[0, 255], false);
        let r2 = execute(&last, &mut storage, &mut placements, &mut loading, 0, 0);
        // implicit_id suppresses PTY response but must NOT skip placement.
        assert!(r2.response.is_none());
        assert_eq!(placements.len(), 1, "implicit_id must not skip add_placement");
        assert_eq!(placements[0].rows, 2);
        assert_eq!(placements[0].columns, 3);
        assert_eq!(placements[0].location, PlacementLocation::Pin { row: 5, col: 0 });
    }

    /// Cursor movement: `a=T` with explicit rows must report cursor_rows_to_move.
    #[test]
    fn test_transmit_and_display_cursor_movement() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;

        let cmd = transmit_and_display_cmd(80, &rgba_1x1(), false, 5, 10);
        let result = execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        assert_eq!(result.cursor_rows_to_move, 5);
    }

    /// Cursor movement: `C=1` (DoNotMove) must report 0.
    #[test]
    fn test_cursor_no_move() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;

        let cmd = Command {
            action: Action::TransmitAndDisplay,
            transmission: Transmission {
                format: Format::Rgba,
                width: 1,
                height: 1,
                image_id: 81,
                ..Default::default()
            },
            display: DisplayCmd {
                rows: 5,
                cursor_movement: CursorMovement::DoNotMove,
                ..Default::default()
            },
            data: rgba_1x1(),
            ..Default::default()
        };
        let result = execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        assert_eq!(result.cursor_rows_to_move, 0);
        assert_eq!(placements.len(), 1);
    }

    /// Single-chunk `a=T`, no `i=` (implicit id): placement created,
    /// no PTY response, cursor still moves.
    #[test]
    fn test_single_chunk_implicit_id_creates_placement_and_moves_cursor() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;

        let cmd = Command {
            action: Action::TransmitAndDisplay,
            transmission: Transmission {
                format: Format::Rgba,
                width: 1,
                height: 1,
                ..Default::default()
            },
            display: DisplayCmd { rows: 3, columns: 4, ..Default::default() },
            data: rgba_1x1(),
            ..Default::default()
        };
        let result = execute(&cmd, &mut storage, &mut placements, &mut loading, 2, 1);
        assert!(result.response.is_none(), "implicit_id must suppress PTY response");
        assert_eq!(placements.len(), 1, "placement must still be created");
        assert_eq!(placements[0].rows, 3);
        assert_eq!(placements[0].columns, 4);
        assert_eq!(placements[0].location, PlacementLocation::Pin { row: 2, col: 1 });
        assert_eq!(result.cursor_rows_to_move, 3, "cursor must advance even with implicit_id");
    }

    /// Cursor movement with chunked implicit-id upload.
    #[test]
    fn test_chunked_implicit_id_cursor_movement() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;

        let first = Command {
            action: Action::TransmitAndDisplay,
            transmission: Transmission {
                format: Format::Rgba,
                width: 1,
                height: 1,
                more_chunks: true,
                ..Default::default()
            },
            display: DisplayCmd { rows: 4, columns: 6, ..Default::default() },
            data: vec![255, 0],
            ..Default::default()
        };
        execute(&first, &mut storage, &mut placements, &mut loading, 1, 0);

        let last = continuation_chunk(&[0, 255], false);
        let result = execute(&last, &mut storage, &mut placements, &mut loading, 0, 0);
        assert_eq!(result.cursor_rows_to_move, 4);
        assert_eq!(placements.len(), 1);
    }

    #[test]
    fn test_execute_chunk_id_mismatch() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;

        let first = transmit_cmd(11, &[255, 0, 0], true);
        execute(&first, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(loading.is_some());
        assert_eq!(loading.as_ref().unwrap().id, 11);

        let mismatched = transmit_cmd(12, &rgba_1x1(), false);
        execute(&mismatched, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(loading.is_none());
        assert!(storage.get_image(11).is_none());
        assert!(storage.get_image(12).is_some());
    }

    /// Re-displaying an existing (image_id, placement_id) pair replaces the
    /// placement instead of stacking a duplicate.
    #[test]
    fn test_display_replaces_existing_placement() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        execute(&transmit_cmd(90, &rgba_1x1(), false), &mut storage, &mut placements, &mut loading, 0, 0);

        let display = |placement_id| Command {
            action: Action::Display,
            transmission: Transmission { image_id: 90, placement_id, ..Default::default() },
            ..Default::default()
        };
        execute(&display(7), &mut storage, &mut placements, &mut loading, 1, 1);
        execute(&display(7), &mut storage, &mut placements, &mut loading, 3, 4);

        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].placement_id, 7);
        assert_eq!(placements[0].location, PlacementLocation::Pin { row: 3, col: 4 });
    }

    /// d=c deletes placements *intersecting* the cursor, not just anchored at it.
    #[test]
    fn test_delete_at_cursor_intersects() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;

        let mut cmd = transmit_cmd(91, &rgba_1x1(), false);
        cmd.action = Action::TransmitAndDisplay;
        cmd.display.rows = 5;
        cmd.display.columns = 10;
        execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);

        // Cursor on row 3: inside the image's 5-row span but not its anchor.
        let delete = Command {
            action: Action::Delete,
            delete: DeleteAction::AtCursor { delete_data: false },
            ..Default::default()
        };
        execute(&delete, &mut storage, &mut placements, &mut loading, 3, 5);
        assert!(placements.is_empty());
    }

    /// d=p uses 1-based cell coordinates and deletes intersecting placements.
    #[test]
    fn test_delete_at_position_intersects_one_based() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;

        let mut cmd = transmit_cmd(92, &rgba_1x1(), false);
        cmd.action = Action::TransmitAndDisplay;
        cmd.display.rows = 2;
        cmd.display.columns = 3;
        execute(&cmd, &mut storage, &mut placements, &mut loading, 2, 4);

        // x=6,y=4 (1-based) = cell (row 3, col 5): inside the 2x3 span at (2,4).
        let delete = Command {
            action: Action::Delete,
            delete: DeleteAction::AtPosition { x: 6, y: 4, z: 0, delete_data: false },
            ..Default::default()
        };
        execute(&delete, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(placements.is_empty());

        // x=0/y=0 is outside the 1-based coordinate space: must not panic or match.
        let mut cmd = transmit_cmd(93, &rgba_1x1(), false);
        cmd.action = Action::TransmitAndDisplay;
        execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        let delete = Command {
            action: Action::Delete,
            delete: DeleteAction::AtPosition { x: 0, y: 0, z: 0, delete_data: false },
            ..Default::default()
        };
        execute(&delete, &mut storage, &mut placements, &mut loading, 0, 0);
        assert_eq!(placements.len(), 1);
    }

    /// Deletes answer only when the client supplied an id/number to match.
    #[test]
    fn test_delete_response() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        execute(&transmit_cmd(94, &rgba_1x1(), false), &mut storage, &mut placements, &mut loading, 0, 0);

        let delete = |quiet| Command {
            action: Action::Delete,
            quiet,
            transmission: Transmission { image_id: 94, ..Default::default() },
            delete: DeleteAction::ById { image_id: 94, delete_data: false },
            ..Default::default()
        };
        let result = execute(&delete(Quiet::No), &mut storage, &mut placements, &mut loading, 0, 0);
        let text = String::from_utf8_lossy(result.response.as_deref().unwrap()).into_owned();
        assert!(text.contains("i=94;OK"));

        let result =
            execute(&delete(Quiet::SuppressAll), &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(result.response.is_none());

        let anonymous = Command {
            action: Action::Delete,
            delete: DeleteAction::All { delete_data: false },
            ..Default::default()
        };
        let result = execute(&anonymous, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(result.response.is_none());
    }
}
