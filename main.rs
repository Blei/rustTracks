#[feature(globs)];

extern mod extra;
extern mod gtk;
extern mod http;

mod api;
mod gui;
mod player;
mod webinterface;

pub fn main() {
    let mut gui = gui::Gui::new();
    gui.init(std::os::args());

    gui.get_chan().send(gui::FetchPlayToken);
    gui.get_chan().send(gui::GetMixes(~"tags:folk:recent"));

    gui.run();
}
