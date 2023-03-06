use std::sync::Arc;

use crate::{
    library::Library,
    queue::Queue,
    traits::{IntoBoxedViewExt, ListItem},
    ui::listview::ListView,
};

#[derive(Clone, Deserialize, Serialize)]
pub struct Category {
    pub id: String,
    pub name: String,
}

impl From<&rspotify::model::Category> for Category {
    fn from(c: &rspotify::model::Category) -> Self {
        Category {
            id: c.id.clone(),
            name: c.name.clone(),
        }
    }
}

impl ListItem for Category {
    fn is_playing(&self, _queue: &Queue) -> bool {
        false
    }

    fn display_left(&self, _library: &Library) -> String {
        self.name.clone()
    }

    fn display_right(&self, _library: &Library) -> String {
        "".to_string()
    }

    fn play(&mut self, _queue: &Queue) {}

    fn play_next(&mut self, _queue: &Queue) {}

    fn queue(&mut self, _queue: &Queue) {}

    fn toggle_saved(&mut self, _library: &Library) {}

    fn save(&mut self, _library: &Library) {}

    fn unsave(&mut self, _library: &Library) {}

    fn open(
        &self,
        queue: Arc<crate::queue::Queue>,
        library: Arc<crate::library::Library>,
    ) -> Option<Box<dyn crate::traits::ViewExt>> {
        let playlists = queue.get_spotify().api.category_playlists(&self.id);
        let view = ListView::new(playlists.items.clone(), queue, library).with_title(&self.name);
        playlists.apply_pagination(view.get_pagination());
        Some(view.into_boxed_view_ext())
    }

    fn share_url(&self) -> Option<String> {
        Some(format!("https://open.spotify.com/genre/{}", self.id))
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
