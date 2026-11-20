# Safay build commands. A thin layer over cargo and scripts/: those stay the source of truth (docs/DEVELOPMENT.md), so
# every target runs the same command the docs and CI use. `make` alone lists the targets.
SHELL := /bin/bash
.DEFAULT_GOAL := help
.DELETE_ON_ERROR:

PYTHON ?= python3
CARGO ?= cargo
NSH := $(PYTHON) scripts/nsh.py
# No .pyc files: the scripts import each other, which would otherwise leave scripts/__pycache__ in the tree.
export PYTHONDONTWRITEBYTECODE := 1

NAME := $(shell $(PYTHON) -c 'import json; print(json.load(open("nsh.json"))["name"])')
VERSION := $(shell $(PYTHON) -c 'import json; print(json.load(open("nsh.json"))["version"])')
BUILD := $(shell $(PYTHON) -c 'import json; print(json.load(open("nsh.json"))["build"])')

# The native UI test target is the host's own.
UNAME := $(shell uname -s)
ifeq ($(UNAME),Darwin)
  HOST_TARGET := macos
  APP := build/debug/$(NAME).app
  APP_BIN := $(APP)/Contents/MacOS/$(NAME)
  HOST_TRIPLE := $(shell uname -m | sed 's/arm64/aarch64/')-apple-darwin
else ifeq ($(UNAME),Linux)
  HOST_TARGET := linux
  HOST_TRIPLE := $(shell uname -m)-unknown-linux-gnu
else
  HOST_TARGET := windows
  HOST_TRIPLE := x86_64-pc-windows-msvc
endif

.PHONY: help all ci build build-release run preview fmt fmt-check lint nsh-features raycast check test test-ui test-native screenshots \
	package package-adhoc package-release open icloud-check release update-keygen nsh-ready nsh-install nsh-update audit clean clean-all \
	site site-shots site-serve site-deploy

help: ## List the targets
	@echo "$(NAME) $(VERSION) ($(BUILD)) build commands"
	@awk 'BEGIN { FS = ":.*## " } \
		/^##@ / { printf "\n%s\n", substr($$0, 5); next } \
		/^[a-zA-Z0-9_-]+:.*## / { printf "  make %-16s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

##@ Everyday

all: ci package ## Everything CI runs, then the development package

ci: nsh-ready fmt-check lint test check test-ui test-native ## The CI sequence (.github/workflows/desktop.yml) on this host

build: ## Debug build (cargo build)
	$(CARGO) build --locked

build-release: ## Release build of the binary only (no bundle; see package-release)
	$(CARGO) build --release --locked

# Linux: the desktop entry NSH's runner writes takes its icon from `<app ID>.png` beside the executable, where a package
# puts it (scripts/package.py): notifications, the launcher and window lists show it.
ifeq ($(UNAME),Linux)
DEBUG_ICON := target/debug/$(shell $(PYTHON) -c 'import json; print(json.load(open("nsh.json"))["id"])').debug.png
$(DEBUG_ICON): assets/icon.png
	@mkdir -p $(dir $@) && cp $< $@
endif

run: $(DEBUG_ICON) ## Run with the real local vault (debug build; development data directory)
	$(CARGO) run --locked

preview: $(DEBUG_ICON) ## Run with an in-memory vault, discarded on quit; never touches the keychain
	$(CARGO) run --locked -- --preview

##@ Checks and tests

fmt: ## Format the code
	$(CARGO) fmt --all

fmt-check: ## Check formatting only
	$(CARGO) fmt --all -- --check

lint: nsh-features ## clippy, warnings as errors; the launcher integrations' generated text and script are current
	$(CARGO) clippy --workspace --all-targets --locked -- -D warnings
	python3 scripts/integrations.py --check

# The UI tests' hosts and `cargo test` (through dev-dependencies) always have NSH's `svg`; only the app's own build
# shows whether the platform crate turns it on. Without it every icon and the countdown ring draw nothing.
nsh-features: ## the app's build links the NSH features it needs (svg)
	@$(CARGO) tree --locked -e normal,features -i nsh-core --prefix none | grep -q 'nsh-core feature "svg"' \
		|| { echo "NSH is built without svg: add it to the nsh-platform-* features in Cargo.toml" >&2; exit 1; }

raycast: ## build and lint the Raycast / Tinycast extension (integrations/raycast; needs Node)
	python3 scripts/integrations.py
	cd integrations/raycast && npm ci && npx ray build -e dist -o dist && npx eslint src && npx prettier --check src

check: nsh-ready ## nsh check: cargo check plus the TS tests against the generated types
	$(NSH) check

test: ## cargo test: unit, golden, cloud and headless UI tests
	$(CARGO) test --workspace --locked

test-ui: nsh-ready ## nsh test, headless
	$(NSH) test App.test.tsx

test-native: nsh-ready ## nsh test on this host's native UI (opens windows)
	$(NSH) test App.test.tsx --target $(HOST_TARGET)

