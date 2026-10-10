use crate::library::Library;
use crate::traits::ListItem;
use log::debug;
use std::sync::{Arc, RwLock};

pub struct ApiPage<I> {
    pub offset: u32,
    pub total: u32,
    pub items: Vec<I>,
}
pub type FetchPageFn<I> = dyn Fn(u32) -> Option<ApiPage<I>> + Send + Sync;
pub struct ApiResult<I> {
    offset: Arc<RwLock<u32>>,
    limit: u32,
    pub total: u32,
    pub items: Arc<RwLock<Vec<I>>>,
    fetch_page: Arc<FetchPageFn<I>>,
    /// The first page failed, so `total` is 0 regardless of the remote collection.
    first_page_failed: bool,
}

impl<I: ListItem + Clone> ApiResult<I> {
    pub fn new(limit: u32, fetch_page: Arc<FetchPageFn<I>>) -> Self {
        let items = Arc::new(RwLock::new(Vec::new()));
        if let Some(first_page) = fetch_page(0) {
            debug!(
                "fetched first page, items: {}, total: {}",
                first_page.items.len(),
                first_page.total
            );
            items.write().unwrap().extend(first_page.items);
            Self {
                offset: Arc::new(RwLock::new(first_page.offset)),
                limit,
                total: first_page.total,
                items,
                fetch_page: fetch_page.clone(),
                first_page_failed: false,
            }
        } else {
            Self {
                offset: Arc::new(RwLock::new(0)),
                limit,
                total: 0,
                items,
                fetch_page: fetch_page.clone(),
                first_page_failed: true,
            }
        }
    }

    fn offset(&self) -> u32 {
        *self.offset.read().unwrap()
    }

    pub fn at_end(&self) -> bool {
        (self.offset() + self.limit) >= self.total
    }

    /// Whether all pages were fetched. `at_end` alone is wrong after a failed page.
    pub fn is_complete(&self) -> bool {
        !self.first_page_failed && self.at_end()
    }

    /// Fetch the remaining pages and take all items, or `None` if a page failed.
    pub fn fetch_all(&self) -> Option<Vec<I>> {
        while !self.at_end() {
            self.next()?;
        }
        self.is_complete()
            .then(|| std::mem::take(&mut *self.items.write().unwrap()))
    }

    pub fn apply_pagination(self, pagination: &Pagination<I>) {
        let total = self.total as usize;
        let fetched_items = self.items.read().unwrap().len();
        pagination.set(
            fetched_items,
            total,
            Box::new(move |_| {
                self.next();
            }),
        )
    }

    pub fn next(&self) -> Option<Vec<I>> {
        let offset = self.offset() + self.limit;
        debug!("fetching next page at offset {offset}");
        if !self.at_end() {
            if let Some(next_page) = (self.fetch_page)(offset) {
                *self.offset.write().unwrap() = next_page.offset;
                self.items.write().unwrap().extend(next_page.items.clone());
                Some(next_page.items)
            } else {
                None
            }
        } else {
            debug!("paginator is at end");
            None
        }
    }
}

pub type Paginator<I> = Box<dyn Fn(Arc<RwLock<Vec<I>>>) + Send + Sync>;

/// Manages the loading of ListItems, to increase performance and decrease
/// memory usage.
///
/// `loaded_content`: The amount of currently loaded items
/// `max_content`: The maximum amount of items
/// `callback`: TODO: document
/// `busy`: TODO: document
#[derive(Clone)]
pub struct Pagination<I: ListItem> {
    loaded_content: Arc<RwLock<usize>>,
    max_content: Arc<RwLock<Option<usize>>>,
    callback: Arc<RwLock<Option<Paginator<I>>>>,
    busy: Arc<RwLock<bool>>,
}

impl<I: ListItem> Default for Pagination<I> {
    fn default() -> Self {
        Self {
            loaded_content: Arc::new(RwLock::new(0)),
            max_content: Arc::new(RwLock::new(None)),
            callback: Arc::new(RwLock::new(None)),
            busy: Arc::new(RwLock::new(false)),
        }
    }
}

impl<I: ListItem + Clone> Pagination<I> {
    pub fn clear(&mut self) {
        *self.max_content.write().unwrap() = None;
        *self.callback.write().unwrap() = None;
    }
    pub fn set(&self, loaded_content: usize, max_content: usize, callback: Paginator<I>) {
        *self.loaded_content.write().unwrap() = loaded_content;
        *self.max_content.write().unwrap() = Some(max_content);
        *self.callback.write().unwrap() = Some(callback);
    }

    pub fn loaded_content(&self) -> usize {
        *self.loaded_content.read().unwrap()
    }

    pub fn max_content(&self) -> Option<usize> {
        *self.max_content.read().unwrap()
    }

    fn is_busy(&self) -> bool {
        *self.busy.read().unwrap()
    }

    pub fn call(&self, content: &Arc<RwLock<Vec<I>>>, library: Arc<Library>) {
        let pagination = self.clone();
        let content = content.clone();
        if !self.is_busy() {
            *self.busy.write().unwrap() = true;
            std::thread::spawn(move || {
                let cb = pagination.callback.read().unwrap();
                if let Some(ref cb) = *cb {
                    debug!("calling paginator!");
                    cb(content.clone());
                    *pagination.loaded_content.write().unwrap() = content.read().unwrap().len();
                    *pagination.busy.write().unwrap() = false;
                    library.trigger_redraw();
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::playable::Playable;
    use crate::queue::tests::make_track;

    /// Returns `total` tracks, where fetching the page at `failing_offset` fails.
    fn api_result(total: u32, failing_offset: Option<u32>) -> ApiResult<Playable> {
        ApiResult::new(
            2,
            Arc::new(move |offset| {
                (Some(offset) != failing_offset).then(|| ApiPage {
                    offset,
                    total,
                    items: (offset..total.min(offset + 2)).map(make_track).collect(),
                })
            }),
        )
    }

    #[test]
    fn fetch_all_returns_all_items() {
        let result = api_result(5, None);
        assert_eq!(result.fetch_all().map(|items| items.len()), Some(5));
        assert!(result.is_complete());
    }

    #[test]
    fn fetch_all_stops_at_failed_page() {
        let result = api_result(5, Some(2));
        assert!(result.fetch_all().is_none());
        assert!(!result.is_complete());
    }

    #[test]
    fn failed_first_page_is_incomplete() {
        let result = api_result(5, Some(0));
        assert!(result.fetch_all().is_none());
        assert!(!result.is_complete());
    }
}
