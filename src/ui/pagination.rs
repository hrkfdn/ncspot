use std::sync::{Arc, RwLock};

use crate::traits::ListItem;

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
    pub fn set(&mut self, max_content: usize, callback: Paginator<I>) {
        *self.max_content.write().unwrap() = Some(max_content);
        *self.callback.write().unwrap() = Some(callback);
    }

    pub fn max_content(&self) -> Option<usize> {
        *self.max_content.read().unwrap()
    }

    fn is_busy(&self) -> bool {
        *self.busy.read().unwrap()
    }

    pub fn call(&self, content: &Arc<RwLock<Vec<I>>>) {
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
                }
            });
        }
    }
}
