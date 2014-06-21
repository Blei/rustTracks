#![feature(globs)]
#![feature(phase)]
#![feature(default_type_params)]
#![feature(unsafe_destructor)]

#[phase(plugin, link)] extern crate log;

extern crate debug;
extern crate libc;
extern crate native;
extern crate serialize;
extern crate sync;
extern crate url;

extern crate gtk;
extern crate http;
extern crate timerfd_source;

use std::comm;
use std::os;

mod api;
mod gui;
mod player;
mod webinterface;

pub fn my_main() {
    let mut gui = gui::Gui::new();
    gui.init(os::args());

    gui.get_sender().send(gui::Notify("Welcome to RustTracks!".to_string()));
    gui.get_sender().send(gui::FetchPlayToken);
    gui.get_sender().send(gui::GetMixes("tags:folk:recent".to_string()));

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
