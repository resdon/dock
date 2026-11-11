# Makefile for dock
BINARY_NAME ?= dock
PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share/dock

# Support standard DESTDIR for packaging
DESTDIR ?=

FINAL_BINDIR = $(DESTDIR)$(BINDIR)
FINAL_DATADIR = $(DESTDIR)$(DATADIR)

.PHONY: all build install uninstall dist clean

all: build

build:
	@if [ ! -f target/release/$(BINARY_NAME) ] && [ ! -f target/debug/$(BINARY_NAME) ]; then \
		echo "Compiling $(BINARY_NAME)..."; \
		cargo build --release; \
	fi

install: build
	@echo "Installing $(BINARY_NAME) binary to $(FINAL_BINDIR)..."
	mkdir -p $(FINAL_BINDIR)
	@if [ -f target/release/$(BINARY_NAME) ]; then \
		install -m 755 target/release/$(BINARY_NAME) $(FINAL_BINDIR)/$(BINARY_NAME); \
	elif [ -f target/debug/$(BINARY_NAME) ]; then \
		install -m 755 target/debug/$(BINARY_NAME) $(FINAL_BINDIR)/$(BINARY_NAME); \
	else \
		echo "Build artifact not found for $(BINARY_NAME)" >&2; \
		exit 1; \
	fi
	
	@echo "Installing assets to $(FINAL_DATADIR)..."
	mkdir -p $(FINAL_DATADIR)/24
	install -m 755 scripts/launcher.sh $(FINAL_DATADIR)/launcher.sh
	install -m 644 assets/font.ttf $(FINAL_DATADIR)/font.ttf
	cp -r assets/24/* $(FINAL_DATADIR)/24/

	@echo "Indexing icon paths..."
	bash ./scripts/list_icons.sh $(FINAL_DATADIR)/icon_list.txt

	@echo "Installation complete!"

dist: build
	@echo "Creating release archive..."
	rm -rf dock-linux-amd64 dock-linux-amd64.tar.gz
	mkdir -p dock-linux-amd64/assets/24 dock-linux-amd64/scripts
	cp target/release/$(BINARY_NAME) dock-linux-amd64/
	cp Makefile dock-linux-amd64/
	cp assets/font.ttf dock-linux-amd64/assets/
	cp assets/24/*.svg dock-linux-amd64/assets/24/
	cp scripts/launcher.sh dock-linux-amd64/scripts/
	cp scripts/list_icons.sh dock-linux-amd64/scripts/
	chmod +x dock-linux-amd64/scripts/*.sh
	tar -czvf dock-linux-amd64.tar.gz dock-linux-amd64
	rm -rf dock-linux-amd64
	@echo "Archive dock-linux-amd64.tar.gz successfully created!"

clean:
	cargo clean
	rm -f dock-linux-amd64.tar.gz

uninstall:
	rm -f $(FINAL_BINDIR)/$(BINARY_NAME)
	rm -rf $(FINAL_DATADIR)
	@echo "Uninstallation complete."
