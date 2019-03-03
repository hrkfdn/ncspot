use cursive::direction::Orientation;
use cursive::event::Key;
use cursive::traits::Boxable;
use cursive::traits::Identifiable;
use cursive::views::*;
use cursive::Cursive;

use std::sync::Arc;
use std::sync::Mutex;

use librespot::core::spotify_id::SpotifyId;
use rspotify::spotify::model::track::FullTrack;

use queue::{Queue, QueueChange};
use spotify::Spotify;
use ui::trackbutton::TrackButton;

pub struct QueueView {
    pub view: Option<Panel<BoxView<BoxView<ScrollView<IdView<LinearLayout>>>>>>, // FIXME: wow
    queue: Arc<Mutex<Queue>>,
    spotify: Arc<Spotify>,
}

impl QueueView {
    pub fn new(queue: Arc<Mutex<Queue>>, spotify: Arc<Spotify>) -> QueueView {
        let queuelist = LinearLayout::new(Orientation::Vertical).with_id("queue_list");
        let scrollable = ScrollView::new(queuelist).full_width().full_height();
        let panel = Panel::new(scrollable).title("Queue");

        QueueView {
            view: Some(panel),
            queue: queue,
            spotify: spotify,
        }
    }

    fn cb_delete(cursive: &mut Cursive, queue: &mut Queue) {
        let view_ref: Option<ViewRef<LinearLayout>> = cursive.find_id("queue_list");
        if let Some(queuelist) = view_ref {
            let index = queuelist.get_focus_index();
            queue.remove(index);
        }
    }

    fn cb_play(cursive: &mut Cursive, queue: &mut Queue, spotify: &Spotify) {
        let view_ref: Option<ViewRef<LinearLayout>> = cursive.find_id("queue_list");
        if let Some(queuelist) = view_ref {
            let index = queuelist.get_focus_index();
            let track = queue.remove(index).expect("could not dequeue track");
            let trackid = SpotifyId::from_base62(&track.id).expect("could not load track");
            spotify.load(trackid);
            spotify.play();
        }
    }

    pub fn handle_ev(&self, cursive: &mut Cursive, ev: QueueChange) {
        let view_ref: Option<ViewRef<LinearLayout>> = cursive.find_id("queue_list");
        if let Some(mut queuelist) = view_ref {
            match ev {
                QueueChange::Enqueue => {
                    let queue = self.queue.lock().expect("could not lock queue");
                    let track = queue.peek().expect("queue is empty");
                    let button = self.create_button(&track);
                    queuelist.insert_child(0, button);
                }
                QueueChange::Dequeue => {
                    queuelist.remove_child(0);
                }
                QueueChange::Remove(index) => {
                    queuelist.remove_child(index);
                }
                QueueChange::Show => self.populate(&mut queuelist),
            }
        }
    }

    fn create_button(&self, track: &FullTrack) -> TrackButton {
        let mut button = TrackButton::new(&track);
        // 'd' deletes the selected track
        {
            let queue_ref = self.queue.clone();
            button.add_callback('d', move |cursive| {
                Self::cb_delete(
                    cursive,
                    &mut queue_ref.lock().expect("could not lock queue"),
                );
            });
        }

        // <enter> dequeues the selected track
        {
            let queue_ref = self.queue.clone();
            let spotify = self.spotify.clone();
            button.add_callback(Key::Enter, move |cursive| {
                Self::cb_play(
                    cursive,
                    &mut queue_ref.lock().expect("could not lock queue"),
                    &spotify,
                );
            });
        }
        button
    }

    pub fn populate(&self, queuelist: &mut LinearLayout) {
        while queuelist.len() > 0 {
            queuelist.remove_child(0);
        }

        let queue = self.queue.lock().expect("could not lock queue");
        for track in queue.iter() {
            let button = self.create_button(&track);
            queuelist.add_child(button);
        }
    }
}
