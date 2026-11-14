use std::path::Path;

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
    //
    // Discard it when the incoming command uses a non-direct medium:
    // non-direct transmissions unconditionally abandon any in-progress
    // chunked load, so the saved display belongs to a different (now
    // discarded) transmission and must not bleed into the new image.
    let saved_display = if cmd.transmission.medium == Medium::Direct {
        loading.as_ref().and_then(|li| li.pending_display.clone())
    } else {
        None
    };

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

    // Non-direct mediums (file, temp-file, shared-memory) are always
    // single-shot — they never participate in chunked loading. If one
    // arrives while a chunked direct transmission is in progress, abandon
    // the incomplete load (following Ghostty's 3079a4396 fix).
    if t.medium != Medium::Direct {
        *loading = None;
    }

    if let Some(ref current) = *loading {
        if t.image_id != 0 && current.id != t.image_id {
            *loading = None;
        }
    }

    // Invariant: if we reach here with `loading.is_some()`, the medium is
    // Direct (non-Direct was cleared above).
    if let Some(existing) = loading.take() {
        debug_assert_eq!(t.medium, Medium::Direct);
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
                    transmission_order: 0,
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
        storage.delete(&DeleteAction::ById { image_id: t.image_id, placement_id: 0, delete_data: true });
    }

    let current = start_loading(cmd, storage)?;
    if t.more_chunks && t.medium == Medium::Direct {
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
                transmission_order: 0,
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

    match t.medium {
        Medium::Direct => {
            loading.add_data(&cmd.data).map_err(ExecError::Image)?;
        },
        Medium::File | Medium::TempFile => {
            let data = read_file_payload(&cmd.data, t.medium, t.offset, t.size)?;
            loading.add_data(&data).map_err(ExecError::Image)?;
        },
        Medium::SharedMemory => {
            return Err(ExecError::Message("EINVAL: shared memory transmission not yet supported"));
        },
    }

    Ok(loading)
}

fn try_complete_image(cmd: &Command) -> Result<(), ExecError> {
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

    match t.medium {
        Medium::Direct => {
            loading.add_data(&cmd.data).map_err(ExecError::Image)?;
        },
        Medium::File | Medium::TempFile => {
            let data = read_file_payload(&cmd.data, t.medium, t.offset, t.size)?;
            loading.add_data(&data).map_err(ExecError::Image)?;
        },
        Medium::SharedMemory => {
            return Err(ExecError::Message("EINVAL: shared memory transmission not yet supported"));
        },
    }

    let image = loading.complete().map_err(ExecError::Image)?;
    drop(image);
    Ok(())
}

/// Maximum file size accepted via `t=f`/`t=t` (200 MiB).
const MAX_FILE_READ: usize = 200 * 1024 * 1024;

/// Marker substring required in `t=t` temp file paths (matching Kitty/Ghostty convention).
const TEMP_FILE_MARKER: &str = "tty-graphics-protocol";

