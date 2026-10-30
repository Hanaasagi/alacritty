use std::collections::HashMap;

use crate::graphics::command::DeleteAction;
use crate::graphics::image::Image;

struct ImageRecord {
    image: Image,
    /// Placement references (P3). While > 0, deleting image data clears pixels only.
    references: u32,
}

/// Kitty graphics image storage with a fixed memory budget.
pub struct ImageStorage {
    images: HashMap<u32, ImageRecord>,
    /// Image IDs in insertion order (oldest first) for eviction.
    insertion_order: Vec<u32>,
    total_bytes: usize,
    total_limit: usize,
    next_image_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// Image exceeds the total storage limit even alone.
    ImageTooLarge,
}

impl Default for ImageStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageStorage {
    pub fn new() -> Self {
        Self::with_limit(320_000_000)
    }

    pub fn with_limit(limit: usize) -> Self {
        Self {
            images: HashMap::new(),
            insertion_order: Vec::new(),
            total_bytes: 0,
            total_limit: limit,
            next_image_id: 2_147_483_647,
        }
    }

    /// Update the storage limit, evicting oldest images if necessary.
    pub fn set_max_limit(&mut self, limit: usize) {
        if limit == self.total_limit {
            return;
        }

        self.total_limit = limit;
        if self.total_bytes > limit {
            self.evict(self.total_bytes - limit, 0);
        }
    }

    /// Add an image. If an image with the same ID exists, replace it.
    /// If storage is full, evict oldest images to make room.
    pub fn add_image(&mut self, image: Image) -> Result<(), StorageError> {
        let new_len = image.data.len();
        if new_len > self.total_limit {
            return Err(StorageError::ImageTooLarge);
        }

        let id = image.id;
        let old_len = self.images.get(&id).map(|record| record.image.data.len()).unwrap_or(0);
        let projected = self.total_bytes - old_len + new_len;
        if projected > self.total_limit {
            self.evict(projected - self.total_limit, id);
        }

        if self.total_bytes - old_len + new_len > self.total_limit {
            return Err(StorageError::ImageTooLarge);
        }

        if let Some(record) = self.images.get_mut(&id) {
            self.total_bytes -= record.image.data.len();
            record.image = image;
            self.total_bytes += record.image.data.len();
        } else {
            self.total_bytes += new_len;
            self.insertion_order.push(id);
            self.images.insert(id, ImageRecord { image, references: 0 });
        }
        Ok(())
    }

    /// Get an image by ID.
    pub fn get_image(&self, id: u32) -> Option<&Image> {
        self.images.get(&id).map(|record| &record.image)
    }

    /// Get an image by image_number (`I=` parameter).
    pub fn get_image_by_number(&self, number: u32) -> Option<&Image> {
        if number == 0 {
            return None;
        }
        self.images
            .values()
            .find(|record| record.image.image_number == number)
            .map(|record| &record.image)
    }

    /// Delete images based on the action.
    pub fn delete(&mut self, action: &DeleteAction) {
        match action {
            DeleteAction::All { delete_data } => {
                if *delete_data {
                    self.clear_all_images();
                }
            },
            DeleteAction::ById { image_id, delete_data } => {
                self.delete_by_id(*image_id, *delete_data);
            },
            DeleteAction::ByNumber { image_number, delete_data } => {
                let id = self
                    .images
                    .values()
                    .find(|record| record.image.image_number == *image_number)
                    .map(|record| record.image.id);
                if let Some(id) = id {
                    self.delete_by_id(id, *delete_data);
                }
            },
            DeleteAction::AtCursor { .. }
            | DeleteAction::AnimationFrames { .. }
            | DeleteAction::AtPosition { .. }
            | DeleteAction::ByZIndex { .. }
            | DeleteAction::InColumn { .. }
            | DeleteAction::InRow { .. }
            | DeleteAction::ByRange { .. } => {},
        }
    }

