RUSTC ?= rustc
LIBS = `pkg-config --libs gtk+-3.0 gstreamer-1.0`
RUSTC_FLAGS = -Z debug-info -Z extra-debug-info --opt-level=0
#RUSTC_FLAGS = -O --cfg ndebug -Z lto

rusttracks: main.rs
	$(RUSTC) --dep-info -L../dumb-gtk -L../rust-http/build $(RUSTC_FLAGS) --link-args "$(LIBS)" "$<"

.PHONY: clean
clean:
	rm -f rusttracks main.d

-include main.d
