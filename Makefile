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
	cp target/release/$(BINARY_NAME) $(INSTALL_BIN)/
	
	@echo "Installing assets to $(DATA_DIR)..."
	mkdir -p $(DATA_DIR)
	cp launcher.sh font.ttf $(DATA_DIR)/
	@echo "Installation complete!"

uninstall:
	rm -f $(INSTALL_BIN)/$(BINARY_NAME)
	rm -rf $(DATA_DIR)
	@echo "Uninstallation complete."