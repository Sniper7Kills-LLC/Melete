PREFIX        := $(HOME)/.local
BIN_DIR       := $(PREFIX)/bin
APP_DIR       := $(PREFIX)/share/applications
ICON_DIR      := $(PREFIX)/share/icons/hicolor/scalable/apps

BINARY        := target/release/journal-app
DESKTOP       := resources/dev.s7k.journal.desktop
ICON          := resources/icons/dev.s7k.journal.svg

.PHONY: build install uninstall

build:
	cargo build --release -p journal-app

install: build
	install -Dm755 $(BINARY) $(BIN_DIR)/journal-app
	install -Dm644 $(DESKTOP) $(APP_DIR)/dev.s7k.journal.desktop
	install -Dm644 $(ICON) $(ICON_DIR)/dev.s7k.journal.svg
	@if command -v update-desktop-database >/dev/null 2>&1; then \
		update-desktop-database $(APP_DIR); \
	fi
	@if command -v gtk-update-icon-cache >/dev/null 2>&1; then \
		gtk-update-icon-cache -f -t $(PREFIX)/share/icons/hicolor; \
	fi

uninstall:
	rm -f $(BIN_DIR)/journal-app
	rm -f $(APP_DIR)/dev.s7k.journal.desktop
	rm -f $(ICON_DIR)/dev.s7k.journal.svg
	@if command -v update-desktop-database >/dev/null 2>&1; then \
		update-desktop-database $(APP_DIR); \
	fi
	@if command -v gtk-update-icon-cache >/dev/null 2>&1; then \
		gtk-update-icon-cache -f -t $(PREFIX)/share/icons/hicolor; \
	fi
