# Makefile for dock
BINARY_NAME=dock
INSTALL_BIN=$(HOME)/.local/bin
DESKTOP_DIR=$(HOME)/.local/share/applications
AUTOSTART_DIR=$(HOME)/.config/autostart

.PHONY: build install uninstall

build:
	@echo "Compiling dock..."
	cargo build --release

install: build
	@echo "Installing dock to $(INSTALL_BIN)..."
	mkdir -p $(INSTALL_BIN)
	cp target/release/$(BINARY_NAME) $(INSTALL_BIN)/
	
	@echo "Creating desktop entry..."
	mkdir -p $(DESKTOP_DIR)
	echo "[Desktop Entry]\nName=Dock\nExec=$(INSTALL_BIN)/$(BINARY_NAME)\nType=Application\nCategories=Utility;" > $(DESKTOP_DIR)/$(BINARY_NAME).desktop
	
	@echo "Enabling autostart..."
	mkdir -p $(AUTOSTART_DIR)
	ln -sf $(DESKTOP_DIR)/$(BINARY_NAME).desktop $(AUTOSTART_DIR)/$(BINARY_NAME).desktop
	@echo "Installation complete!"

uninstall:
	rm -f $(INSTALL_BIN)/$(BINARY_NAME)
	rm -f $(DESKTOP_DIR)/$(BINARY_NAME).desktop
	rm -f $(AUTOSTART_DIR)/$(BINARY_NAME).desktop
	@echo "Uninstallation complete."
