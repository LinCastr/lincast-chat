# Makefile for Lincast Chat (Concord v2)

PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share
APPDIR ?= $(DATADIR)/applications
ICONDIR ?= $(DATADIR)/icons/hicolor/256x256/apps

CARGO ?= cargo
BUILD_FLAGS ?= --release --manifest-path v2/Cargo.toml
TARGET_DIR = v2/target/release

INSTALL = install
INSTALL_PROGRAM = $(INSTALL) -D -m 755
INSTALL_DATA = $(INSTALL) -D -m 644

all: build

build:
	$(CARGO) build $(BUILD_FLAGS)

check:
	desktop-file-validate lincast-chat.desktop
	$(CARGO) clippy --manifest-path v2/Cargo.toml --all-targets -- -D warnings

test:
	$(CARGO) test --manifest-path v2/Cargo.toml

install:
	$(INSTALL_PROGRAM) $(TARGET_DIR)/lincast-chat $(DESTDIR)$(BINDIR)/lincast-chat
	$(INSTALL_PROGRAM) $(TARGET_DIR)/concord-audio $(DESTDIR)$(BINDIR)/concord-audio
	$(INSTALL_DATA) lincast-chat.desktop $(DESTDIR)$(APPDIR)/lincast-chat.desktop
	$(INSTALL_DATA) v2/cliente/assets/icone-256.png $(DESTDIR)$(ICONDIR)/lincast-chat.png
	@if [ -z "$(DESTDIR)" ]; then \
		command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database $(APPDIR) || true; \
		command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -f -t $(DATADIR)/icons/hicolor 2>/dev/null || true; \
	fi

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/lincast-chat
	rm -f $(DESTDIR)$(BINDIR)/concord-audio
	rm -f $(DESTDIR)$(APPDIR)/lincast-chat.desktop
	rm -f $(DESTDIR)$(ICONDIR)/lincast-chat.png
	@if [ -z "$(DESTDIR)" ]; then \
		command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database $(APPDIR) || true; \
		command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -f -t $(DATADIR)/icons/hicolor 2>/dev/null || true; \
	fi

clean:
	$(CARGO) clean --manifest-path v2/Cargo.toml

.PHONY: all build check test install uninstall clean