    /// Assign the next available image ID.
    pub fn next_image_id(&mut self) -> u32 {
        let mut id = self.next_image_id;
        let attempts = self.images.len().saturating_add(2);
        for _ in 0..attempts {
            if id != 0 && !self.images.contains_key(&id) {
                break;
            }
            id = id.wrapping_add(1);
            if id == 0 {
                id = 1;
            }
        }
        if id == 0 {
            id = 1;
        }
        self.next_image_id = id.wrapping_add(1);
        id
    }

    /// Total bytes used.
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// Increment the reference count for an image.
    pub fn add_reference(&mut self, image_id: u32) {
        if let Some(record) = self.images.get_mut(&image_id) {
            record.references += 1;
        }
    }

    /// Decrement the reference count for an image.
    pub fn remove_reference(&mut self, image_id: u32) {
        if let Some(record) = self.images.get_mut(&image_id) {
            record.references = record.references.saturating_sub(1);
        }
    }

    fn delete_by_id(&mut self, image_id: u32, delete_data: bool) -> bool {
        if delete_data {
            self.delete_image_data(image_id)
        } else {
            false
        }
    }

    fn delete_image_data(&mut self, image_id: u32) -> bool {
        let Some(record) = self.images.get_mut(&image_id) else {
            return false;
        };

        if record.references > 0 {
            if record.image.data.is_empty() {
                return false;
            }
            self.total_bytes -= record.image.data.len();
            record.image.data.clear();
            return true;
        }

        let len = record.image.data.len();
        self.total_bytes -= len;
        self.images.remove(&image_id);
        self.insertion_order.retain(|&id| id != image_id);
        true
    }

    fn clear_all_images(&mut self) {
        self.images.clear();
        self.insertion_order.clear();
        self.total_bytes = 0;
    }

    fn evict(&mut self, needed_bytes: usize, except_id: u32) {
        let mut freed = 0usize;
        let mut idx = 0;
        while freed < needed_bytes && idx < self.insertion_order.len() {
            let id = self.insertion_order[idx];
            if id == except_id {
                idx += 1;
                continue;
            }
            if let Some(record) = self.images.get(&id) {
                if record.references > 0 {
                    idx += 1;
                    continue;
                }
                let len = record.image.data.len();
                self.images.remove(&id);
                self.total_bytes = self.total_bytes.saturating_sub(len);
                freed += len;
                self.insertion_order.remove(idx);
                continue;
            }
            self.insertion_order.remove(idx);
        }
    }

    #[cfg(test)]
    fn set_references(&mut self, id: u32, references: u32) {
        if let Some(record) = self.images.get_mut(&id) {
            record.references = references;
        }
    }

