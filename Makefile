RUSTC ?= rustc
LIBS = `pkg-config --libs gtk+-3.0 gstreamer-1.0`
RUSTC_FLAGS = -Z debug-info -Z extra-debug-info --opt-level=0

rusttracks: main.rs api.rs webinterface.rs gui.rs player.rs
	$(RUSTC) -L../dumb-gtk -L../rust-http/build $(RUSTC_FLAGS) --link-args "$(LIBS)" main.rs -o $@

.PHONY: clean
clean:
	rm -f rusttracks
