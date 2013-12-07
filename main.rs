#[feature(globs)];

extern mod extra;
extern mod gtk;
extern mod http;

mod api;
mod gui;
mod player;
mod timerfd_source;
mod webinterface;

#[start]
pub fn start(argc: int, argv: **u8) -> int {
    std::rt::start_on_main_thread(argc, argv, proc() {
        let mut gui = gui::Gui::new();
        gui.init(std::os::args());

        gui.get_chan().send(gui::Notify(~"Welcome to RustTracks!"));
        gui.get_chan().send(gui::FetchPlayToken);
        gui.get_chan().send(gui::GetMixes(~"tags:folk:recent"));

        gui.run();
    })
}