/// Read image data from a file path in `cmd.data`.
///
/// `cmd.data` is already base64-decoded by the APC parser; it contains a
/// UTF-8 file path. For `t=t` (temporary file), the file is deleted after
/// reading.
///
/// Security model (follows Ghostty's approach):
/// - Open the file **first**, then canonicalize the path and verify that
///   the canonical path's inode matches the opened fd's inode — this
///   closes the TOCTOU gap (a symlink swap between open and canonicalize
///   is detected via inode mismatch).
/// - Only regular files are accepted.
/// - Sensitive pseudo-filesystem paths (`/proc`, `/sys`, `/dev` except
///   `/dev/shm`) are blocked for both `t=f` and `t=t`.
/// - For `t=t`, the resolved path must reside under a system temp directory
///   and must contain the `tty-graphics-protocol` marker in its name.
fn read_file_payload(
    raw_payload: &[u8],
    medium: Medium,
    offset: u32,
    size: u32,
) -> Result<Vec<u8>, ExecError> {
    use std::fs;
    use std::io::{Read, Seek, SeekFrom};

    if raw_payload.is_empty() {
        return Err(ExecError::Message("EINVAL: empty file path"));
    }

    if size as usize > MAX_FILE_READ {
        return Err(ExecError::Message("EINVAL: declared size exceeds maximum"));
    }

    let path_str = std::str::from_utf8(raw_payload)
        .map_err(|_| ExecError::Message("EINVAL: file path is not valid UTF-8"))?;

    let path = Path::new(path_str);

    // Open with O_NONBLOCK on Unix to prevent blocking on FIFOs/named
    // pipes that have no writer. Graphics commands run synchronously under
    // the terminal lock, so a blocking open would freeze the entire
    // terminal. The flag is harmless for regular files.
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .map_err(|_| ExecError::Message("ENOENT: cannot open file"))?
    };
    #[cfg(not(unix))]
    let mut file = fs::File::open(path)
        .map_err(|_| ExecError::Message("ENOENT: cannot open file"))?;

    // Verify the opened fd is a regular file.
    let opened_meta = file.metadata()
        .map_err(|_| ExecError::Message("EIO: cannot stat file"))?;
    if !opened_meta.is_file() {
        return Err(ExecError::Message("EINVAL: not a regular file"));
    }

    // Canonicalize for path-based validation.
    let canonical = fs::canonicalize(path)
        .map_err(|_| ExecError::Message("ENOENT: file not found"))?;

    // TOCTOU guard: verify the canonical path still points to the same
    // file we opened (same device + inode). If someone swapped the
    // symlink between our open() and canonicalize(), the inodes diverge.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let canonical_meta = fs::metadata(&canonical)
            .map_err(|_| ExecError::Message("ENOENT: canonical path disappeared"))?;
        if opened_meta.dev() != canonical_meta.dev()
            || opened_meta.ino() != canonical_meta.ino()
        {
            return Err(ExecError::Message("EINVAL: file path changed during validation"));
        }
    }

    // Block sensitive pseudo-filesystem paths.
    reject_sensitive_path(&canonical)?;

    if medium == Medium::TempFile {
        validate_temp_path(&canonical)?;
    }

    if offset > 0 {
        file.seek(SeekFrom::Start(offset as u64))
            .map_err(|_| ExecError::Message("EIO: seek failed"))?;
    }

    let buf = if size > 0 {
        let mut buf = vec![0u8; size as usize];
        file.read_exact(&mut buf)
            .map_err(|_| ExecError::Message("EIO: file shorter than declared size"))?;
        buf
    } else {
        let mut buf = Vec::new();
        file.take(MAX_FILE_READ as u64)
            .read_to_end(&mut buf)
            .map_err(|_| ExecError::Message("EIO: read failed"))?;
        buf
    };

    if buf.is_empty() {
        return Err(ExecError::Message("ENODATA: file is empty"));
    }

    // Delete temporary files after reading (Kitty protocol requirement).
    if medium == Medium::TempFile {
        let _ = fs::remove_file(&canonical);
    }

    Ok(buf)
}

/// Reject paths under sensitive pseudo-filesystems.
///
/// Following Ghostty, we block `/proc/`, `/sys/`, and `/dev/` (except
/// `/dev/shm/` which is a legitimate shared-memory temp location on Linux).
fn reject_sensitive_path(canonical: &Path) -> Result<(), ExecError> {
    if canonical.starts_with("/proc") || canonical.starts_with("/sys") {
        return Err(ExecError::Message("EPERM: access to pseudo-filesystem denied"));
    }

    if canonical.starts_with("/dev") && !canonical.starts_with("/dev/shm") {
        return Err(ExecError::Message("EPERM: access to device path denied"));
    }

    Ok(())
}

