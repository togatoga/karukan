OUTDIR = out
APP = $(OUTDIR)/Karukan.app
INSTALL_DIR = $(HOME)/Library/Input Methods
CARGO_MANIFEST = ../Cargo.toml
CARGO_TARGET = ../target
DATA_DIR = $(HOME)/Library/Application Support/com.karukan.karukan-im
DICT_URL = https://github.com/togatoga/karukan/releases/latest/download/dict.tgz

.PHONY: all build build-universal bundle install install-dict install-models test fmt lint clean

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

# Download the prebuilt system dictionary unless one is already installed.
# Non-fatal on failure (e.g. offline) — the IME works model-only without it.
install-dict:
	@if [ -f "$(DATA_DIR)/dict.bin" ]; then \
		echo "System dictionary already installed: $(DATA_DIR)/dict.bin (skipping download)"; \
	else \
		echo "Downloading system dictionary from $(DICT_URL) ..."; \
		tmp=$$(mktemp -d) && \
		( curl -fL --progress-bar -o "$$tmp/dict.tgz" "$(DICT_URL)" && \
		  tar xzf "$$tmp/dict.tgz" -C "$$tmp" && \
		  mkdir -p "$(DATA_DIR)" && \
		  cp "$$tmp/dict.bin" "$(DATA_DIR)/" && \
		  echo "Installed system dictionary to $(DATA_DIR)/dict.bin" \
		) || echo "WARNING: system dictionary install failed; install manually (see README)"; \
		rm -rf "$$tmp"; \
	fi

# Prefetch the conversion models into the HuggingFace cache so the first
# launch doesn't block for minutes on the download.
# Non-fatal on failure (e.g. offline) — the engine downloads on first launch.
install-models: build
	@echo "Prefetching conversion models (HuggingFace cache) ..."
	@$(CARGO_TARGET)/release/karukan-imserver --prefetch-models || \
		echo "WARNING: model prefetch failed; models will be downloaded on first launch"

install: bundle install-dict install-models
	rm -rf "$(INSTALL_DIR)/Karukan.app"
	cp -a $(APP) "$(INSTALL_DIR)/"
	-killall KarukanIME 2>/dev/null || true
	@echo "Installed to $(INSTALL_DIR)/Karukan.app"
	@echo "First install: log out and back in, then add Karukan in System Settings > Keyboard > Input Sources."

fmt:
	swift-format format -ri Sources/ Tests/

lint:
	swift-format lint -r Sources/ Tests/

clean:
	rm -rf .build/ $(OUTDIR)/
