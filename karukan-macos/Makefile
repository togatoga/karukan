OUTDIR = out
APP = $(OUTDIR)/Karukan.app
INSTALL_DIR = $(HOME)/Library/Input Methods
CARGO_MANIFEST = ../Cargo.toml
CARGO_TARGET = ../target

.PHONY: all build build-universal bundle install test clean

all: build

build:
	swift build -c release
	cargo build --release --manifest-path $(CARGO_MANIFEST) -p karukan-im --bin karukan-imserver

build-universal:
	swift build -c release --arch arm64
	swift build -c release --arch x86_64
	mkdir -p .build/release
	lipo -create \
		.build/arm64-apple-macosx/release/KarukanIME \
		.build/x86_64-apple-macosx/release/KarukanIME \
		-output .build/release/KarukanIME
	rustup target add aarch64-apple-darwin x86_64-apple-darwin
	cargo build --release --manifest-path $(CARGO_MANIFEST) -p karukan-im --bin karukan-imserver --target aarch64-apple-darwin
	cargo build --release --manifest-path $(CARGO_MANIFEST) -p karukan-im --bin karukan-imserver --target x86_64-apple-darwin
	lipo -create \
		$(CARGO_TARGET)/aarch64-apple-darwin/release/karukan-imserver \
		$(CARGO_TARGET)/x86_64-apple-darwin/release/karukan-imserver \
		-output $(CARGO_TARGET)/release/karukan-imserver

test:
	swift test
	cargo test --manifest-path $(CARGO_MANIFEST) -p karukan-im

# Assemble the .app bundle in out/ from the built binaries
bundle: build
	rm -rf $(APP)
	mkdir -p $(APP)/Contents/MacOS
	mkdir -p $(APP)/Contents/Resources
	cp Info.plist $(APP)/Contents/
	cp .build/release/KarukanIME $(APP)/Contents/MacOS/
	cp $(CARGO_TARGET)/release/karukan-imserver $(APP)/Contents/MacOS/
	cp -r resources/* $(APP)/Contents/Resources/
	codesign --force --deep --sign - $(APP)

install: bundle
	rm -rf "$(INSTALL_DIR)/Karukan.app"
	cp -a $(APP) "$(INSTALL_DIR)/"
	-killall KarukanIME 2>/dev/null || true
	@echo "Installed to $(INSTALL_DIR)/Karukan.app"
	@echo "First install: log out and back in, then add Karukan in System Settings > Keyboard > Input Sources."

clean:
	rm -rf .build/ $(OUTDIR)/
