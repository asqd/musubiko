DESKTOP := apps/desktop
APP := target/release/bundle/macos/Musubiko.app

.PHONY: test check deps dev app run icon branches sandbox bridge clean

# Rust tests: branch statuses on throwaway repositories built with the real git.
test:
	cargo test --workspace --exclude musubiko-app

# Rust tests plus a TypeScript type check of the UI.
check: test deps
	cd $(DESKTOP) && npm run typecheck

deps:
	cd $(DESKTOP) && [ -d node_modules ] || npm install

# The app with hot reload of the UI.
dev: deps
	cd $(DESKTOP) && npx tauri dev

# Dev only: the UI in a browser against the real core, at http://localhost:1420/bridge.html.
bridge: deps
	cargo build -q -p musubiko-bridge
	(cd $(DESKTOP) && npx vite) & target/debug/musubiko-bridge

# Release build of Musubiko.app (unsigned).
app: deps
	cd $(DESKTOP) && npx tauri build --bundles app

run: app
	open $(APP)

# Redraws the icon from design/make-icon.swift and regenerates the app's icon set.
icon: deps
	swift design/make-icon.swift design/icon_1024.png
	cd $(DESKTOP) && npx tauri icon ../../design/icon_1024.png -o src-tauri/icons
	rm -rf $(DESKTOP)/src-tauri/icons/android $(DESKTOP)/src-tauri/icons/ios

# What the app would show for a repository: make branches REPO=~/work/project [BASE=master]
branches:
	cargo run -q -p musubiko-cli -- branches $(or $(REPO),.) $(if $(BASE),--base $(BASE))

# Test bench from docs/TESTING.md: throwaway repositories in /tmp/musubiko-sandbox.
sandbox:
	scripts/sandbox.sh

clean:
	cargo clean
	rm -rf $(DESKTOP)/dist
