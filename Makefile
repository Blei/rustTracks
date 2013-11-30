LIBS = `pkg-config --libs gtk+-3.0 gstreamer-0.10`
RUSTC_FLAGS = -Z debug-info -Z extra-debug-info --opt-level=0

# this is a horrible horrible hack
rusttracks: rusttracks.o
	@# rustc -L../dumb-gtk -L../rust-http/build $(RUSTC_FLAGS) --link-args "$(LIBS)" main.rs -o $@
	cc -L/home/philipp/programming/rust/x86_64-unknown-linux-gnu/stage2/lib/rustc/x86_64-unknown-linux-gnu/lib -m64 -o rusttracks rusttracks.o -L/home/philipp/programming/rust/x86_64-unknown-linux-gnu/stage2/lib/rustc/x86_64-unknown-linux-gnu/lib -lstd-6425b930ca146ae9-0.9-pre -L/home/philipp/programming/rust/x86_64-unknown-linux-gnu/stage2/lib/rustc/x86_64-unknown-linux-gnu/lib -lrustuv-a13edc95d75df17-0.9-pre -L/home/philipp/programming/rust/x86_64-unknown-linux-gnu/stage2/lib/rustc/x86_64-unknown-linux-gnu/lib -lextra-aaa96aab146eb38e-0.9-pre -L../dumb-gtk -lgtk-7f1763ade3219f3b-0.0 -L../rust-http/build -lhttp-1a63dc49587b4f20-0.1-pre -L../dumb-gtk -L../rust-http/build -L/home/philipp/programming/rust-misc/rusttracks/.rust -L/home/philipp/programming/rust-misc/rusttracks -Wl,--as-needed -lrt -ldl -lm -lmorestack -lrustrt -Wl,-rpath,$$ORIGIN/../../rust/x86_64-unknown-linux-gnu/stage2/lib/rustc/x86_64-unknown-linux-gnu/lib -Wl,-rpath,$$ORIGIN/../dumb-gtk -Wl,-rpath,$$ORIGIN/../rust-http/build -Wl,-rpath,/home/philipp/programming/rust/x86_64-unknown-linux-gnu/stage2/lib/rustc/x86_64-unknown-linux-gnu/lib -Wl,-rpath,/home/philipp/programming/rust-misc/dumb-gtk -Wl,-rpath,/home/philipp/programming/rust-misc/rust-http/build -Wl,-rpath,/usr/local/lib/rustc/x86_64-unknown-linux-gnu/lib $(LIBS)

rusttracks.o: main.rs api.rs webinterface.rs gui.rs
	rustc -c -L../dumb-gtk -L../rust-http/build $(RUSTC_FLAGS) --link-args "$(LIBS)" main.rs -o $@

.PHONY: clean
clean:
	rm -f rusttracks{,.o}
