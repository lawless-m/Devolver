.PHONY: all build install deploy clean

all: build

build:
	cargo build --release

install: build
	install -m755 target/release/devlog ~/.local/bin/devlog

deploy: install
	@if systemctl --user cat devlog-receiver.service >/dev/null 2>&1; then \
		echo "Restarting devlog-receiver.service..."; \
		systemctl --user restart devlog-receiver.service; \
	else \
		echo "devlog-receiver.service not present; binary installed, skipping restart."; \
	fi

clean:
	cargo clean
