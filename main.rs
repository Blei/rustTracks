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

    let mix_set_json = webinterface::get_mix_set("all");
    let mix_set = api::parse_mix_set_response(&mix_set_json);
    gui.enqueue_message(gui::UpdateMixes(mix_set.contents.mixes));
    let mix_set_json = webinterface::get_mix_set("tags:folk");
    let mix_set = api::parse_mix_set_response(&mix_set_json);
    gui.enqueue_message(gui::UpdateMixes(mix_set.contents.mixes));

    gui.run();
}
