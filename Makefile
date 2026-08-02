# Makefile for dock
BINARY_NAME=dock
INSTALL_BIN=$(HOME)/.local/bin
DATA_DIR=$(HOME)/.local/share/dock

.PHONY: build install uninstall

build:
	@echo "Compiling $(BINARY_NAME)..."
	cargo build --release

install: build
	@echo "Installing $(BINARY_NAME) binary to $(INSTALL_BIN)..."
	mkdir -p $(INSTALL_BIN)
	install -m 755 target/release/$(BINARY_NAME) $(INSTALL_BIN)/$(BINARY_NAME)
	
	@echo "Installing assets to $(DATA_DIR)..."
	mkdir -p $(DATA_DIR)/24
	install -m 755 scripts/launcher.sh $(DATA_DIR)/launcher.sh
	install -m 644 assets/font.ttf $(DATA_DIR)/font.ttf
	cp -r assets/24/* $(DATA_DIR)/24/

	@echo "Indexing icon paths..."
	./scripts/list_icons.sh $(DATA_DIR)/icon_list.txt

	@echo "Installation complete!"

uninstall:
	rm -f $(INSTALL_BIN)/$(BINARY_NAME)
	rm -rf $(DATA_DIR)
	@echo "Uninstallation complete."