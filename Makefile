PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share
EXT_UUID = fences@ther12k
USER_EXT_DIR = $(HOME)/.local/share/gnome-shell/extensions/$(EXT_UUID)

.PHONY: all build test test-core install install-user-ext test-ext clean

all: build

build:
	cargo build --release -p gnome-fences

test-core:
	cargo test -p gnome-fences-core

test: test-core test-ext

# Install binary + desktop entry system-wide (requires sudo).
install: build
	install -d $(DESTDIR)$(BINDIR)
	install -m 755 target/release/gnome-fences $(DESTDIR)$(BINDIR)/gnome-fences
	install -d $(DESTDIR)$(DATADIR)/applications
	install -m 644 data/io.github.ther12k.Fences.desktop $(DESTDIR)$(DATADIR)/applications/
	install -d $(DESTDIR)$(DATADIR)/gnome-shell/extensions/$(EXT_UUID)
	install -m 644 extension/$(EXT_UUID)/metadata.json $(DESTDIR)$(DATADIR)/gnome-shell/extensions/$(EXT_UUID)/
	install -m 644 extension/$(EXT_UUID)/extension.js $(DESTDIR)$(DATADIR)/gnome-shell/extensions/$(EXT_UUID)/

# Install the extension only into the user's home directory (no sudo needed).
install-user-ext:
	mkdir -p $(USER_EXT_DIR)
	cp extension/$(EXT_UUID)/metadata.json $(USER_EXT_DIR)/
	cp extension/$(EXT_UUID)/extension.js $(USER_EXT_DIR)/
	@echo "Extension installed to $(USER_EXT_DIR)"
	@echo "Enable with: gnome-extensions enable $(EXT_UUID)"

# Check JavaScript syntax with Node.
test-ext:
	node -c extension/$(EXT_UUID)/extension.js
	@echo "Extension JS syntax OK"

clean:
	cargo clean