    #[cfg(test)]
    fn reference_count(&self, id: u32) -> u32 {
        self.images.get(&id).map(|record| record.references).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_image(id: u32, size: usize) -> Image {
        Image {
            id,
            width: 1,
            height: (size / 4).max(1) as u32,
            data: vec![0xAB; size],
            image_number: id.saturating_mul(10),
            implicit_id: false,
        }
    }

    #[test]
    fn test_add_and_get_image() {
        let mut storage = ImageStorage::new();
        let image = sample_image(1, 16);
        storage.add_image(image).unwrap();
        let stored = storage.get_image(1).unwrap();
        assert_eq!(stored.id, 1);
        assert_eq!(stored.data.len(), 16);
    }

    #[test]
    fn test_replace_image() {
        let mut storage = ImageStorage::new();
        storage.add_image(sample_image(1, 16)).unwrap();
        storage.add_image(sample_image(1, 32)).unwrap();
        assert_eq!(storage.get_image(1).unwrap().data.len(), 32);
        assert_eq!(storage.total_bytes(), 32);
    }

    #[test]
    fn test_delete_all() {
        let mut storage = ImageStorage::new();
        storage.add_image(sample_image(1, 16)).unwrap();
        storage.add_image(sample_image(2, 16)).unwrap();
        storage.delete(&DeleteAction::All { delete_data: true });
        assert!(storage.get_image(1).is_none());
        assert!(storage.get_image(2).is_none());
        assert_eq!(storage.total_bytes(), 0);
    }

    #[test]
    fn test_delete_by_id() {
        let mut storage = ImageStorage::new();
        storage.add_image(sample_image(1, 16)).unwrap();
        storage.delete(&DeleteAction::ById { image_id: 1, delete_data: true });
        assert!(storage.get_image(1).is_none());
    }

    #[test]
    fn test_eviction() {
        let mut storage = ImageStorage::with_limit(32);
        storage.add_image(sample_image(1, 16)).unwrap();
        storage.add_image(sample_image(2, 16)).unwrap();
        storage.add_image(sample_image(3, 16)).unwrap();
        assert!(storage.get_image(1).is_none());
        assert!(storage.get_image(2).is_some());
        assert!(storage.get_image(3).is_some());
        assert_eq!(storage.total_bytes(), 32);
    }

    #[test]
    fn test_total_bytes() {
        let mut storage = ImageStorage::new();
        storage.add_image(sample_image(1, 16)).unwrap();
        storage.add_image(sample_image(2, 32)).unwrap();
        assert_eq!(storage.total_bytes(), 48);
    }

    #[test]
    fn test_next_image_id() {
        let mut storage = ImageStorage::new();
        let first = storage.next_image_id();
        let second = storage.next_image_id();
        assert_ne!(first, second);
        storage.add_image(sample_image(first, 16)).unwrap();
        let third = storage.next_image_id();
        assert_ne!(third, first);
    }

    #[test]
    fn test_image_too_large() {
        let mut storage = ImageStorage::with_limit(16);
        let result = storage.add_image(sample_image(1, 32));
        assert_eq!(result, Err(StorageError::ImageTooLarge));
    }

    #[test]
    fn test_delete_by_id_data_only() {
        let mut storage = ImageStorage::new();
        storage.add_image(sample_image(1, 16)).unwrap();
        storage.set_references(1, 1);
        storage.delete(&DeleteAction::ById { image_id: 1, delete_data: true });
        let image = storage.get_image(1).unwrap();
        assert!(image.data.is_empty());
        assert_eq!(storage.total_bytes(), 0);
    }

    #[test]
    fn test_delete_preserves_other_images() {
        let mut storage = ImageStorage::new();
        storage.add_image(sample_image(1, 16)).unwrap();
        storage.add_image(sample_image(2, 16)).unwrap();
        storage.delete(&DeleteAction::ById { image_id: 1, delete_data: true });
        assert!(storage.get_image(1).is_none());
        assert!(storage.get_image(2).is_some());
    }

    #[test]
    fn test_get_image_by_number() {
        let mut storage = ImageStorage::new();
        storage.add_image(sample_image(5, 16)).unwrap();
        assert!(storage.get_image_by_number(50).is_some());
        assert!(storage.get_image_by_number(99).is_none());
    }

    #[test]
    fn test_delete_by_number() {
        let mut storage = ImageStorage::new();
        storage.add_image(sample_image(5, 16)).unwrap();
        storage.delete(&DeleteAction::ByNumber { image_number: 50, delete_data: true });
        assert!(storage.get_image(5).is_none());
        assert!(storage.get_image_by_number(50).is_none());
    }

    #[test]
    fn test_add_and_remove_reference() {
        let mut storage = ImageStorage::new();
        storage.add_image(sample_image(1, 16)).unwrap();
        assert_eq!(storage.reference_count(1), 0);
        storage.add_reference(1);
        storage.add_reference(1);
        assert_eq!(storage.reference_count(1), 2);
        storage.remove_reference(1);
        assert_eq!(storage.reference_count(1), 1);
        storage.remove_reference(1);
        assert_eq!(storage.reference_count(1), 0);
        storage.remove_reference(1);
        assert_eq!(storage.reference_count(1), 0);
    }

    #[test]
    fn test_eviction_skips_referenced() {
        let mut storage = ImageStorage::with_limit(32);
        storage.add_image(sample_image(1, 16)).unwrap();
        storage.add_reference(1);
        storage.add_image(sample_image(2, 16)).unwrap();
        storage.add_image(sample_image(3, 16)).unwrap();
        assert!(storage.get_image(1).is_some());
        assert!(storage.get_image(2).is_none());
        assert!(storage.get_image(3).is_some());
        assert_eq!(storage.total_bytes(), 32);
    }
}
