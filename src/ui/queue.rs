use cursive::direction::Orientation;
use cursive::event::Key;
use cursive::traits::Boxable;
use cursive::traits::Identifiable;
use cursive::views::*;
use cursive::Cursive;

use std::sync::Arc;
use std::sync::Mutex;

use queue::{Queue, QueueEvent};
use track::Track;
use ui::splitbutton::SplitButton;
use ui::trackbutton::TrackButton;

pub struct QueueView {
    pub view: Option<Panel<BoxView<BoxView<ScrollView<IdView<LinearLayout>>>>>>, // FIXME: wow
    queue: Arc<Mutex<Queue>>,
}

impl QueueView {
    pub fn new(queue: Arc<Mutex<Queue>>) -> QueueView {
        let queuelist = LinearLayout::new(Orientation::Vertical).with_id("queue_list");
        let scrollable = ScrollView::new(queuelist).full_width().full_height();
        let panel = Panel::new(scrollable).title("Queue");

        QueueView {
            view: Some(panel),
            queue: queue,
        }
    }

    fn cb_delete(cursive: &mut Cursive, queue: &mut Queue) {
        let view_ref: Option<ViewRef<LinearLayout>> = cursive.find_id("queue_list");
        if let Some(queuelist) = view_ref {
            let index = queuelist.get_focus_index();
            queue.remove(index);
        }
    }

    fn cb_play(cursive: &mut Cursive, queue: &mut Queue) {
        let view_ref: Option<ViewRef<LinearLayout>> = cursive.find_id("queue_list");
        if let Some(queuelist) = view_ref {
            let index = queuelist.get_focus_index();
            queue.play(index);
        }
    }

    pub fn handle_ev(&self, cursive: &mut Cursive, ev: QueueEvent) {
        let view_ref: Option<ViewRef<LinearLayout>> = cursive.find_id("queue_list");
        if let Some(mut queuelist) = view_ref {
            match ev {
                QueueEvent::Add(index) => {
                    let queue = self.queue.lock().expect("could not lock queue");
                    let track = queue.get(index);
                    let button = self.create_button(&track);
                    queuelist.insert_child(index, button);
                }
                QueueEvent::Remove(index) => {
                    queuelist.remove_child(index);
                }
                QueueEvent::Show => self.populate(&mut queuelist),
            }
        }
    }

    fn create_button(&self, track: &Track) -> SplitButton {
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

        // <enter> plays the selected track
        {
            let queue_ref = self.queue.clone();
            button.add_callback(Key::Enter, move |cursive| {
                Self::cb_play(
                    cursive,
                    &mut queue_ref.lock().expect("could not lock queue"),
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
            let button = self.create_button(track);
            queuelist.add_child(button);
        }
    }
}
