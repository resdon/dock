# Makefile for dock
BINARY_NAME=dock
INSTALL_BIN=$(HOME)/.local/bin
DATA_DIR=$(HOME)/.local/share/dock

.PHONY: build install uninstall dist

build:
	@if [ ! -f $(BINARY_NAME) ]; then \
		echo "Compiling $(BINARY_NAME)..."; \
		cargo build --release; \
	fi

install: build
	@echo "Installing $(BINARY_NAME) binary to $(INSTALL_BIN)..."
	mkdir -p $(INSTALL_BIN)
	@if [ -f $(BINARY_NAME) ]; then \
		install -m 755 $(BINARY_NAME) $(INSTALL_BIN)/$(BINARY_NAME); \
	else \
		install -m 755 target/release/$(BINARY_NAME) $(INSTALL_BIN)/$(BINARY_NAME); \
	fi
	
	@echo "Installing assets to $(DATA_DIR)..."
	mkdir -p $(DATA_DIR)/24
	install -m 755 scripts/launcher.sh $(DATA_DIR)/launcher.sh
	install -m 644 assets/font.ttf $(DATA_DIR)/font.ttf
	cp -r assets/24/* $(DATA_DIR)/24/

	@echo "Indexing icon paths..."
	./scripts/list_icons.sh $(DATA_DIR)/icon_list.txt

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

uninstall:
	rm -f $(INSTALL_BIN)/$(BINARY_NAME)
	rm -rf $(DATA_DIR)
	@echo "Uninstallation complete."