/// Validate that a canonical path resides under a system temp directory.
///
/// Accepted directories:
/// - `std::env::temp_dir()` (and its canonicalized form)
/// - `/tmp` (always, even on Linux where it differs from `temp_dir`)
/// - `/dev/shm` (Linux shared memory)
/// - `/private/tmp` (macOS, where `/tmp` → `/private/tmp`)
///
/// For `t=t` the path must also contain the `tty-graphics-protocol` marker
/// to prevent accidental deletion of unrelated temp files.
fn validate_temp_path(canonical: &Path) -> Result<(), ExecError> {
    // Check for the required marker in the file name / path components.
    if !canonical.to_string_lossy().contains(TEMP_FILE_MARKER) {
        return Err(ExecError::Message(
            "EINVAL: temp file path must contain 'tty-graphics-protocol'",
        ));
    }

    // Canonical env temp dir (handles /var → /private/var on macOS).
    let temp_dir = std::env::temp_dir();
    if let Ok(canonical_temp) = std::fs::canonicalize(&temp_dir) {
        if canonical.starts_with(&canonical_temp) {
            return Ok(());
        }
    }

    // Direct comparison with the un-canonicalized temp dir.
    if canonical.starts_with(&temp_dir) {
        return Ok(());
    }

    // /tmp is always a valid temp root.
    if canonical.starts_with("/tmp") {
        return Ok(());
    }

    // macOS: /tmp → /private/tmp.
    #[cfg(target_os = "macos")]
    if canonical.starts_with("/private/tmp") {
        return Ok(());
    }

    // Linux: /dev/shm is used for shared-memory backed temp files.
    #[cfg(target_os = "linux")]
    if canonical.starts_with("/dev/shm") {
        return Ok(());
    }

    Err(ExecError::Message("EPERM: temporary file not in temp directory"))
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
        DeleteAction::ById { image_id, placement_id, .. } => {
            remove_placements_matching(placements, storage, |p| {
                p.image_id == *image_id && (*placement_id == 0 || p.placement_id == *placement_id)
            });
        },
        DeleteAction::ByNumber { image_number, placement_id, .. } => {
            if let Some(id) = storage.get_image_by_number(*image_number).map(|img| img.id) {
                remove_placements_matching(placements, storage, |p| {
                    p.image_id == id && (*placement_id == 0 || p.placement_id == *placement_id)
                });
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
                DeleteAction::ById { image_id, placement_id: 0, delete_data: *delete_data }
            },
            DeleteAction::AtPosition { delete_data, .. }
            | DeleteAction::ByZIndex { delete_data, .. }
            | DeleteAction::InColumn { delete_data, .. }
            | DeleteAction::InRow { delete_data, .. } => DeleteAction::ById { image_id, placement_id: 0, delete_data: *delete_data },
            _ => DeleteAction::ById { image_id, placement_id: 0, delete_data: true },
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
            delete: DeleteAction::ById { image_id: 40, placement_id: 0, delete_data: true },
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

    /// d=i with p= deletes only the named placement of the image.
    #[test]
    fn test_delete_by_id_with_placement_selector() {
        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        execute(&transmit_cmd(95, &rgba_1x1(), false), &mut storage, &mut placements, &mut loading, 0, 0);

        let display = |placement_id, _row| Command {
            action: Action::Display,
            transmission: Transmission { image_id: 95, placement_id, ..Default::default() },
            ..Default::default()
        };
        execute(&display(1, 0), &mut storage, &mut placements, &mut loading, 0, 0);
        execute(&display(2, 0), &mut storage, &mut placements, &mut loading, 1, 0);
        assert_eq!(placements.len(), 2);

        let delete = Command {
            action: Action::Delete,
            transmission: Transmission { image_id: 95, placement_id: 1, ..Default::default() },
            delete: DeleteAction::ById { image_id: 95, placement_id: 1, delete_data: false },
            ..Default::default()
        };
        execute(&delete, &mut storage, &mut placements, &mut loading, 0, 0);
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].placement_id, 2);
        assert!(storage.get_image(95).is_some(), "image data must survive");
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
            delete: DeleteAction::ById { image_id: 94, placement_id: 0, delete_data: false },
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

    // --- File and temp-file medium tests ---

    #[test]
    fn test_transmit_file_medium() {
        use std::io::Write;
        let rgba = rgba_1x1();
        let dir = std::env::temp_dir();
        let path = dir.join("alacritty_test_kitty_file.rgba");
        std::fs::File::create(&path).unwrap().write_all(&rgba).unwrap();

        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        let cmd = Command {
            action: Action::Transmit,
            transmission: Transmission {
                format: Format::Rgba,
                medium: Medium::File,
                width: 1,
                height: 1,
                image_id: 100,
                ..Default::default()
            },
            data: path.to_string_lossy().into_owned().into_bytes(),
            ..Default::default()
        };
        let result = execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(result.response.is_some());
        assert!(storage.get_image(100).is_some());
        // File should still exist (not a temp file).
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_transmit_temp_file_deletes_after_read() {
        use std::io::Write;
        let rgba = rgba_1x1();
        let dir = std::env::temp_dir();
        let path = dir.join("tty-graphics-protocol-test-temp.rgba");
        std::fs::File::create(&path).unwrap().write_all(&rgba).unwrap();

        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        let cmd = Command {
            action: Action::Transmit,
            transmission: Transmission {
                format: Format::Rgba,
                medium: Medium::TempFile,
                width: 1,
                height: 1,
                image_id: 101,
                ..Default::default()
            },
            data: path.to_string_lossy().into_owned().into_bytes(),
            ..Default::default()
        };
        let result = execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(result.response.is_some());
        assert!(storage.get_image(101).is_some());
        // Temp file must be deleted after reading.
        assert!(!path.exists());
    }

    #[test]
    fn test_temp_file_rejects_non_temp_path() {
        let path = if cfg!(target_os = "macos") {
            "/Users/fake/tty-graphics-protocol-image.rgba"
        } else {
            "/home/fake/tty-graphics-protocol-image.rgba"
        };
        let result = validate_temp_path(std::path::Path::new(path));
        assert!(result.is_err());
    }

    #[test]
    fn test_temp_file_rejects_missing_marker() {
        let dir = std::env::temp_dir();
        let canonical_temp = std::fs::canonicalize(&dir).unwrap();
        let path = canonical_temp.join("no-marker-image.rgba");
        let result = validate_temp_path(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_temp_file_accepts_temp_dir_with_marker() {
        let dir = std::env::temp_dir();
        let canonical_temp = std::fs::canonicalize(&dir).unwrap();
        let path = canonical_temp.join("tty-graphics-protocol-test.rgba");
        assert!(validate_temp_path(&path).is_ok());
    }

    #[test]
    fn test_file_offset_and_size() {
        use std::io::Write;
        // Write 8 bytes of RGBA data: 2 pixels.
        let data = [255u8, 0, 0, 255, 0, 255, 0, 255];
        let dir = std::env::temp_dir();
        let path = dir.join("alacritty_test_kitty_offset.rgba");
        std::fs::File::create(&path).unwrap().write_all(&data).unwrap();

        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;
        let cmd = Command {
            action: Action::Transmit,
            transmission: Transmission {
                format: Format::Rgba,
                medium: Medium::File,
                width: 1,
                height: 1,
                image_id: 102,
                offset: 4,
                size: 4,
                ..Default::default()
            },
            data: path.to_string_lossy().into_owned().into_bytes(),
            ..Default::default()
        };
        let result = execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(result.response.is_some());
        let img = storage.get_image(102).unwrap();
        assert_eq!(&img.data, &[0, 255, 0, 255]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_file_size_exceeds_file_length() {
        use std::io::Write;
        let data = [255u8, 0, 0, 255];
        let dir = std::env::temp_dir();
        let path = dir.join("alacritty_test_kitty_short.rgba");
        std::fs::File::create(&path).unwrap().write_all(&data).unwrap();

        let result = read_file_payload(
            path.to_string_lossy().as_bytes(),
            Medium::File,
            0,
            100, // Much larger than the 4-byte file.
        );
        assert!(result.is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_file_rejects_directory() {
        let dir = std::env::temp_dir();
        let result = read_file_payload(
            dir.to_string_lossy().as_bytes(),
            Medium::File,
            0,
            0,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_file_medium_abandons_chunked_load() {
        use std::io::Write;

        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;

        // Start a chunked direct transmission (more_chunks=true).
        let chunk1 = Command {
            action: Action::Transmit,
            transmission: Transmission {
                format: Format::Rgba,
                medium: Medium::Direct,
                width: 1,
                height: 1,
                image_id: 200,
                more_chunks: true,
                ..Default::default()
            },
            data: vec![255, 0, 0, 255],
            ..Default::default()
        };
        execute(&chunk1, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(loading.is_some(), "chunked load should be in progress");

        // A file medium command arrives — should abandon the chunked load.
        let rgba = rgba_1x1();
        let dir = std::env::temp_dir();
        let path = dir.join("alacritty_test_abandon_chunk.rgba");
        std::fs::File::create(&path).unwrap().write_all(&rgba).unwrap();

        let file_cmd = Command {
            action: Action::Transmit,
            transmission: Transmission {
                format: Format::Rgba,
                medium: Medium::File,
                width: 1,
                height: 1,
                image_id: 201,
                ..Default::default()
            },
            data: path.to_string_lossy().into_owned().into_bytes(),
            ..Default::default()
        };
        let result = execute(&file_cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(result.response.is_some());
        // The file image should be stored; the chunked load abandoned.
        assert!(storage.get_image(201).is_some());
        assert!(loading.is_none(), "loading should be cleared after file medium");
        // The partial chunked image (200) should NOT be stored.
        assert!(storage.get_image(200).is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_file_medium_ignores_more_chunks() {
        use std::io::Write;

        let rgba = rgba_1x1();
        let dir = std::env::temp_dir();
        let path = dir.join("alacritty_test_no_chunk.rgba");
        std::fs::File::create(&path).unwrap().write_all(&rgba).unwrap();

        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;

        // File medium with more_chunks=true should NOT enter chunked mode.
        let cmd = Command {
            action: Action::Transmit,
            transmission: Transmission {
                format: Format::Rgba,
                medium: Medium::File,
                width: 1,
                height: 1,
                image_id: 202,
                more_chunks: true,
                ..Default::default()
            },
            data: path.to_string_lossy().into_owned().into_bytes(),
            ..Default::default()
        };
        let result = execute(&cmd, &mut storage, &mut placements, &mut loading, 0, 0);
        assert!(result.response.is_some());
        // Image should be fully loaded (not chunked).
        assert!(storage.get_image(202).is_some());
        assert!(loading.is_none(), "file medium should not enter chunked loading");
        let _ = std::fs::remove_file(&path);
    }

    /// P2 fix: a chunked `a=T,m=1` followed by a file `a=t` must NOT
    /// inherit the abandoned T&D's display intent (saved_display).
    #[test]
    fn test_file_medium_does_not_inherit_saved_display() {
        use std::io::Write;

        let mut storage = ImageStorage::new();
        let mut placements = Vec::new();
        let mut loading = None;

        // Start a chunked TransmitAndDisplay (a=T, m=1).
        let first = Command {
            action: Action::TransmitAndDisplay,
            transmission: Transmission {
                format: Format::Rgba,
                medium: Medium::Direct,
                width: 1,
                height: 1,
                image_id: 300,
                more_chunks: true,
                ..Default::default()
            },
            data: vec![255, 0, 0, 255],
            ..Default::default()
        };
        execute_transmit(&first, &mut storage, &mut placements, &mut loading, Some((2, 4)));
        assert!(loading.is_some());
        assert!(loading.as_ref().unwrap().pending_display.is_some());

        // Now a file-medium Transmit (NOT TransmitAndDisplay) arrives.
        let rgba = rgba_1x1();
        let dir = std::env::temp_dir();
        let path = dir.join("alacritty_test_no_inherit_display.rgba");
        std::fs::File::create(&path).unwrap().write_all(&rgba).unwrap();

        let file_cmd = Command {
            action: Action::Transmit,
            transmission: Transmission {
                format: Format::Rgba,
                medium: Medium::File,
                width: 1,
                height: 1,
                image_id: 301,
                ..Default::default()
            },
            data: path.to_string_lossy().into_owned().into_bytes(),
            ..Default::default()
        };
        execute_transmit(&file_cmd, &mut storage, &mut placements, &mut loading, None);

        // The file image (301) should be stored but NOT have a placement
        // (it was action=Transmit, not TransmitAndDisplay).
        assert!(storage.get_image(301).is_some());
        let has_placement_301 = placements.iter().any(|p| p.image_id == 301);
        assert!(!has_placement_301, "file image should not inherit abandoned T&D placement");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_reject_sensitive_paths() {
        assert!(reject_sensitive_path(Path::new("/proc/self/environ")).is_err());
        assert!(reject_sensitive_path(Path::new("/sys/class/net")).is_err());
        assert!(reject_sensitive_path(Path::new("/dev/urandom")).is_err());
        // /dev/shm is allowed.
        assert!(reject_sensitive_path(Path::new("/dev/shm/test")).is_ok());
        // Normal paths are allowed.
        assert!(reject_sensitive_path(Path::new("/tmp/test.png")).is_ok());
    }

    #[test]
    fn test_size_exceeds_max_file_read() {
        let result = read_file_payload(
            b"/tmp/test.png",
            Medium::File,
            0,
            (MAX_FILE_READ + 1) as u32,
        );
        assert!(result.is_err());
    }
}
