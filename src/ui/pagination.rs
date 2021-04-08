use std::sync::{Arc, RwLock};

use crate::library::Library;
use crate::traits::ListItem;

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
}

impl<I: ListItem + Clone> ApiResult<I> {
    pub fn new(limit: u32, fetch_page: Arc<FetchPageFn<I>>) -> ApiResult<I> {
        let items = Arc::new(RwLock::new(Vec::new()));
        if let Some(first_page) = fetch_page(0) {
            items.write().unwrap().extend(first_page.items);
            ApiResult {
                offset: Arc::new(RwLock::new(first_page.offset)),
                limit,
                total: first_page.total,
                items,
                fetch_page: fetch_page.clone(),
            }
        } else {
            ApiResult {
                offset: Arc::new(RwLock::new(0)),
                limit,
                total: 0,
                items,
                fetch_page: fetch_page.clone(),
            }
        }
    }

    fn offset(&self) -> u32 {
        *self.offset.read().unwrap()
    }

    pub fn at_end(&self) -> bool {
        (self.offset() + self.limit as u32) >= self.total
    }

    pub fn apply_pagination(self, pagination: &Pagination<I>) {
        let total = self.total as usize;
        pagination.set(
            total,
            Box::new(move |_| {
                self.next();
            }),
        )
    }

    pub fn next(&self) -> Option<Vec<I>> {
        let offset = self.offset() + self.limit as u32;
        debug!("fetching next page at offset {}", offset);
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

pub struct Pagination<I: ListItem> {
    max_content: Arc<RwLock<Option<usize>>>,
    callback: Arc<RwLock<Option<Paginator<I>>>>,
    busy: Arc<RwLock<bool>>,
}

impl<I: ListItem> Default for Pagination<I> {
    fn default() -> Self {
        Pagination {
            max_content: Arc::new(RwLock::new(None)),
            callback: Arc::new(RwLock::new(None)),
            busy: Arc::new(RwLock::new(false)),
        }
    }
}

// TODO: figure out why deriving Clone doesn't work
impl<I: ListItem> Clone for Pagination<I> {
    fn clone(&self) -> Self {
        Pagination {
            max_content: self.max_content.clone(),
            callback: self.callback.clone(),
            busy: self.busy.clone(),
        }
    }
}

impl<I: ListItem> Pagination<I> {
    pub fn clear(&mut self) {
        *self.max_content.write().unwrap() = None;
        *self.callback.write().unwrap() = None;
    }
    pub fn set(&self, max_content: usize, callback: Paginator<I>) {
        *self.max_content.write().unwrap() = Some(max_content);
        *self.callback.write().unwrap() = Some(callback);
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
                    cb(content);
                    *pagination.busy.write().unwrap() = false;
                    library.trigger_redraw();
                }
            });
        }
    }
}
