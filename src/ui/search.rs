use cursive::direction::Orientation;
use cursive::traits::Boxable;
use cursive::traits::Identifiable;
use cursive::views::*;
use cursive::Cursive;
use std::sync::Arc;
use std::rc::Rc;
use std::cell::RefCell;

use librespot::core::spotify_id::SpotifyId;

use spotify::Spotify;
use queue::Queue;

pub struct SearchView {
    pub view: Panel<LinearLayout>,
    queue: Rc<RefCell<Queue>>,
}

impl SearchView {
    fn search_handler(s: &mut Cursive, input: &str, spotify: Arc<Spotify>, queue: Rc<RefCell<Queue>>) {
        let mut results: ViewRef<ListView> = s.find_id("search_results").unwrap();
        let tracks = spotify.search(input, 50, 0);

        results.clear();

        if let Ok(tracks) = tracks {
            for track in tracks.tracks.items {
                let s = spotify.clone();
                let trackid = SpotifyId::from_base62(&track.id).expect("could not load track");
                let artists = track.artists.iter()
                    .map(|ref artist| artist.name.clone())
                    .collect::<Vec<String>>()
                    .join(", ");
                let formatted = format!("{} - {}", artists, track.name);
                let button = Button::new_raw(formatted, move |_cursive| {
                    s.load(trackid);
                    s.play();
                });
                let p = queue.clone();
                let button_queue = OnEventView::new(button)
                    .on_event(' ', move |_cursive| {
                        p.borrow_mut().enqueue(track.clone());
                        debug!("Added to queue: {}", track.name);
                    });
                results.add_child("", button_queue);
            }
        }
    }

    pub fn new(spotify: Arc<Spotify>, queue: Rc<RefCell<Queue>>) -> SearchView {
        let spotify_ref = spotify.clone();
        let queue_ref = queue.clone();
        let searchfield = EditView::new()
            .on_submit(move |s, input| {
                SearchView::search_handler(s, input, spotify_ref.clone(), queue_ref.clone());
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
        return SearchView {
            view: rootpanel,
            queue: queue,
        };
    }
}
