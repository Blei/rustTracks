#![feature(globs)]
#![feature(phase)]
#![feature(default_type_params)]

#[phase(syntax, link)] extern crate log;

extern crate libc;
extern crate native;
extern crate serialize;
extern crate sync;
extern crate url;

extern crate gtk;
extern crate http;

use std::comm;
use std::os;

mod api;
mod gui;
mod player;
mod timerfd_source;
mod webinterface;

pub fn my_main() {
    let mut gui = gui::Gui::new();
    gui.init(os::args());

    gui.get_sender().send(gui::Notify(~"Welcome to RustTracks!"));
    gui.get_sender().send(gui::FetchPlayToken);
    gui.get_sender().send(gui::GetMixes(~"tags:folk:recent"));

    gui.run();
}

pub fn main() {
    let (sender, receiver) = comm::channel();
    spawn(proc() {
        my_main();
        sender.send(1);
    });
    receiver.recv();
}

#[start]
fn start(argc: int, argv: **u8) -> int { native::start(argc, argv, main) }
