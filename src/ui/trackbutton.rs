use track::Track;
use ui::splitbutton::SplitButton;

pub struct TrackButton {}

impl TrackButton {
    pub fn new(track: &Track) -> SplitButton {
        let button = SplitButton::new(&track.to_string(), &track.duration_str());
        button
    }
}