screenshots: nsh-ready ## Capture every page into docs/screenshots (macOS, takes the foreground; screen must be unlocked)
	$(NSH) test previews/screenshots.test.tsx --target macos --foreground

audit: ## cargo-deny advisories and licenses for this host's platform (needs cargo-deny and the network)
	@command -v cargo-deny >/dev/null || { echo "cargo-deny is not installed: cargo install --locked cargo-deny"; exit 1; }
	@metadata=$$(mktemp -t safay-metadata.XXXXXX) && trap 'rm -f "$$metadata"' EXIT && \
		$(CARGO) metadata --format-version 1 --filter-platform $(HOST_TRIPLE) --locked > "$$metadata" && \
		$(CARGO) deny --metadata-path "$$metadata" --target $(HOST_TRIPLE) --locked check advisories licenses

##@ Packages and release

package: ## Development package in build/debug: Apple Development + profile (iCloud Development), ad-hoc without them
	$(PYTHON) scripts/package.py

package-adhoc: ## Development package signed ad-hoc (no iCloud)
	SAFAY_DEVELOPMENT_IDENTITY=- $(PYTHON) scripts/package.py

package-release: ## Release package and zip in build/release: Developer ID, Hardened Runtime (not notarized)
	$(PYTHON) scripts/package.py --release

open: ## Open the development package (make package first)
	@test -d "$(APP)" || { echo "No package yet: make package"; exit 1; }
	open "$(APP)"

icloud-check: ## The development package's CloudKit access (read-only; prints the account and backup state)
	@test -x "$(APP_BIN)" || { echo "No package yet: make package"; exit 1; }
	"$(APP_BIN)" --icloud-check

release: ## Tests, then a notarized app, zip, DMG, update archive and manifest in build/dist (uploads nothing)
	$(PYTHON) scripts/release.py

update-keygen: nsh-ready ## Create the update-signing key: make update-keygen KEY=~/Documents/Developer/safay-update
	@test -n "$(KEY)" || { echo "Set KEY to the path without extension, outside the repository"; exit 1; }
	@case "$(abspath $(KEY))" in "$(CURDIR)"/*) echo "KEY must be outside the repository"; exit 1;; esac
	$(NSH) keygen "$(KEY)"
	@echo "Next: local.toml [release] update_key = \"$(KEY).secret\"; scripts/package.py UPDATE_PUBLIC_KEY = the .pub content"

##@ Website (safay.gicek.com)

SITE_DIST := build/site/dist
# Pinned so that a deploy never picks up an untested wrangler release. Installed under build/tools/ (npx's cache can
# miss workerd's platform binary); its usage telemetry is turned off.
WRANGLER_VERSION := 4.151.2
WRANGLER_DIR := build/tools/wrangler-$(WRANGLER_VERSION)
WRANGLER := WRANGLER_SEND_METRICS=false "$(CURDIR)/$(WRANGLER_DIR)/node_modules/.bin/wrangler"

site: ## Build the website into build/site/dist (site/build.py, standard library only)
	PYTHONDONTWRITEBYTECODE=1 $(PYTHON) site/build.py

site-shots: ## Refresh site/shots/*.webp and the logo sizes from docs/img and docs/screenshots (needs Pillow)
	PYTHONDONTWRITEBYTECODE=1 $(PYTHON) site/build.py --shots

site-serve: site ## Build and serve the website at http://localhost:8787
	$(PYTHON) -m http.server 8787 --directory "$(SITE_DIST)"

site-deploy: site ## Build and deploy the website to Cloudflare (site/wrangler.toml; signs in through the browser the first time)
	@test -x "$(WRANGLER_DIR)/node_modules/.bin/wrangler" || npm install --prefix "$(WRANGLER_DIR)" --no-save --no-audit --no-fund \
	    "wrangler@$(WRANGLER_VERSION)"
	cd build/site && $(WRANGLER) deploy

##@ NSH and housekeeping

# The targets that run the CLI build it only when there is none for the pinned commit (scripts/nsh.py checks).
nsh-ready:
	@$(NSH) --version >/dev/null 2>&1 || $(PYTHON) scripts/install_nsh.py

nsh-install: ## (Re)build the NSH CLI from the commit Cargo.lock pins
	$(PYTHON) scripts/install_nsh.py

nsh-update: ## Move to NSH main's latest commit, rebuild the CLI; then make all and review the NSH table in docs/ARCHITECTURE.md
	$(CARGO) update -p nsh-core
	$(PYTHON) scripts/install_nsh.py

clean: ## Delete packages and NSH's generated output; keeps compiled dependencies and the NSH CLI
	rm -rf build .nsh/out .nsh/test
	find scripts -name __pycache__ -type d -prune -exec rm -rf {} +

clean-all: clean ## Also cargo clean (the next build recompiles every dependency); the NSH CLI stays
	$(CARGO) clean
