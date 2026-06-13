.PHONY: build bundle run dev test clean icon

# Régénère assets/AppIcon.icns depuis assets/icon.svg.
icon:
	./scripts/make_icon.sh

# Compilation release du binaire.
build:
	cargo build --release

# Assemble dist/Tabs.app (binaire release + Info.plist + signature ad-hoc).
bundle:
	./scripts/bundle.sh

# Build + bundle puis lance l'application packagée.
run: bundle
	open dist/Tabs.app

# Lancement rapide du binaire nu (logs en console), sans bundle.
dev:
	cargo run

test:
	cargo test

clean:
	cargo clean
	rm -rf dist
