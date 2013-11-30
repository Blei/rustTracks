#[feature(globs)];

extern mod extra;
extern mod gtk;
extern mod http;

mod api;
mod gui;
mod player;
mod webinterface;

// Yuck, yuck, yuck
static mut global_gui: Option<gui::Gui> = None;

pub fn main() {
    let args = std::os::args();
    let gui = unsafe {
        global_gui = Some(gui::Gui::new());
        global_gui.get_mut_ref()
    };
    gui.init(args);
    gui.start();

    let mix_set_json = webinterface::get_mix_set("all");
    let mix_set = api::parse_mix_set_response(&mix_set_json);
    gui.enqueue_message(gui::UpdateMixes(mix_set.contents.mixes));
    let mix_set_json = webinterface::get_mix_set("tags:folk");
    let mix_set = api::parse_mix_set_response(&mix_set_json);
    gui.enqueue_message(gui::UpdateMixes(mix_set.contents.mixes));
}
