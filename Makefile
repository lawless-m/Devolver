.PHONY: all build install clean

all: build

build:
	cargo build --release

install: build
	cp target/release/devlog ~/.local/bin/devlog

clean:
	cargo clean
