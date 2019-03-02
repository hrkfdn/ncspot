use cursive::direction::Orientation;
use cursive::event::Key;
use cursive::traits::Boxable;
use cursive::traits::Identifiable;
use cursive::views::*;
use cursive::Cursive;

use std::sync::Arc;
use std::sync::Mutex;

use librespot::core::spotify_id::SpotifyId;

use queue::Queue;
use spotify::Spotify;
use ui::trackbutton::TrackButton;

pub struct QueueView {
    pub view: Option<Panel<LinearLayout>>,
    queue: Arc<Mutex<Queue>>,
    spotify: Arc<Spotify>,
}

impl QueueView {
    pub fn new(queue: Arc<Mutex<Queue>>, spotify: Arc<Spotify>) -> QueueView {
        let queuelist = ListView::new().with_id("queue_list").full_width();
        let scrollable = ScrollView::new(queuelist).full_width().full_height();
        let layout = LinearLayout::new(Orientation::Vertical).child(scrollable);
        let panel = Panel::new(layout).title("Queue");

        QueueView {
            view: Some(panel),
            queue: queue,
            spotify: spotify,
        }
    }

    pub fn redraw(&self, s: &mut Cursive) {
        let view_ref: Option<ViewRef<ListView>> = s.find_id("queue_list");
        if let Some(mut queuelist) = view_ref {
            queuelist.clear();

            let queue_ref = self.queue.clone();
            let queue = self.queue.lock().unwrap();
            for (index, track) in queue.iter().enumerate() {
                let mut button = TrackButton::new(&track);
                let spotify = self.spotify.clone();

                // <enter> dequeues the selected track
                let queue_ref = queue_ref.clone();
                button.add_callback(Key::Enter, move |_cursive| {
                    let track = queue_ref.lock().unwrap().remove(index).expect("could not dequeue track");
                    let trackid = SpotifyId::from_base62(&track.id).expect("could not load track");
                    spotify.load(trackid);
                    spotify.play();
                });

                queuelist.add_child("", button);
            }
        }
    }
}
