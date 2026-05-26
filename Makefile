PREFIX        ?= /usr/local
BIN_DIR       := $(DESTDIR)$(PREFIX)/bin
APP_DIR       := $(DESTDIR)$(PREFIX)/share/applications
ICON_DIR      := $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps
HICOLOR_DIR   := $(DESTDIR)$(PREFIX)/share/icons/hicolor

BINARY        := target/release/melete-app
DESKTOP       := resources/app.melete.desktop
ICON          := resources/icons/app.melete.svg

.PHONY: build install uninstall all

all: build

build:
	cargo build --release -p melete-app

install:
	@test -f $(BINARY) || { echo "Error: $(BINARY) missing. Run 'make build' as your user first."; exit 1; }
	install -Dm755 $(BINARY) $(BIN_DIR)/melete-app
	install -d $(APP_DIR)
	sed 's|^Exec=melete-app|Exec=$(PREFIX)/bin/melete-app|' $(DESKTOP) > $(APP_DIR)/app.melete.desktop
	chmod 644 $(APP_DIR)/app.melete.desktop
	install -Dm644 $(ICON) $(ICON_DIR)/app.melete.svg
	@if command -v update-desktop-database >/dev/null 2>&1; then \
		update-desktop-database $(APP_DIR); \
	fi
	@if command -v gtk-update-icon-cache >/dev/null 2>&1; then \
		gtk-update-icon-cache -f -t $(HICOLOR_DIR); \
	fi

uninstall:
	rm -f $(BIN_DIR)/melete-app
	rm -f $(APP_DIR)/app.melete.desktop
	rm -f $(ICON_DIR)/app.melete.svg
	@if command -v update-desktop-database >/dev/null 2>&1; then \
		update-desktop-database $(APP_DIR); \
	fi
	@if command -v gtk-update-icon-cache >/dev/null 2>&1; then \
		gtk-update-icon-cache -f -t $(HICOLOR_DIR); \
	fi
