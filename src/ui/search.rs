use cursive::direction::Orientation;
use cursive::traits::Boxable;
use cursive::traits::Identifiable;
use cursive::views::*;
use cursive::Cursive;
use std::sync::Arc;

use librespot::core::spotify_id::SpotifyId;

use spotify::Spotify;

pub struct SearchView {
    pub view: Panel<LinearLayout>,
}

impl SearchView {
    pub fn search_handler(s: &mut Cursive, input: &str, spotify: Arc<Spotify>) {
        let mut results: ViewRef<ListView> = s.find_id("search_results").unwrap();
        let tracks = spotify.search(input, 50, 0);

        results.clear();

        if let Ok(tracks) = tracks {
            for track in tracks.tracks.items {
                let s = spotify.clone();
                let trackid = SpotifyId::from_base62(&track.id).expect("could not load track");
                let button = Button::new(track.name, move |_cursive| {
                    s.load(trackid);
                    s.play();
                });
                results.add_child(&track.id, button);
            }
        }
    }

    pub fn new(spotify: Arc<Spotify>) -> SearchView {
        let spotify_ref = spotify.clone();
        let searchfield = EditView::new()
            .on_submit(move |s, input| {
                SearchView::search_handler(s, input, spotify_ref.clone());
            })
            .with_id("search_edit")
            .full_width()
            .fixed_height(1);
        let results = ListView::new().with_id("search_results");
        let scrollable = ScrollView::new(results).full_width().full_height();
        let layout = LinearLayout::new(Orientation::Vertical)
            .child(searchfield)
            .child(scrollable);
        let rootpanel = Panel::new(layout).title("Search");
        return SearchView { view: rootpanel };
    }
}
