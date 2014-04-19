RUSTC ?= rustc
LIBS = `pkg-config --libs gtk+-3.0 gstreamer-1.0`
RUSTC_FLAGS = -g --opt-level=0
#RUSTC_FLAGS = -O --cfg ndebug -Z lto

rusttracks: main.rs
	$(RUSTC) -o $@ --dep-info -L../dumb-gtk -L../rust-http/build $(RUSTC_FLAGS) -C link-args="$(LIBS)" "$<"

.PHONY: clean
clean:
	rm -f rusttracks rusttracks.d

-include rusttracks.d
