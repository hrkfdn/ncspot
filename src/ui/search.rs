use cursive::direction::Orientation;
use cursive::event::Key;
use cursive::traits::Boxable;
use cursive::traits::Identifiable;
use cursive::views::*;
use cursive::Cursive;
use std::sync::Arc;
use std::sync::Mutex;

use queue::Queue;
use spotify::Spotify;
use track::Track;
use ui::trackbutton::TrackButton;

pub struct SearchView {
    pub view: Panel<LinearLayout>,
    queue: Arc<Mutex<Queue>>,
}

impl SearchView {
    fn search_handler(
        s: &mut Cursive,
        input: &str,
        spotify: Arc<Spotify>,
        queue: Arc<Mutex<Queue>>,
    ) {
        let mut results: ViewRef<ListView> = s.find_id("search_results").unwrap();
        let tracks = spotify.search(input, 50, 0);

        results.clear();

        if let Ok(tracks) = tracks {
            for search_track in tracks.tracks.items {
                let track = Track::new(&search_track);

                let s = spotify.clone();
                let mut button = TrackButton::new(&track);

                // <enter> plays the selected track
                let t = track.clone();
                button.add_callback(Key::Enter, move |_cursive| {
                    s.load(t.clone());
                    s.play();
                });

                // <space> queues the selected track
                let queue = queue.clone();
                button.add_callback(' ', move |_cursive| {
                    let mut queue = queue.lock().unwrap();
                    queue.enqueue(track.clone());
                });

                results.add_child("", button);
            }
        }
    }

    pub fn new(spotify: Arc<Spotify>, queue: Arc<Mutex<Queue>>) -> SearchView {
        let queue_ref = queue.clone();
        let searchfield = EditView::new()
            .on_submit(move |s, input| {
                if input.len() > 0 {
                    Self::search_handler(s, input, spotify.clone(), queue_ref.clone());
                }
            })
            .with_id("search_edit")
            .full_width()
            .fixed_height(1);
        let results = ListView::new().with_id("search_results").full_width();
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
