use std::sync::Arc;

use queue::Queue;

pub trait ListItem {
    fn is_playing(&self, queue: Arc<Queue>) -> bool;
    fn display_left(&self) -> String;
    fn display_right(&self) -> String;
}
